//! Отмена тендера видна снаружи (FR-305, п. 78–79, G-078) - против живой БД.
//!
//! Отмена пишет на тендер две отметки: момент и основание (`cancelled_at`,
//! `cancel_reason`). Выборка тендера их не отдавала, поэтому портал и кабинет
//! знали об отмене ровно одно слово - статус. Участник, у которого на этом
//! тендере лежит заявка и уплаченный взнос, читал «отменен» без даты и без
//! причины, хотя причина обязательна по п. 78 и хранится рядом.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021): отметки ставит UPDATE под
//! триггером `check_tender_cancellation`, и проверять их без БД нечем.

use uuid::Uuid;

async fn try_pool() -> Result<Option<tou_db::Db>, sqlx::Error> {
    match tou_testkit::database_ready()
        .await
        .map_err(|e| sqlx::Error::Configuration(Box::new(e)))?
    {
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - отмена не проверялась");
                return;
            }
        }
    };
}

const REASON: &str = "G-078: помещение передано под учебный процесс (п. 78)";

/// Рубеж G-078: у отмененного тендера снаружи видно, когда и почему.
#[tokio::test]
async fn a_cancelled_tender_says_when_and_why() {
    let db = require_db!();

    let organizer = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'G-078 организатор', core.now()) RETURNING id",
        format!("g078-{}@tou.test", Uuid::now_v7().simple())
    )
    .fetch_one(&db)
    .await
    .expect("организатор");

    let tender = sqlx::query_scalar!(
        "INSERT INTO core.tenders
           (title, status, organizer_id, announced_at, submission_deadline, opening_at)
         VALUES ('G-078 аренда аудитории', 'announced', $1, core.now() - interval '2 days',
                 core.now() + interval '12 days', core.now() + interval '13 days')
         RETURNING id",
        organizer
    )
    .fetch_one(&db)
    .await
    .expect("тендер");

    let before = tou_db::tenders::get(&db, tender)
        .await
        .expect("чтение тендера")
        .expect("тендер на месте");
    assert!(
        before.cancelled_at.is_none() && before.cancel_reason.is_none(),
        "у живого тендера отметок отмены нет"
    );

    tou_db::amendments::cancel_tender(&db, organizer, tender, REASON)
        .await
        .expect("отмена тендера");

    let after = tou_db::tenders::get(&db, tender)
        .await
        .expect("чтение тендера")
        .expect("тендер на месте");
    assert_eq!(after.status, "cancelled");
    assert_eq!(
        after.cancel_reason.as_deref(),
        Some(REASON),
        "основание отмены обязано доезжать до карточки (п. 78)"
    );
    let cancelled_at = after.cancelled_at.expect("момент отмены");
    assert!(
        cancelled_at >= before.created_at,
        "момент отмены не может быть раньше самого тендера"
    );

    let _ = tou_db::purge::purge(
        &db,
        organizer,
        tou_db::purge::PurgeScope::Tenders,
        Some(&[tender]),
    )
    .await;
    let _ = sqlx::query!("DELETE FROM core.users WHERE id = $1", organizer)
        .execute(&db)
        .await;
}
