//! Гаунтлет G-006: неполный пакет заявки - своя причина отказа (FR-502, п. 40).
//!
//! Решение о допуске возможно по заявке, которая одновременно в решаемом
//! статусе и с полным пакетом из пяти зашифрованных PDF. Условия разные,
//! а отказ до этой проверки был один: `application_not_pending` приходил
//! и тогда, когда со статусом все в порядке, а не хватало документов, -
//! секретарь читал про статус и искал причину не там.
//!
//! Проверяется классификация отказа на живой БД: полнота пакета считается
//! функцией `core.application_package_complete`, и повторить ее в Rust
//! значило бы проверять копию, а не правило.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021); каждая проверка живет в
//! транзакции с откатом.

use tou_db::admission::decidable_package_complete;
use uuid::Uuid;

async fn try_pool() -> Result<Option<tou_db::Db>, sqlx::Error> {
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - пакет заявки не проверялся");
                return;
            }
        }
    };
}

/// Пять обязательных типов пакета (п. 40) - тот же перечень, что в
/// `core.application_package_complete`.
const PACKAGE: [&str; 5] = [
    "application_form",
    "registration_certificate",
    "tax_clearance",
    "guarantee_payment",
    "qualification_documents",
];

async fn user(tx: &mut sqlx::PgConnection, tag: &str) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', $2, core.now()) RETURNING id",
        format!("g006-{tag}-{}@tou.test", Uuid::now_v7().simple()),
        format!("G-006 {tag}")
    )
    .fetch_one(tx)
    .await
}

/// Заявка «подана» по тендеру в заданном статусе - без единого документа.
async fn application(
    tx: &mut sqlx::PgConnection,
    organizer: Uuid,
    participant: Uuid,
    tender_status: &str,
) -> Result<Uuid, sqlx::Error> {
    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'G-006 объект', 'адрес', 10.00) RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await?;

    let now = sqlx::query_scalar!(r#"SELECT core.now() AS "now!""#)
        .fetch_one(&mut *tx)
        .await?;

    // Дедлайн приема - в будущем: заявку с истекшим сроком не примет
    // триггер INV-037, а проверяется здесь не он
    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id, announced_at,
                                   submission_deadline, opening_at)
         VALUES ('G-006 тендер', $1::text::core.tender_status, $2, $3, $4, $4)
         RETURNING id",
        tender_status,
        organizer,
        now - time::Duration::days(15),
        now + time::Duration::days(5)
    )
    .fetch_one(&mut *tx)
    .await?;

    let lot_id = sqlx::query_scalar!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                base_rate_monthly, guarantee_fee, rate_calculation)
         VALUES ($1, 1, $2, 'офис', 12, 30000.00, 30000.00, '{}'::jsonb)
         RETURNING id",
        tender_id,
        object_id
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query_scalar!(
        r#"INSERT INTO core.applications
             (tender_id, lot_id, participant_id, status, applicant_kind, applicant_details)
           VALUES ($1, $2, $3, 'submitted', 'legal_entity', '{"name": "ТОО G-006"}'::jsonb)
           RETURNING id"#,
        tender_id,
        lot_id,
        participant
    )
    .fetch_one(&mut *tx)
    .await
}

async fn attach(
    tx: &mut sqlx::PgConnection,
    application_id: Uuid,
    kind: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO core.application_files
           (application_id, file_key, filename, content_type, size_bytes,
            document_kind, encryption_version)
         VALUES ($1, $2, $3, 'application/pdf', 1024,
                 $4::text::core.application_document_kind, 1)",
        application_id,
        format!("applications/{application_id}/{kind}.pdf"),
        format!("{kind}.pdf"),
        kind
    )
    .execute(tx)
    .await
    .map(|_| ())
}

/// Неполный пакет опознается отдельно от нерешаемого статуса: у заявки,
/// которая по всем прочим условиям решается, ответ `Some(false)`.
#[tokio::test]
async fn incomplete_package_is_told_apart_from_a_wrong_status() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let organizer = user(&mut tx, "организатор").await.expect("организатор");
    let participant = user(&mut tx, "участник").await.expect("участник");
    tou_db::set_actor(&mut tx, organizer).await.expect("актор");

    let application_id = application(&mut tx, organizer, participant, "qualification")
        .await
        .expect("заявка на вскрытом тендере");

    assert_eq!(
        decidable_package_complete(&mut tx, application_id)
            .await
            .expect("классификация отказа"),
        Some(false),
        "причина отказа - неполный пакет, а не статус заявки (п. 40)"
    );

    // Тот же пакет, но полный: решению больше ничего не мешает
    for kind in PACKAGE {
        attach(&mut tx, application_id, kind)
            .await
            .expect("документ пакета");
    }
    assert_eq!(
        decidable_package_complete(&mut tx, application_id)
            .await
            .expect("классификация отказа"),
        Some(true),
        "полный пакет из пяти документов не может быть причиной отказа"
    );

    tx.rollback().await.expect("откат");
}

/// Тендер не вскрыт - дело не в пакете: `None` означает «причина другая»,
/// и обработчик остается при `application_not_pending`.
#[tokio::test]
async fn a_tender_before_opening_is_not_a_package_problem() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let organizer = user(&mut tx, "организатор").await.expect("организатор");
    let participant = user(&mut tx, "участник").await.expect("участник");
    tou_db::set_actor(&mut tx, organizer).await.expect("актор");

    let application_id = application(&mut tx, organizer, participant, "accepting")
        .await
        .expect("заявка на невскрытом тендере");

    assert_eq!(
        decidable_package_complete(&mut tx, application_id)
            .await
            .expect("классификация отказа"),
        None,
        "до вскрытия решение невозможно по другой причине, чем пакет"
    );

    tx.rollback().await.expect("откат");
}
