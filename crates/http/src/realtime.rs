//! Разнос эфемерных событий между экземплярами api (T58, NFR-12).
//!
//! Лента торгов (FR-603) и центр уведомлений (FR-1301) доставляются
//! подписчикам через in-process broadcast: это самый быстрый путь до сокета,
//! уже открытого этим процессом. Пока процесс один, этого достаточно — но
//! второй экземпляр api своих подписчиков о чужих событиях не узнает, и
//! `api` оставался ограничен вертикальным масштабированием (A-023).
//!
//! Шина закрывает ровно это: публикация уходит в Redis, а обратно приходит
//! всем экземплярам, включая опубликовавший. Путь доставки остается один —
//! локальный broadcast, — поэтому дублей не возникает и «свои» события не
//! требуют особого случая.
//!
//! Потеря эфемерного события не теряет факта: уведомление уже записано в
//! `core.notifications`, ставка — в `core.bids`. Клиент дотянет пропущенное
//! историей и снимком комнаты (NFR-05), поэтому ошибка шины логируется,
//! но ничего не откатывает.

use fred::clients::SubscriberClient;
use fred::prelude::{ClientLike, Config, EventInterface, PubsubInterface};
use serde::{Deserialize, Serialize};

pub use fred::prelude::Pool;
use uuid::Uuid;

/// Канал уведомлений центра (FR-1301).
const NOTIFY_CHANNEL: &str = "tou:notify";
/// Канал лент аукционных комнат (FR-603): комната опознается полем сообщения,
/// а не отдельным каналом — подписка не меняется при открытии новой комнаты.
const AUCTION_CHANNEL: &str = "tou:auction";

/// Сообщение шины. Полезная нагрузка — уже сериализованное событие: она
/// одинакова для всех получателей, и второй разбор ей не нужен.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BusMessage {
    Notification { user_id: Uuid, event: String },
    Auction { auction_id: Uuid, event: String },
}

impl BusMessage {
    fn channel(&self) -> &'static str {
        match self {
            BusMessage::Notification { .. } => NOTIFY_CHANNEL,
            BusMessage::Auction { .. } => AUCTION_CHANNEL,
        }
    }
}

/// Издатель шины. Клонируется вместе с состоянием приложения.
#[derive(Clone)]
pub struct Bus {
    pool: Pool,
}

impl Bus {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Публикация не ждет доставки: вызывающий — обработчик, уже записавший
    /// факт в БД, и его транзакция не должна зависеть от Redis.
    pub fn publish(&self, message: BusMessage) {
        let pool = self.pool.clone();
        tokio::spawn(async move {
            let channel = message.channel();
            let payload = match serde_json::to_string(&message) {
                Ok(payload) => payload,
                Err(error) => {
                    tracing::error!(%error, "сериализация сообщения шины");
                    return;
                }
            };
            // Публикует обычный клиент пула: подписанное соединение
            // других команд не выполняет, поэтому подписчик — отдельный
            if let Err(error) = pool
                .next_connected()
                .publish::<(), _, _>(channel, payload)
                .await
            {
                tracing::error!(%error, channel, "публикация в шину реалтайма");
            }
        });
    }
}

/// Подключение к Redis для сессий и шины: один пул на процесс.
pub async fn connect(redis_url: &str) -> Result<Pool, Box<dyn std::error::Error + Send + Sync>> {
    let config = Config::from_url(redis_url)?;
    let pool = Pool::new(config, None, None, None, 6)?;
    pool.init().await?;
    Ok(pool)
}

/// Подписчик шины: отдельное соединение, потому что подписанное соединение
/// в Redis не выполняет обычных команд. Пересоздание подписок после обрыва
/// берет на себя `manage_subscriptions`.
pub async fn subscribe(
    redis_url: &str,
    mut deliver: impl FnMut(BusMessage) + Send + 'static,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = Config::from_url(redis_url)?;
    let client = SubscriberClient::new(config, None, None, None);
    client.init().await?;
    client.manage_subscriptions();
    client
        .subscribe(vec![NOTIFY_CHANNEL, AUCTION_CHANNEL])
        .await?;

    let mut messages = client.message_rx();
    tokio::spawn(async move {
        // Клиент держится живым вместе с задачей: его Drop закрывает подписку
        let _client = client;
        loop {
            match messages.recv().await {
                Ok(message) => match message.value.as_string() {
                    Some(payload) => match serde_json::from_str::<BusMessage>(&payload) {
                        Ok(parsed) => deliver(parsed),
                        Err(error) => {
                            tracing::error!(%error, channel = %message.channel, "разбор сообщения шины")
                        }
                    },
                    None => tracing::error!(channel = %message.channel, "сообщение шины не строка"),
                },
                // Отставание означает всплеск: пропущенное клиент дотянет
                // историей уведомлений и снимком комнаты (NFR-05)
                Err(error) => tracing::warn!(%error, "поток шины реалтайма прерван"),
            }
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Формат сообщения — контракт между экземплярами api: разные версии
    /// приложения могут какое-то время работать рядом при выкатке.
    #[test]
    fn message_round_trips_through_the_wire_format() {
        let user_id = Uuid::now_v7();
        let message = BusMessage::Notification {
            user_id,
            event: r#"{"kind":"auction_invitation"}"#.to_owned(),
        };

        let wire = serde_json::to_string(&message).expect("сериализация");
        assert!(wire.contains("\"kind\":\"notification\""), "{wire}");

        match serde_json::from_str::<BusMessage>(&wire).expect("разбор") {
            BusMessage::Notification {
                user_id: got,
                event,
            } => {
                assert_eq!(got, user_id);
                assert!(event.contains("auction_invitation"));
            }
            other => panic!("не то сообщение: {other:?}"),
        }
    }

    #[test]
    fn each_message_goes_to_its_own_channel() {
        let notification = BusMessage::Notification {
            user_id: Uuid::now_v7(),
            event: String::new(),
        };
        let auction = BusMessage::Auction {
            auction_id: Uuid::now_v7(),
            event: String::new(),
        };

        assert_eq!(notification.channel(), NOTIFY_CHANNEL);
        assert_eq!(auction.channel(), AUCTION_CHANNEL);
        assert_ne!(notification.channel(), auction.channel());
    }
}
