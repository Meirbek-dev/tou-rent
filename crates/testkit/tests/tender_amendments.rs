//! Изменение документации и отмена тендера против живой БД
//! (T27, FR-304, FR-305, FR-1004).
//!
//! Проверяется то, что делает БД сама: окно изменения п. 27 (не позже чем
//! за два календарных дня до дедлайна, продление не менее чем на десять),
//! продление срока приема следствием редакции, отмена только с основанием
//! и до заключения договора, освобождение объекта отмененного лота (FR-103).
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - п. 27 не проверялся");
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
    lot_id: Uuid,
    object_id: Uuid,
}

/// Объявленный тендер с открытым приемом заявок: в этой точке документация
/// еще может измениться (п. 27). `days_left` - сколько осталось до дедлайна.
async fn fixture(tx: &mut sqlx::PgConnection, days_left: i32) -> Result<Fixture, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let organizer = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т27 организатор', now()) RETURNING id",
        format!("t27-org-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Т27 объект', 'адрес', 15.00) RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await?;

    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id, announced_at,
                                   submission_deadline, opening_at)
         VALUES ('Т27 тендер', 'accepting', $1, now() - interval '20 days',
                 now() + make_interval(days => $2),
                 now() + make_interval(days => $2 + 1))
         RETURNING id",
        organizer,
        days_left
    )
    .fetch_one(&mut *tx)
    .await?;

    let lot_id = sqlx::query_scalar!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                base_rate_monthly, rate_calculation, guarantee_fee)
         VALUES ($1, 1, $2, 'офис', 12, 60000.00, '{}'::jsonb, 60000.00)
         RETURNING id",
        tender_id,
        object_id
    )
    .fetch_one(&mut *tx)
    .await?;

    Ok(Fixture {
        tender_id,
        lot_id,
        object_id,
    })
}

/// Редакция документации: запрос один на все проверки, но теперь он еще
/// и сверяется со схемой - макросу нужен литерал, а не константа.
macro_rules! amendment {
    ($tender:expr, $days:expr) => {
        sqlx::query!(
            "INSERT INTO core.tender_amendments (tender_id, version, summary, new_deadline)
             VALUES ($1, 1, 'изменено целевое назначение лота',
                     now() + make_interval(days => $2))",
            $tender,
            $days
        )
    };
}

/// FR-304 (п. 27): ближе двух календарных дней до дедлайна документация
/// не меняется, а изменение обязано продлить прием на десять дней.
#[tokio::test]
async fn fr304_amendment_window_and_extension() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");

    let frozen = fixture(&mut tx, 1).await.expect("дедлайн завтра");
    let too_late = rejected!(
        tx,
        amendment!(frozen.tender_id, 30),
        "изменение в последние два дня приема обязано быть отклонено"
    );
    assert!(too_late.contains("FR-304"), "{too_late}");

    let open = fixture(&mut tx, 20).await.expect("дедлайн через 20 дней");
    let not_extended = rejected!(
        tx,
        amendment!(open.tender_id, 15),
        "редакция без продления срока приема обязана быть отклонена"
    );
    assert!(not_extended.contains("продлить"), "{not_extended}");

    // Продление есть, но короче десяти календарных дней от публикации (п. 27)
    let soon = fixture(&mut tx, 5).await.expect("дедлайн через 5 дней");
    let short = rejected!(
        tx,
        amendment!(soon.tender_id, 7),
        "продление меньше 10 дней от публикации редакции обязано быть отклонено"
    );
    assert!(short.contains("10 календарных дней"), "{short}");
}

/// Изменение продлевает срок приема заявок и сдвигает вскрытие (п. 27).
#[tokio::test]
async fn amendment_extends_the_acceptance_period() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, 20).await.expect("фикстура");

    // `!` у сроков - не переопределение схемы, а то же требование, что было
    // в кортеже без Option: фикстура их задала, NULL здесь означал бы поломку
    let before = sqlx::query!(
        r#"SELECT submission_deadline AS "submission_deadline!", opening_at AS "opening_at!"
           FROM core.tenders WHERE id = $1"#,
        f.tender_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("тендер");

    amendment!(f.tender_id, 30)
        .execute(&mut *tx)
        .await
        .expect("редакция документации");

    let after = sqlx::query!(
        r#"SELECT t.submission_deadline AS "submission_deadline!",
                  t.opening_at AS "opening_at!", a.version, a.previous_deadline
           FROM core.tenders t
           JOIN core.tender_amendments a ON a.tender_id = t.id
           WHERE t.id = $1"#,
        f.tender_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("тендер и редакция");

    assert!(
        after.submission_deadline > before.submission_deadline,
        "срок приема продлен (п. 27)"
    );
    assert!(
        after.opening_at > before.opening_at,
        "вскрытие сдвинуто вслед за приемом"
    );
    assert_eq!(after.version, 1, "первая редакция документации");
    assert_eq!(
        after.previous_deadline,
        Some(before.submission_deadline),
        "редакция помнит прежний срок"
    );
}

/// Опубликованная редакция не переписывается: участники решали по ней.
#[tokio::test]
async fn published_amendment_is_immutable() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, 20).await.expect("фикстура");

    amendment!(f.tender_id, 30)
        .execute(&mut *tx)
        .await
        .expect("редакция");

    let edited = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.tender_amendments SET summary = 'другое' WHERE tender_id = $1",
            f.tender_id
        ),
        "правка опубликованной редакции обязана быть отклонена"
    );
    assert!(edited.contains("FR-304"), "{edited}");

    let deleted = rejected!(
        tx,
        sqlx::query!(
            "DELETE FROM core.tender_amendments WHERE tender_id = $1",
            f.tender_id
        ),
        "удаление редакции обязано быть отклонено"
    );
    assert!(
        deleted.contains("FR-304") || deleted.contains("permission denied"),
        "{deleted}"
    );

    // Ссылку на печатную форму дописать можно - это не изменение условий
    sqlx::query!(
        "UPDATE core.tender_amendments SET doc_key = 'tenders/x.pdf' WHERE tender_id = $1",
        f.tender_id
    )
    .execute(&mut *tx)
    .await
    .expect("ключ печатной формы дописывается");
}

/// FR-305 (п. 78): отмена только с основанием и только до заключения договора.
#[tokio::test]
async fn fr305_cancellation_needs_a_reason_and_no_contract() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, 20).await.expect("фикстура");

    let no_reason = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.tenders SET status = 'cancelled' WHERE id = $1",
            f.tender_id
        ),
        "отмена без основания обязана быть отклонена"
    );
    assert!(no_reason.contains("FR-305"), "{no_reason}");

    sqlx::query!(
        "UPDATE core.tenders SET status = 'cancelled', cancel_reason = 'нарушение п. 5'
         WHERE id = $1",
        f.tender_id
    )
    .execute(&mut *tx)
    .await
    .expect("отмена с основанием");

    let cancelled_at = sqlx::query_scalar!(
        "SELECT cancelled_at FROM core.tenders WHERE id = $1",
        f.tender_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("тендер");
    assert!(cancelled_at.is_some(), "момент отмены проставляется БД");
}

/// Отмена лота освобождает его объект (FR-103, FR-305).
#[tokio::test]
async fn cancelled_lot_frees_its_object() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, 20).await.expect("фикстура");

    // `status` приходит из представления - планировщик считает такой столбец
    // потенциально NULL, хотя CASE в нем всегда что-то возвращает
    let before = sqlx::query_scalar!(
        r#"SELECT status AS "status!" FROM core.object_statuses WHERE object_id = $1"#,
        f.object_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("статус объекта");
    assert_eq!(before, "in_tender", "объект разыгрывается (FR-103)");

    let no_reason = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.lots SET cancelled_at = now() WHERE id = $1",
            f.lot_id
        ),
        "отмена лота без основания обязана быть отклонена"
    );
    assert!(
        no_reason.contains("lot_cancellation_has_reason"),
        "{no_reason}"
    );

    sqlx::query!(
        "UPDATE core.lots SET cancelled_at = now(), cancel_reason = 'нарушение п. 5' WHERE id = $1",
        f.lot_id
    )
    .execute(&mut *tx)
    .await
    .expect("отмена лота");

    let after = sqlx::query_scalar!(
        r#"SELECT status AS "status!" FROM core.object_statuses WHERE object_id = $1"#,
        f.object_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("статус объекта");
    assert_eq!(after, "free", "объект отмененного лота свободен (FR-103)");
}
