//! Автозавершение торгов по истечении таймера (INV-066, FR-606, п. 66, 68)
//! против живой БД.
//!
//! Ставку после `ends_at` БД не принимает и без этого прохода, но комната
//! оставалась в статусе `running`: победитель не определен, срок протокола
//! итогов (п. 73) не открыт, а участники видели таймер на нуле и ничего
//! больше. Завершал торги только председатель нажатием - если он его не
//! делал, процесс останавливался совсем.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use rust_decimal::Decimal;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Сдвигающие тесты идут по одному - строка сдвига в базе одна (ADR-0005,
/// та же причина, что в `controllable_time`).
static CLOCK: Mutex<()> = Mutex::const_new(());

/// Пул владельца: сдвиг часов роли приложения недоступен, а ставку в
/// истекшие торги не пропустит триггер - «время вышло» приходится
/// изображать часами, а не датой в будущем.
async fn try_pool() -> Result<Option<sqlx::PgPool>, sqlx::Error> {
    let required =
        tou_testkit::database_url().map_err(|e| sqlx::Error::Configuration(Box::new(e)))?;
    let Some(url) = required else {
        return Ok(None);
    };
    let owner = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await?;
    Ok(Some(owner))
}

macro_rules! require_db {
    () => {
        match try_pool()
            .await
            .expect("TESTKIT_DATABASE_URL: подключение не удалось")
        {
            Some(db) => db,
            None => {
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - истечение торгов не проверялось");
                return;
            }
        }
    };
}

struct Fixture {
    auction_id: Uuid,
    application_id: Uuid,
    tender_id: Uuid,
}

/// Идущие торги с одной допущенной заявкой и одной ставкой; таймер
/// заканчивается через час. «Время вышло» изображается сдвигом часов.
async fn fixture(tx: &mut sqlx::PgConnection) -> Result<Fixture, sqlx::Error> {
    let organizer = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'T82 организатор', core.now()) RETURNING id",
        format!("t82-org-{}@tou.test", Uuid::now_v7().simple())
    )
    .fetch_one(&mut *tx)
    .await?;

    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id)
         VALUES ('T82 тендер', 'trading', $1) RETURNING id",
        organizer
    )
    .fetch_one(&mut *tx)
    .await?;

    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'T82 объект', 'адрес', 10.00) RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await?;

    let lot_id = sqlx::query_scalar!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                base_rate_monthly, guarantee_fee, rate_calculation)
         VALUES ($1, 1, $2, 'офис', 12, 50000.00, 50000.00, '{}'::jsonb) RETURNING id",
        tender_id,
        object_id
    )
    .fetch_one(&mut *tx)
    .await?;

    let participant = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'T82 участник', core.now()) RETURNING id",
        format!("t82-p-{}@tou.test", Uuid::now_v7().simple())
    )
    .fetch_one(&mut *tx)
    .await?;

    let application_id = sqlx::query_scalar!(
        "INSERT INTO core.applications
           (tender_id, lot_id, participant_id, status, applicant_kind, applicant_details)
         VALUES ($1, $2, $3, 'admitted', 'legal_entity', '{}'::jsonb) RETURNING id",
        tender_id,
        lot_id,
        participant
    )
    .fetch_one(&mut *tx)
    .await?;

    let auction_id = sqlx::query_scalar!(
        "INSERT INTO core.auctions (lot_id, starting_bid, bid_step, status, started_at, ends_at,
                                    current_turn_application_id)
         VALUES ($1, 55000.00, 2750.00, 'running', core.now(),
                 core.now() + interval '1 hour', $2)
         RETURNING id",
        lot_id,
        application_id
    )
    .fetch_one(&mut *tx)
    .await?;

    // Ставка кладется напрямую: правило шага проверяет свой тест (INV-063),
    // здесь важен только итог, который домен посчитает по ленте.
    // Идентификатор ставки задает клиент (NFR-05), умолчания у него нет
    sqlx::query!(
        "INSERT INTO core.bids (id, auction_id, application_id, amount)
         VALUES ($1, $2, $3, $4)",
        Uuid::now_v7(),
        auction_id,
        application_id,
        Decimal::from(57_750)
    )
    .execute(&mut *tx)
    .await?;

    Ok(Fixture {
        auction_id,
        application_id,
        tender_id,
    })
}

async fn status_of(tx: &mut sqlx::PgConnection, auction_id: Uuid) -> Result<String, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT status::text AS "status!" FROM core.auctions WHERE id = $1"#,
        auction_id
    )
    .fetch_one(tx)
    .await
}

/// FR-606 (п. 66, 68): торги с истекшим таймером закрываются сами, победитель
/// определяется по ленте ставок, и открывается срок протокола итогов (п. 73).
#[tokio::test]
async fn expired_auction_is_finished_with_a_winner() {
    let _serialized = CLOCK.lock().await;
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");

    let f = fixture(&mut tx).await.expect("fixture");

    // Час прошел. Сдвиг виден только этой транзакции и откатывается с ней
    sqlx::query!("UPDATE refdata.clock_offset SET shift = interval '2 hours' WHERE id")
        .execute(&mut *tx)
        .await
        .expect("сдвиг часов");

    let finished = tou_db::auctions::finish_expired_on(&mut tx, 32)
        .await
        .expect("проход по истекшим торгам");

    // На стенде могут идти и чужие торги: проверяется свой аукцион,
    // а не то, что он единственный в выборке
    let record = finished
        .iter()
        .find(|a| a.id == f.auction_id)
        .expect("истекшие торги обязаны закрыться");
    assert_eq!(
        record.winner_application_id,
        Some(f.application_id),
        "победитель берется из ленты ставок (FR-606)"
    );
    assert!(
        !record.finished_early,
        "время вышло - это не досрочное завершение по согласию (п. 67)"
    );
    assert_eq!(
        status_of(&mut tx, f.auction_id).await.expect("статус"),
        "finished"
    );

    let obligations = sqlx::query_scalar!(
        r#"SELECT count(*) AS "obligations!" FROM core.obligations
          WHERE tender_id = $1 AND action = 'results_protocol'"#,
        f.tender_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("обязательства");
    assert_eq!(
        obligations, 1,
        "завершение торгов открывает срок протокола итогов (п. 73, FR-1702)"
    );
}

/// Идущие торги проход не трогает: закрывать по таймеру можно только то,
/// у чего таймер истек.
#[tokio::test]
async fn running_auction_is_left_alone() {
    let _serialized = CLOCK.lock().await;
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");

    let f = fixture(&mut tx).await.expect("fixture");

    let finished = tou_db::auctions::finish_expired_on(&mut tx, 32)
        .await
        .expect("проход по истекшим торгам");
    assert!(
        finished.iter().all(|a| a.id != f.auction_id),
        "торги с живым таймером закрывать нельзя"
    );
    assert_eq!(
        status_of(&mut tx, f.auction_id).await.expect("статус"),
        "running"
    );
}

/// Одна испорченная комната не отменяет проход (круг 2 гаунтлета, G-028).
///
/// Раньше `finish_expired` закрывал до 32 комнат одной транзакцией: отказ
/// на любой из них откатывал и все уже закрытые, а воркер каждую минуту
/// подбирал ту же пачку и снова упирался в ту же комнату. Итог - площадка
/// не закрывала торги вовсе, пока кто-нибудь не разберет испорченную
/// строку руками.
///
/// Испорченная комната здесь настоящая, а не выдуманная: в ленте торгов по
/// второму лоту лежит ставка заявки первого лота. Домен объявит ее
/// победителем, а составной ключ `auctions_winner_application_lot_fkey`
/// (победитель принадлежит лоту этих торгов) такую запись не примет.
#[tokio::test]
async fn a_poisoned_auction_does_not_take_the_batch_down_with_it() {
    let _serialized = CLOCK.lock().await;
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");

    let healthy = fixture(&mut tx).await.expect("исправные торги");

    // Второй лот того же тендера со своими торгами. Таймер у них истекает
    // раньше - проход дойдет до них первыми, и закрытие исправных торгов
    // случится уже после отказа
    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'T82 объект (испорченный лот)', 'адрес', 12.00) RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await
    .expect("объект");

    let lot_id = sqlx::query_scalar!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                base_rate_monthly, guarantee_fee, rate_calculation)
         VALUES ($1, 2, $2, 'склад', 12, 50000.00, 50000.00, '{}'::jsonb) RETURNING id",
        healthy.tender_id,
        object_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("лот");

    let poisoned = sqlx::query_scalar!(
        "INSERT INTO core.auctions (lot_id, starting_bid, bid_step, status, started_at, ends_at)
         VALUES ($1, 55000.00, 2750.00, 'running', core.now(),
                 core.now() + interval '30 minutes')
         RETURNING id",
        lot_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("испорченные торги");

    // Ставка заявки чужого лота: правила ставки ее пропускают (круг не
    // заведен), а победителем этих торгов она стать не может
    sqlx::query!(
        "INSERT INTO core.bids (id, auction_id, application_id, amount)
         VALUES ($1, $2, $3, $4)",
        Uuid::now_v7(),
        poisoned,
        healthy.application_id,
        Decimal::from(57_750)
    )
    .execute(&mut *tx)
    .await
    .expect("ставка чужого лота");

    // Оба таймера вышли
    sqlx::query!("UPDATE refdata.clock_offset SET shift = interval '2 hours' WHERE id")
        .execute(&mut *tx)
        .await
        .expect("сдвиг часов");

    let finished = tou_db::auctions::finish_expired_on(&mut tx, 32)
        .await
        .expect("отказ по одной комнате не отменяет проход");

    assert!(
        finished.iter().any(|a| a.id == healthy.auction_id),
        "исправные торги обязаны закрыться, несмотря на соседнюю испорченную комнату"
    );
    assert!(
        finished.iter().all(|a| a.id != poisoned),
        "испорченные торги закрыться не могли"
    );
    assert_eq!(
        status_of(&mut tx, healthy.auction_id)
            .await
            .expect("статус"),
        "finished"
    );
    assert_eq!(
        status_of(&mut tx, poisoned).await.expect("статус"),
        "running",
        "отказ откатывается один и не уносит с собой соседей"
    );
}
