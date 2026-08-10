//! Состояние приложения, разделяемое хендлерами.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tokio::sync::broadcast;
use tou_db::Db;
use tou_domain::auction::DEFAULT_DURATION_MINUTES;
use tou_ports::notifications::{NotificationEnvelope, NotificationPublisher};
use uuid::Uuid;

use crate::oidc::OidcProvider;
use crate::ratelimit::RateLimiter;
use crate::realtime::{Bus, BusMessage};
use crate::storage::Storage;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub storage: Storage,
    pub notifier: Notifier,
    pub auction_hub: AuctionHub,
    /// Счетчик попыток на дологинных маршрутах (NFR-07); без Redis - пустая
    /// проверка
    pub rate_limit: RateLimiter,
    /// Ставить ли `Secure` на выдаваемые cookie. Значение одно на все cookie
    /// приложения: сессионную ставит слой tower-sessions, CSRF-токен -
    /// [`crate::csrf::issue`], и разъехаться они не должны
    pub secure_cookies: bool,
    /// Длительность торгов от объявления старта (FR-602, п. 66); демо-режим
    /// укорачивает ее параметром запуска api
    pub auction_minutes: i64,
    /// Внешний провайдер идентичности (FR-1502, ADR-0003); `None` - стенд
    /// работает на локальном входе контура 1
    pub oidc: Option<Arc<OidcProvider>>,
}

impl AppState {
    pub fn new(db: Db, storage: Storage) -> Self {
        Self {
            db,
            storage,
            notifier: Notifier::new(),
            auction_hub: AuctionHub::default(),
            rate_limit: RateLimiter::default(),
            secure_cookies: false,
            auction_minutes: DEFAULT_DURATION_MINUTES,
            oidc: None,
        }
    }

    /// `COOKIE_SECURE=1` в проде (за Caddy TLS).
    pub fn with_secure_cookies(mut self, secure: bool) -> Self {
        self.secure_cookies = secure;
        self
    }

    /// Демо-режим торгов: `api --demo-timer 5m` вместо часа (задача Т11).
    pub fn with_auction_minutes(mut self, minutes: i64) -> Self {
        self.auction_minutes = minutes;
        self
    }

    /// Внешний провайдер идентичности (FR-1502): `None` - вход только локальный.
    pub fn with_oidc(mut self, provider: Option<Arc<OidcProvider>>) -> Self {
        self.oidc = provider;
        self
    }

    /// Счетчик попыток входа в том же Redis, что и сессии (NFR-12): при двух
    /// экземплярах api лимит обязан быть общим.
    pub fn with_rate_limit(mut self, pool: crate::realtime::Pool) -> Self {
        self.rate_limit = RateLimiter::new(pool);
        self
    }

    /// Подключение шины реалтайма (T58, NFR-12): публикация уходит в Redis,
    /// а подписка возвращает события всем экземплярам api, включая этот.
    /// Обработчики об этом не знают - они по-прежнему зовут `notifier.publish`
    /// и `auction_hub.publish`.
    pub async fn attach_realtime(
        &self,
        pool: &crate::realtime::Pool,
        redis_url: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let notifier = self.notifier.clone();
        let auction_hub = self.auction_hub.clone();

        crate::realtime::subscribe(redis_url, move |message| match message {
            BusMessage::Notification { user_id, event } => notifier.deliver(user_id, event.into()),
            BusMessage::Auction { auction_id, event } => {
                auction_hub.deliver(auction_id, event.into())
            }
        })
        .await?;

        let bus = crate::realtime::Bus::new(pool.clone());
        self.notifier.attach(bus.clone());
        self.auction_hub.attach(bus);
        Ok(())
    }
}

/// Комнаты торгов (FR-603): на каждый аукцион - свой broadcast, подписчики
/// получают события ленты и смены состояния.
///
/// Доставка до сокета всегда локальная - это самый короткий путь. Шина
/// (T58, NFR-12) отвечает только за то, чтобы событие дошло до остальных
/// экземпляров api: с ней публикация уходит в Redis и возвращается всем,
/// включая этот процесс, поэтому путь доставки остается один и дублей нет.
#[derive(Clone, Default)]
pub struct AuctionHub {
    rooms: Arc<Mutex<HashMap<Uuid, broadcast::Sender<Arc<str>>>>>,
    bus: Arc<OnceLock<Bus>>,
}

impl AuctionHub {
    pub fn subscribe(&self, auction_id: Uuid) -> broadcast::Receiver<Arc<str>> {
        match self.rooms.lock() {
            Ok(mut rooms) => rooms
                .entry(auction_id)
                .or_insert_with(|| broadcast::channel(256).0)
                .subscribe(),
            Err(error) => {
                tracing::error!(%error, %auction_id, "auction hub lock poisoned");
                // Отвалившийся отправитель закроет поток - клиент переподключится
                broadcast::channel(1).1
            }
        }
    }

    /// Событие сериализуется один раз на всю комнату.
    pub fn publish<T: Serialize>(&self, auction_id: Uuid, event: &T) {
        let json = match serde_json::to_string(event) {
            Ok(json) => json,
            Err(error) => {
                tracing::error!(%error, %auction_id, "serialize auction event");
                return;
            }
        };

        match self.bus.get() {
            Some(bus) => bus.publish(BusMessage::Auction {
                auction_id,
                event: json,
            }),
            None => self.deliver(auction_id, json.into()),
        }
    }

    /// Доставка подписчикам этого процесса: конец пути как без шины, так и с
    /// ней - с шиной сюда приходит уже вернувшееся из Redis событие.
    pub fn deliver(&self, auction_id: Uuid, json: Arc<str>) {
        match self.rooms.lock() {
            // Err у send = комната пуста: доставка не требуется
            Ok(rooms) => {
                if let Some(room) = rooms.get(&auction_id) {
                    let _ = room.send(json);
                }
            }
            Err(error) => tracing::error!(%error, %auction_id, "auction hub lock poisoned"),
        }
    }

    /// Подключение шины: до вызова комнаты работают в пределах процесса.
    pub fn attach(&self, bus: Bus) {
        let _ = self.bus.set(bus);
    }
}

/// Событие центра уведомлений для SSE-доставки (FR-1301): JSON
/// `NotificationDto` сериализуется один раз при публикации.
#[derive(Clone)]
pub struct NotificationEvent {
    pub user_id: Uuid,
    pub json: Arc<str>,
}

/// Разветвитель SSE-доставки: хендлер, записавший уведомление в БД,
/// публикует событие; стримы получателей фильтруют по `user_id`.
/// Контур 1 - один процесс api, in-process broadcast (A-023); при
/// масштабировании внутренности заменяются на Redis pub/sub (арх. § 5)
/// без изменения вызывающих.
#[derive(Clone)]
pub struct Notifier {
    tx: broadcast::Sender<NotificationEvent>,
    bus: Arc<OnceLock<Bus>>,
}

impl Notifier {
    pub fn new() -> Self {
        // Емкость на всплеск рассылки; отставший подписчик получит Lagged
        // и дотянет пропущенное запросом истории
        let (tx, _) = broadcast::channel(256);
        Self {
            tx,
            bus: Arc::new(OnceLock::new()),
        }
    }

    pub fn publish(&self, user_id: Uuid, json: String) {
        match self.bus.get() {
            Some(bus) => bus.publish(BusMessage::Notification {
                user_id,
                event: json,
            }),
            None => self.deliver(user_id, json.into()),
        }
    }

    /// Доставка стримам этого процесса (см. [`AuctionHub::deliver`]).
    pub fn deliver(&self, user_id: Uuid, json: Arc<str>) {
        // Err = нет ни одного открытого стрима - доставка не требуется
        let _ = self.tx.send(NotificationEvent { user_id, json });
    }

    /// Подключение шины: до вызова доставка идет только внутри процесса.
    pub fn attach(&self, bus: Bus) {
        let _ = self.bus.set(bus);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<NotificationEvent> {
        self.tx.subscribe()
    }
}

impl Default for Notifier {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationPublisher for Notifier {
    fn publish(&self, notification: &NotificationEnvelope) {
        match serde_json::to_string(&notification.event) {
            Ok(json) => self.publish(notification.recipient_id, json),
            Err(error) => {
                tracing::error!(%error, notification_id = %notification.event.id, "serialize notification event");
            }
        }
    }
}
