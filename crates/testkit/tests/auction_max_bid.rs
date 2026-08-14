//! Текущий максимум ленты торгов (INV-063) против живой БД.
//!
//! Максимум считался перебором выборки `bids_of`, а она ограничена
//! `LIMIT MAX_ROWS` по возрастанию `seq`. Пока ставок меньше тысячи, разницы
//! не видно; после - выборка обрезается с начала ленты, максимум остается за
//! ее пределами, и клиент получает заниженный минимум следующей ставки.
//! Триггер `core.enforce_bid_rules` считает максимум по всей таблице и потому
//! прав - он отбивал бы каждую такую ставку, и комната вставала бы.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use rust_decimal::Decimal;
use uuid::Uuid;

/// Ставок заведомо больше потолка выборки: ровно на нем разница и появляется.
const LEDGER_LEN: i64 = tou_db::MAX_ROWS + 5;

const STARTING_BID: i64 = 55_000;
const BID_STEP: i64 = 2_750;

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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - максимум ленты не проверялся");
                return;
            }
        }
    };
}

/// Идущие торги с одной допущенной заявкой; лента наполняется отдельно.
async fn fixture(tx: &mut sqlx::PgConnection) -> Result<(Uuid, Uuid), sqlx::Error> {
    let organizer = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'W10 организатор', core.now()) RETURNING id",
        format!("w10-org-{}@tou.test", Uuid::now_v7().simple())
    )
    .fetch_one(&mut *tx)
    .await?;

    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id)
         VALUES ('W10 тендер', 'trading', $1) RETURNING id",
        organizer
    )
    .fetch_one(&mut *tx)
    .await?;

    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'W10 объект', 'адрес', 10.00) RETURNING id"
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
         VALUES ($1::citext, 'x', 'W10 участник', core.now()) RETURNING id",
        format!("w10-p-{}@tou.test", Uuid::now_v7().simple())
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
        "INSERT INTO core.auctions (lot_id, starting_bid, bid_step, status, started_at, ends_at)
         VALUES ($1, $2, $3, 'running', core.now(), core.now() + interval '1 hour')
         RETURNING id",
        lot_id,
        Decimal::from(STARTING_BID),
        Decimal::from(BID_STEP)
    )
    .fetch_one(&mut *tx)
    .await?;

    Ok((auction_id, application_id))
}

/// Лента длиной `LEDGER_LEN`: каждая ставка ровно на шаг выше предыдущей -
/// именно такую последовательность и пропускает триггер (INV-063).
async fn fill_ledger(
    tx: &mut sqlx::PgConnection,
    auction_id: Uuid,
    application_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO core.bids (id, auction_id, application_id, amount)
         SELECT gen_random_uuid(), $1, $2, $3::numeric + $4::numeric * n
         FROM generate_series(1, $5::bigint) AS n",
        auction_id,
        application_id,
        Decimal::from(STARTING_BID),
        Decimal::from(BID_STEP),
        LEDGER_LEN
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// NFR-02, INV-063: максимум ленты не зависит от ее длины. Прежний способ
/// (максимум по выборке `bids_of`) на той же ленте дает другое, заниженное
/// значение - тест держит обе величины рядом, чтобы разница была видна.
#[tokio::test]
async fn maximum_does_not_depend_on_ledger_length() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");

    let (auction_id, application_id) = fixture(&mut tx).await.expect("fixture");
    fill_ledger(&mut tx, auction_id, application_id)
        .await
        .expect("лента ставок");

    let expected = Decimal::from(STARTING_BID + BID_STEP * LEDGER_LEN);
    let actual = tou_db::auctions::max_amount_on(&mut tx, auction_id)
        .await
        .expect("максимум ленты");
    assert_eq!(
        actual,
        Some(expected),
        "максимум обязан быть последней ставкой, сколько бы их ни было"
    );

    // Прежний счет: максимум по первой тысяче ставок в порядке seq
    let truncated = sqlx::query_scalar!(
        "SELECT max(amount) FROM (
           SELECT amount FROM core.bids WHERE auction_id = $1 ORDER BY seq LIMIT $2
         ) AS page",
        auction_id,
        tou_db::MAX_ROWS
    )
    .fetch_one(&mut *tx)
    .await
    .expect("максимум по обрезанной выборке");
    assert!(
        truncated < Some(expected),
        "выборка обрезана по seq с начала ленты - максимума в ней уже нет"
    );
}

/// Торги без единой ставки: максимума нет, а не «ноль тенге» - на нем
/// стоит расчет минимально допустимой ставки (INV-063).
#[tokio::test]
async fn empty_ledger_has_no_maximum() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");

    let (auction_id, _) = fixture(&mut tx).await.expect("fixture");
    let actual = tou_db::auctions::max_amount_on(&mut tx, auction_id)
        .await
        .expect("максимум ленты");
    assert_eq!(actual, None, "до первой ставки максимума нет");
}
