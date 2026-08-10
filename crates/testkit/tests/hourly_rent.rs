//! Почасовая аренда против живой БД (T30, FR-205, FR-206, п. 97).
//!
//! Почасовой лот отличается единицей ставки и объемом часов: БД требует
//! объем ровно у почасового лота и сама считает гарантийный взнос от него
//! (FR-206), а помесячному лоту объем часов задать нельзя.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use rust_decimal::Decimal;
use sqlx::Acquire as _;
use uuid::Uuid;

async fn try_pool() -> Result<Option<tou_db::Db>, sqlx::Error> {
    // Пропуск без адреса допустим локально, но не в пайплайне (G2/G15):
    // молча пройденный интеграционный тест ничего не проверяет
    match tou_testkit::database_url().map_err(|e| sqlx::Error::Configuration(Box::new(e)))? {
        Some(url) => tou_db::connect(&url).await.map(Some),
        None => Ok(None),
    }
}

macro_rules! require_db {
    () => {
        match try_pool()
            .await
            .expect("TESTKIT_DATABASE_URL: подключение не удалось")
        {
            Some(db) => db,
            None => {
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - почасовой лот не проверялся");
                return;
            }
        }
    };
}

macro_rules! rejected {
    ($tx:expr, $query:expr, $why:expr) => {{
        let mut sp = $tx.begin().await.expect("savepoint");
        let error = $query.execute(&mut *sp).await.expect_err($why);
        sp.rollback().await.expect("rollback savepoint");
        error.to_string()
    }};
}

struct Fixture {
    tender_id: Uuid,
    object_id: Uuid,
}

async fn fixture(tx: &mut sqlx::PgConnection) -> Result<Fixture, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let organizer = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'Т30 организатор', now()) RETURNING id",
        format!("t30-org-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Т30 актовый зал', 'адрес', 200.00) RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await?;

    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id)
         VALUES ('Т30 почасовой тендер', 'draft', $1) RETURNING id",
        organizer
    )
    .fetch_one(&mut *tx)
    .await?;

    Ok(Fixture {
        tender_id,
        object_id,
    })
}

/// Вставка лота: список столбцов остается в одном месте, но проверка по схеме
/// требует литерала прямо в вызове, поэтому общей стала не строка, а макрос.
/// `$4::text::core.rate_unit` - единица приходит строкой, к перечислению ее
/// приводит БД.
macro_rules! lot {
    ($tender:expr, $seq:expr, $object:expr, $unit:expr, $hours:expr) => {
        sqlx::query!(
            "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                    base_rate_monthly, rate_calculation, guarantee_fee,
                                    rate_unit, hours_total)
             VALUES ($1, $2, $3, 'мероприятия', 12, 20000.00, '{}'::jsonb, 20000.00,
                     $4::text::core.rate_unit, $5)",
            $tender,
            $seq,
            $object,
            $unit,
            $hours
        )
    };
}

/// FR-205 (п. 97): объем часов есть ровно у почасового лота.
#[tokio::test]
async fn fr205_hours_belong_to_hourly_lots_only() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    let hourly_without_hours = rejected!(
        tx,
        lot!(
            f.tender_id,
            1_i32,
            f.object_id,
            "hourly",
            Option::<i32>::None
        ),
        "почасовой лот без объема часов обязан быть отклонен"
    );
    assert!(
        hourly_without_hours.contains("hourly_lot_has_hours"),
        "{hourly_without_hours}"
    );

    let monthly_with_hours = rejected!(
        tx,
        lot!(f.tender_id, 2_i32, f.object_id, "monthly", Some(4_i32)),
        "объем часов у помесячного лота обязан быть отклонен"
    );
    assert!(
        monthly_with_hours.contains("hourly_lot_has_hours"),
        "{monthly_with_hours}"
    );
}

/// FR-206 для почасового лота: взнос считает БД от объема часов (п. 97).
#[tokio::test]
async fn hourly_guarantee_fee_covers_the_whole_volume() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    // Взнос в запросе намеренно занижен: БД пересчитает его сама
    lot!(f.tender_id, 1_i32, f.object_id, "hourly", Some(8_i32))
        .execute(&mut *tx)
        .await
        .expect("почасовой лот");

    let row = sqlx::query!(
        r#"SELECT rate_unit::text AS "rate_unit!", hours_total,
                  base_rate_monthly, guarantee_fee
           FROM core.lots WHERE tender_id = $1"#,
        f.tender_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("лот");

    assert_eq!(row.rate_unit, "hourly", "единица ставки - час (п. 97)");
    assert_eq!(row.hours_total, Some(8));
    assert_eq!(
        row.base_rate_monthly,
        Decimal::new(2000000, 2),
        "ставка хранится за час"
    );
    assert_eq!(
        row.guarantee_fee,
        Decimal::new(16000000, 2),
        "взнос = ставка за час × объем часов (FR-206)"
    );
}

/// Помесячный лот остается прежним: взнос равен месячной ставке (FR-206).
#[tokio::test]
async fn monthly_lot_keeps_its_fee() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    lot!(
        f.tender_id,
        1_i32,
        f.object_id,
        "monthly",
        Option::<i32>::None
    )
    .execute(&mut *tx)
    .await
    .expect("помесячный лот");

    let row = sqlx::query!(
        r#"SELECT rate_unit::text AS "rate_unit!", guarantee_fee
           FROM core.lots WHERE tender_id = $1"#,
        f.tender_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("лот");

    assert_eq!(row.rate_unit, "monthly", "по умолчанию ставка помесячная");
    assert_eq!(
        row.guarantee_fee,
        Decimal::new(2000000, 2),
        "взнос = месячная ставка"
    );
}

/// Единицы ставки БД совпадают с доменом (паритет enum, G16).
#[tokio::test]
async fn rate_units_match_the_domain_enum() {
    use tou_domain::rates::RateUnit;

    let db = require_db!();
    let units = sqlx::query_scalar!(
        r#"SELECT unnest(enum_range(NULL::core.rate_unit))::text AS "unit!" ORDER BY 1"#
    )
    .fetch_all(&db)
    .await
    .expect("значения enum");

    let mut domain: Vec<String> = RateUnit::ALL
        .iter()
        .map(|unit| unit.as_str().to_owned())
        .collect();
    domain.sort();

    assert_eq!(units, domain, "перечень единиц ставки совпадает с доменом");
}
