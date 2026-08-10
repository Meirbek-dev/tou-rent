//! Шина реалтайма между экземплярами api (T58, NFR-12) против живого Redis.
//!
//! Проверяется ровно то свойство, ради которого шина заведена: событие,
//! опубликованное одним экземпляром, доходит до подписчика другого. Два
//! экземпляра здесь — два независимых `Notifier`/`AuctionHub`, каждый со
//! своим локальным broadcast и своей подпиской: для Redis это и есть два
//! разных процесса, а внутрипроцессный broadcast между ними не связывает
//! ничего.
//!
//! Подключение — REDIS_URL; без него тест пропускается (как testkit, A-021).

use std::time::Duration;

use tou_http::realtime;
use tou_http::state::{AuctionHub, Notifier};
use uuid::Uuid;

fn redis_url() -> Option<String> {
    std::env::var("REDIS_URL").ok()
}

macro_rules! require_redis {
    () => {
        match redis_url() {
            Some(url) => url,
            None => {
                eprintln!("SKIP: REDIS_URL не задан — шина реалтайма не проверялась");
                return;
            }
        }
    };
}

type Fallible<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Экземпляр api в миниатюре: локальные разветвители плюс подписка на шину.
async fn instance(url: &str) -> Fallible<(Notifier, AuctionHub)> {
    let notifier = Notifier::new();
    let auction_hub = AuctionHub::default();

    let (n, a) = (notifier.clone(), auction_hub.clone());
    realtime::subscribe(url, move |message| match message {
        realtime::BusMessage::Notification { user_id, event } => n.deliver(user_id, event.into()),
        realtime::BusMessage::Auction { auction_id, event } => a.deliver(auction_id, event.into()),
    })
    .await?;

    let bus = realtime::Bus::new(realtime::connect(url).await?);
    notifier.attach(bus.clone());
    auction_hub.attach(bus);

    Ok((notifier, auction_hub))
}

/// Ожидание с потолком: доставка идет через сеть, но секунды здесь — уже
/// не «эфемерная доставка», а сломанная шина.
async fn within<T>(future: impl Future<Output = T>) -> Option<T> {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .ok()
}

/// NFR-12: уведомление, опубликованное одним экземпляром, доходит до стрима,
/// открытого на другом.
#[tokio::test]
async fn notification_crosses_instances() {
    let url = require_redis!();
    let (publisher, _) = instance(&url).await.expect("экземпляр-издатель");
    let (subscriber, _) = instance(&url).await.expect("экземпляр-подписчик");

    let mut stream = subscriber.subscribe();
    let user_id = Uuid::now_v7();
    publisher.publish(user_id, r#"{"kind":"auction_invitation"}"#.to_owned());

    let event = within(async {
        loop {
            let event = stream.recv().await.expect("поток уведомлений");
            if event.user_id == user_id {
                return event;
            }
        }
    })
    .await
    .expect("уведомление между экземплярами не дошло");

    assert!(
        event.json.contains("auction_invitation"),
        "дошло не то событие: {}",
        event.json
    );
}

/// FR-603, NFR-12: ставка, принятая одним экземпляром, попадает в ленту
/// комнаты, открытой на другом.
#[tokio::test]
async fn auction_feed_crosses_instances() {
    let url = require_redis!();
    let (_, publisher) = instance(&url).await.expect("экземпляр-издатель");
    let (_, subscriber) = instance(&url).await.expect("экземпляр-подписчик");

    let auction_id = Uuid::now_v7();
    let mut room = subscriber.subscribe(auction_id);
    publisher.publish(
        auction_id,
        &serde_json::json!({ "kind": "bid", "amount": "79750" }),
    );

    let event = within(room.recv())
        .await
        .expect("лента торгов между экземплярами не дошла")
        .expect("лента комнаты");

    assert!(event.contains("79750"), "дошло не то событие: {event}");
}

/// Публикующий экземпляр получает свое же событие тем же путем, что и чужое:
/// доставка одна, значит дублей нет и особого случая «свое» не существует.
#[tokio::test]
async fn publisher_receives_its_own_event_once() {
    let url = require_redis!();
    let (notifier, _) = instance(&url).await.expect("экземпляр");

    let mut stream = notifier.subscribe();
    let user_id = Uuid::now_v7();
    notifier.publish(user_id, r#"{"kind":"fee_confirmed"}"#.to_owned());

    within(async {
        loop {
            let event = stream.recv().await.expect("поток уведомлений");
            if event.user_id == user_id {
                return;
            }
        }
    })
    .await
    .expect("собственное событие не дошло");

    // Второго такого же события быть не должно
    let extra = tokio::time::timeout(Duration::from_millis(700), async {
        loop {
            let event = stream.recv().await.expect("поток уведомлений");
            if event.user_id == user_id {
                return event;
            }
        }
    })
    .await;
    assert!(extra.is_err(), "событие продублировалось");
}

/// Без шины разветвители работают в пределах процесса: дев-стенд и тесты
/// не обязаны поднимать Redis, чтобы уведомления доходили.
#[tokio::test]
async fn works_without_the_bus() {
    let notifier = Notifier::new();
    let mut stream = notifier.subscribe();
    let user_id = Uuid::now_v7();

    notifier.publish(user_id, r#"{"kind":"local"}"#.to_owned());

    let event = within(stream.recv())
        .await
        .expect("локальная доставка не сработала")
        .expect("поток уведомлений");
    assert_eq!(event.user_id, user_id);
}
