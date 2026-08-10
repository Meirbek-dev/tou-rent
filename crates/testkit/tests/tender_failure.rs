//! Несостоявшийся тендер против живой БД (T23, FR-801–802).
//!
//! Проверяется последний рубеж: переход в `failed` невозможен без основания
//! из закрытого перечня п. 81, а само основание - только из справочника.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use sqlx::Acquire as _;
use tou_domain::failure::FailureGround;
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - п. 81 не проверялся");
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

/// Тендер в статусе `accepting`: из него переход в `failed` разрешен (INV-021).
async fn accepting_tender(tx: &mut sqlx::PgConnection) -> Result<Uuid, sqlx::Error> {
    let organizer = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'Т23 организатор', now()) RETURNING id",
        format!("t23-{}@tou.test", Uuid::now_v7().simple())
    )
    .fetch_one(&mut *tx)
    .await?;

    // Статус задается сразу: триггер переходов действует на UPDATE
    sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id, announced_at,
                                   submission_deadline, opening_at)
         VALUES ('Т23 тендер', 'accepting', $1, now() - interval '20 days',
                 now() - interval '2 days', now() - interval '1 hour')
         RETURNING id",
        organizer
    )
    .fetch_one(tx)
    .await
}

/// FR-801: «не состоялся» без основания п. 81 не записывается.
#[tokio::test]
async fn fr801_failed_status_requires_a_ground() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let tender_id = accepting_tender(&mut tx).await.expect("тендер");

    let error = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.tenders SET status = 'failed' WHERE id = $1",
            tender_id
        ),
        "переход в failed без основания обязан быть отклонен"
    );
    assert!(error.contains("FR-801"), "ожидали отказ FR-801: {error}");

    // С основанием тот же переход проходит и проставляет момент признания
    sqlx::query!(
        "UPDATE core.tenders SET status = 'failed', failure_ground = 'no_applications'
         WHERE id = $1",
        tender_id
    )
    .execute(&mut *tx)
    .await
    .expect("признание с основанием");

    let failed_at = sqlx::query_scalar!(
        "SELECT failed_at FROM core.tenders WHERE id = $1",
        tender_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("момент признания");
    assert!(failed_at.is_some(), "момент признания проставляется БД");
}

/// Основание - только из справочника п. 81 (FK), следствие - только вместе
/// с основанием (CHECK).
#[tokio::test]
async fn ground_and_consequence_are_constrained() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let tender_id = accepting_tender(&mut tx).await.expect("тендер");

    let unknown = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.tenders SET status = 'failed', failure_ground = 'выдуманное'
             WHERE id = $1",
            tender_id
        ),
        "основание вне справочника обязано быть отклонено"
    );
    assert!(unknown.contains("failure_ground"), "{unknown}");

    let orphan = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.tenders SET consequence = 'repeat' WHERE id = $1",
            tender_id
        ),
        "следствие без основания обязано быть отклонено"
    );
    assert!(orphan.contains("consequence_needs_failure"), "{orphan}");
}

/// Справочник оснований и enum домена описывают один и тот же перечень.
#[tokio::test]
async fn grounds_match_the_domain_enum() {
    let db = require_db!();

    let codes = sqlx::query_scalar!("SELECT code FROM refdata.failure_grounds ORDER BY code")
        .fetch_all(&db)
        .await
        .expect("справочник");

    let mut expected: Vec<String> = FailureGround::ALL
        .iter()
        .map(|ground| ground.as_str().to_owned())
        .collect();
    expected.sort();
    assert_eq!(codes, expected, "перечень п. 81 разошелся с доменом");
}
