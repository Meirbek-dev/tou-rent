//! Правка сроков тендера администратором против живой БД (М15, FR-1503,
//! FR-1601; сроки - FR-303).
//!
//! Проверяется то, что делает БД: правка переносит все четыре отметки,
//! включая дату публикации, которую редакция документации (FR-304) не
//! трогает, и у объявленного тендера, а не только у черновика; след
//! ложится в audit.log под актором с прежним и новым значением
//! (INV-AUDIT); перепутанные прием и вскрытие отбивает CHECK
//! `deadline_before_opening`, и отказ приходит причиной перечня, а не
//! поломкой.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use sqlx::Acquire as _;
use time::macros::datetime;
use tou_db::tenders::{ScheduleFields, TransitionError};
use tou_domain::rule::RuleViolation;
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - правка сроков не проверялась");
                return;
            }
        }
    };
}

struct Fixture {
    actor: Uuid,
    tender_id: Uuid,
}

/// Объявленный тендер с открытым приемом, как на стенде: сроки назначены
/// при публикации - она в прошлом, прием и вскрытие впереди.
async fn fixture(tx: &mut sqlx::PgConnection) -> Result<Fixture, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let actor = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'М15 админ', now()) RETURNING id",
        format!("m15-schedule-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id, announced_at,
                                   submission_deadline, opening_at)
         VALUES ('М15 сроки', 'accepting', $1, core.now() - interval '1 day',
                 core.now() + interval '9 days', core.now() + interval '10 days')
         RETURNING id",
        actor
    )
    .fetch_one(&mut *tx)
    .await?;

    Ok(Fixture { actor, tender_id })
}

/// FR-303, FR-1601: правка переносит публикацию, прием, вскрытие и торги
/// у объявленного тендера, не трогая статус, и оставляет след под актором.
#[tokio::test]
async fn admin_schedule_moves_every_mark_including_publication() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");
    tou_db::set_actor(&mut tx, f.actor).await.expect("актор");

    // Сроки из объявления университета (время Астаны, GMT+5): публикация
    // на день раньше, чем тендер завели на стенде, - редакция документации
    // (п. 27) так не умеет, она только продлевает прием
    let announced = datetime!(2026-08-31 13:06 UTC);
    let deadline = datetime!(2026-09-10 13:00 UTC);
    let opening = datetime!(2026-09-11 05:00 UTC);
    let trading = datetime!(2026-09-14 12:58 UTC);

    let updated = tou_db::tenders::set_schedule_on(
        &mut tx,
        f.tender_id,
        ScheduleFields {
            announced_at: Some(announced),
            submission_deadline: Some(deadline),
            opening_at: Some(opening),
            trading_at: Some(trading),
        },
    )
    .await
    .expect("правка сроков")
    .expect("тендер найден");

    assert_eq!(updated.announced_at, Some(announced));
    assert_eq!(updated.submission_deadline, Some(deadline));
    assert_eq!(updated.opening_at, Some(opening));
    assert_eq!(updated.trading_at, Some(trading));
    assert_eq!(updated.status, "accepting", "статус правкой не затронут");

    // След: событие UPDATE по строке тендера под актором, в котором видна
    // и прежняя, и новая дата публикации (NFR-07: фиксация полная)
    let event = sqlx::query!(
        r#"SELECT actor_id,
                  payload -> 'old' ->> 'announced_at' AS "old_announced?",
                  payload -> 'new' ->> 'announced_at' AS "new_announced?"
           FROM audit.log
           WHERE table_name = 'core.tenders' AND row_id = $1 AND action = 'UPDATE'
           ORDER BY id DESC LIMIT 1"#,
        f.tender_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("событие аудита");
    assert_eq!(event.actor_id, Some(f.actor));
    assert!(
        event.old_announced.is_some(),
        "прежняя дата публикации в событии"
    );
    assert!(
        event.new_announced.is_some(),
        "новая дата публикации в событии"
    );
    assert_ne!(event.old_announced, event.new_announced);

    // Несуществующий тендер - None, а не ошибка: 404 решает обработчик
    let missing = tou_db::tenders::set_schedule_on(&mut tx, Uuid::nil(), ScheduleFields::default())
        .await
        .expect("запрос по несуществующему тендеру");
    assert!(missing.is_none());

    tx.rollback().await.expect("rollback");
}

/// FR-303: вскрытие раньше окончания приема отбивает CHECK таблицы, и
/// отказ приезжает причиной перечня - тем же путем, что у правки черновика.
#[tokio::test]
async fn disorder_is_rejected_by_the_table_as_a_rule() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");
    tou_db::set_actor(&mut tx, f.actor).await.expect("актор");

    let mut sp = tx.begin().await.expect("savepoint");
    let err = tou_db::tenders::set_schedule_on(
        &mut sp,
        f.tender_id,
        ScheduleFields {
            announced_at: Some(datetime!(2026-08-31 13:06 UTC)),
            submission_deadline: Some(datetime!(2026-09-11 05:00 UTC)),
            opening_at: Some(datetime!(2026-09-10 13:00 UTC)),
            trading_at: None,
        },
    )
    .await
    .expect_err("вскрытие раньше окончания приема обязано быть отклонено");
    sp.rollback().await.expect("rollback savepoint");

    assert!(
        matches!(
            err,
            TransitionError::Rejected(ref rejection)
                if rejection.rule() == RuleViolation::TenderPublicationTerms
        ),
        "ожидался отказ по правилу FR-303, получено: {err}"
    );

    tx.rollback().await.expect("rollback");
}
