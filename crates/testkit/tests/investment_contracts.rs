//! Инвестиционные договоры против живой БД (T36, FR-1204, INV-091, INV-094,
//! п. 91–94).
//!
//! INV-094: срок договора не превышает семи лет - CHECK в схеме.
//! INV-091: договор не подписывается без полного комплекта приложений п. 91.
//! Приемка инвестиций (п. 92) неизменяема, продление (п. 93) требует полного
//! исполнения, порога объема и не повторяется.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use rust_decimal::Decimal;
use sqlx::Acquire as _;
use time::macros::date;
use tou_domain::investment::{Attachment, EXTENSION_MONTHS, MAX_TERM_MONTHS};
use tou_domain::special::SpecialCategory;
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
                eprintln!(
                    "SKIP: TESTKIT_DATABASE_URL не задан - инвестиционный договор не проверялся"
                );
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
    request_id: Uuid,
    contract_id: Uuid,
    actor_id: Uuid,
}

/// Удовлетворенная заявка инвестиционной категории и договор по ней.
async fn fixture(
    tx: &mut sqlx::PgConnection,
    amount: Decimal,
    term_months: i32,
) -> Result<Fixture, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    // `$1::citext`: почта хранится регистронезависимым типом, а привязка
    // приходит текстом
    let actor_id = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'Т36 сотрудник', now()) RETURNING id",
        format!("t36-actor-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let applicant_id = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'Т36 инвестор', now()) RETURNING id",
        format!("t36-investor-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Т36 помещение', 'адрес', 120.00) RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await?;

    let request_id = sqlx::query_scalar!(
        "INSERT INTO core.special_requests
           (applicant_id, category, applicant_kind, applicant_details, purpose,
            object_id, investment_amount)
         VALUES ($1, $2, 'legal_entity', '{}'::jsonb, 'инвестиционный проект', $3, $4)
         RETURNING id",
        applicant_id,
        SpecialCategory::Category7.as_str(),
        object_id,
        amount
    )
    .fetch_one(&mut *tx)
    .await?;

    // Путь заявки к «предоставить»: заключение подразделения → решение (T34)
    sqlx::query!(
        "INSERT INTO core.special_reviews
           (special_request_id, reviewer_id, conclusion, recommendation)
         VALUES ($1, $2, 'проект соответствует требованиям', 'grant')",
        request_id,
        actor_id
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "INSERT INTO core.special_board_decisions
           (special_request_id, decision, rationale, decided_by)
         VALUES ($1, 'grant', 'проект принят', $2)",
        request_id,
        actor_id
    )
    .execute(&mut *tx)
    .await?;

    let contract_id = sqlx::query_scalar!(
        "INSERT INTO core.contracts (object_id, tenant_id, monthly_rate)
         VALUES ($1, $2, 150000.00) RETURNING id",
        object_id,
        applicant_id
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query!(
        "INSERT INTO core.investment_contracts
           (contract_id, special_request_id, investment_amount, term_months)
         VALUES ($1, $2, $3, $4)",
        contract_id,
        request_id,
        amount,
        term_months
    )
    .execute(&mut *tx)
    .await?;

    Ok(Fixture {
        request_id,
        contract_id,
        actor_id,
    })
}

/// INV-094 (п. 94): срок больше семи лет схема не принимает.
#[tokio::test]
async fn inv094_term_does_not_exceed_seven_years() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, Decimal::from(30_000_000), MAX_TERM_MONTHS)
        .await
        .expect("фикстура");

    let error = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.investment_contracts SET term_months = $2 WHERE contract_id = $1",
            f.contract_id,
            MAX_TERM_MONTHS + 1
        ),
        "срок больше семи лет обязан быть отклонен"
    );
    assert!(
        error.contains("investment_contracts_term_months_check"),
        "{error}"
    );
}

/// FR-1204 (п. 90–91): договор заключается только по удовлетворенной заявке.
#[tokio::test]
async fn contract_needs_a_granted_request() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let tag = Uuid::now_v7().simple().to_string();

    let applicant_id = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'Т36 инвестор', now()) RETURNING id",
        format!("t36-pending-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await
    .expect("заявитель");

    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Т36 помещение', 'адрес', 30.00) RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await
    .expect("объект");

    // Заявка подана, но решения по ней нет
    let request_id = sqlx::query_scalar!(
        "INSERT INTO core.special_requests
           (applicant_id, category, applicant_kind, applicant_details, purpose,
            object_id, investment_amount)
         VALUES ($1, $2, 'legal_entity', '{}'::jsonb, 'проект', $3, 30000000.00)
         RETURNING id",
        applicant_id,
        SpecialCategory::Category7.as_str(),
        object_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("заявка");

    let contract_id = sqlx::query_scalar!(
        "INSERT INTO core.contracts (object_id, tenant_id, monthly_rate)
         VALUES ($1, $2, 100000.00) RETURNING id",
        object_id,
        applicant_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("договор");

    let error = rejected!(
        tx,
        sqlx::query!(
            "INSERT INTO core.investment_contracts
               (contract_id, special_request_id, investment_amount, term_months)
             VALUES ($1, $2, 30000000.00, 60)",
            contract_id,
            request_id
        ),
        "договор по нерешенной заявке обязан быть отклонен"
    );
    assert!(error.contains("FR-1204"), "{error}");
}

/// INV-091 (п. 91): без полного комплекта приложений договор не подписывается.
#[tokio::test]
async fn inv091_signing_needs_every_attachment() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, Decimal::from(30_000_000), 60)
        .await
        .expect("фикстура");

    // Тот же запрос до и после досылки приложений: макрос вместо константы,
    // потому что sqlx проверяет только строковый литерал
    macro_rules! signing {
        ($contract_id:expr) => {
            sqlx::query!(
                "UPDATE core.contracts
                 SET status = 'signing',
                     lease_period = tstzrange(now(), now() + interval '5 years')
                 WHERE id = $1",
                $contract_id
            )
        };
    }

    let error = rejected!(
        tx,
        signing!(f.contract_id),
        "подписание без приложений обязано быть отклонено"
    );
    assert!(error.contains("INV-091"), "{error}");

    // Комплект п. 91 закрыт - договор уходит на подписание
    for attachment in Attachment::ALL {
        sqlx::query!(
            "INSERT INTO core.investment_contract_files
               (contract_id, code, file_key, filename, content_type, size_bytes)
             VALUES ($1, $2, 'k', 'документ.pdf', 'application/pdf', 10)",
            f.contract_id,
            attachment.as_str()
        )
        .execute(&mut *tx)
        .await
        .unwrap_or_else(|err| panic!("приложение {}: {err}", attachment.as_str()));
    }

    signing!(f.contract_id)
        .execute(&mut *tx)
        .await
        .expect("договор с полным комплектом подписывается");
}

/// Приложение вне перечня п. 91 отклоняется FK.
#[tokio::test]
async fn attachments_come_from_the_closed_list() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, Decimal::from(30_000_000), 60)
        .await
        .expect("фикстура");

    let error = rejected!(
        tx,
        sqlx::query!(
            "INSERT INTO core.investment_contract_files
               (contract_id, code, file_key, filename, content_type, size_bytes)
             VALUES ($1, 'business_plan', 'k', 'план.pdf', 'application/pdf', 10)",
            f.contract_id
        ),
        "документ вне перечня п. 91 обязан быть отклонен"
    );
    assert!(
        error.contains("investment_contract_files_code_fkey"),
        "{error}"
    );
}

/// Перечень приложений БД и домена - один (паритет, G16).
#[tokio::test]
async fn attachment_catalog_matches_the_domain() {
    let db = require_db!();

    let codes =
        sqlx::query_scalar!("SELECT code FROM refdata.investment_attachments ORDER BY ordinal")
            .fetch_all(&db)
            .await
            .expect("перечень приложений");

    let domain: Vec<String> = Attachment::ALL
        .iter()
        .map(|attachment| attachment.as_str().to_owned())
        .collect();
    assert_eq!(codes, domain, "перечень п. 91 совпадает с доменом");
}

/// Акт приемки (п. 92) неизменяем и не удаляется.
#[tokio::test]
async fn acceptance_is_a_fact() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, Decimal::from(30_000_000), 60)
        .await
        .expect("фикстура");

    sqlx::query!(
        "INSERT INTO core.investment_acceptances
           (contract_id, act_date, accepted_amount, accepted_by)
         VALUES ($1, $2, 10000000.00, $3)",
        f.contract_id,
        date!(2026 - 09 - 01),
        f.actor_id
    )
    .execute(&mut *tx)
    .await
    .expect("акт приемки");

    let edited = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.investment_acceptances SET accepted_amount = 30000000.00
             WHERE contract_id = $1",
            f.contract_id
        ),
        "правка акта приемки обязана быть отклонена"
    );
    assert!(edited.contains("FR-1204"), "{edited}");

    let deleted = rejected!(
        tx,
        sqlx::query!(
            "DELETE FROM core.investment_acceptances WHERE contract_id = $1",
            f.contract_id
        ),
        "удаление акта приемки обязано быть отклонено"
    );
    assert!(
        deleted.contains("FR-1204") || deleted.contains("permission denied"),
        "{deleted}"
    );

    // `!`: сумму дает функция, а результат функции планировщик считает
    // потенциально NULL
    let accepted = sqlx::query_scalar!(
        r#"SELECT core.investment_accepted($1) AS "accepted!""#,
        f.contract_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("принятый объем");
    assert_eq!(accepted, Decimal::from(10_000_000));
}

/// Продление (п. 93): только при полном исполнении, от 30 млн ₸ и однократно.
#[tokio::test]
async fn extension_requires_full_performance_and_happens_once() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, Decimal::from(30_000_000), 60)
        .await
        .expect("фикстура");

    macro_rules! extend {
        ($contract_id:expr, $months:expr) => {
            sqlx::query!(
                "UPDATE core.investment_contracts
                 SET extended_at = now(), extension_months = $2 WHERE contract_id = $1",
                $contract_id,
                $months
            )
        };
    }

    let partial = rejected!(
        tx,
        extend!(f.contract_id, EXTENSION_MONTHS),
        "продление без исполнения обязано быть отклонено"
    );
    assert!(partial.contains("исполнены не полностью"), "{partial}");

    sqlx::query!(
        "INSERT INTO core.investment_acceptances
           (contract_id, act_date, accepted_amount, accepted_by)
         VALUES ($1, $2, 30000000.00, $3)",
        f.contract_id,
        date!(2026 - 12 - 01),
        f.actor_id
    )
    .execute(&mut *tx)
    .await
    .expect("акт приемки на полный объем");

    extend!(f.contract_id, EXTENSION_MONTHS)
        .execute(&mut *tx)
        .await
        .expect("продление при полном исполнении");

    // Оформленное продление не переписывается: ни момент, ни срок.
    // Повтор в одной транзакции дал бы тот же `now()`, поэтому проверяется
    // именно правка факта - так «однократно» не обойти правкой строки.
    let twice = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.investment_contracts
             SET extended_at = now() + interval '1 day' WHERE contract_id = $1",
            f.contract_id
        ),
        "повторное продление обязано быть отклонено"
    );
    assert!(twice.contains("однократно"), "{twice}");

    let months = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.investment_contracts SET extension_months = 36 * 2
             WHERE contract_id = $1",
            f.contract_id
        ),
        "правка срока продления обязана быть отклонена"
    );
    assert!(months.contains("однократно"), "{months}");

    // Пролонгация требует своего порога - 100 млн ₸ (п. 93)
    let prolongation = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.investment_contracts
             SET prolonged_at = now(), prolongation_months = 60 WHERE contract_id = $1",
            f.contract_id
        ),
        "пролонгация ниже порога обязана быть отклонена"
    );
    assert!(prolongation.contains("100 млн"), "{prolongation}");

    // Заявка договора осталась удовлетворенной - связь не потеряна
    let status = sqlx::query_scalar!(
        r#"SELECT status::text AS "status!" FROM core.special_requests WHERE id = $1"#,
        f.request_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("состояние заявки");
    assert_eq!(status, "granted");
}

/// Пролонгация (п. 93) - от 100 млн ₸ при полном исполнении.
#[tokio::test]
async fn prolongation_needs_a_hundred_million() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, Decimal::from(100_000_000), 84)
        .await
        .expect("фикстура");

    sqlx::query!(
        "INSERT INTO core.investment_acceptances
           (contract_id, act_date, accepted_amount, accepted_by)
         VALUES ($1, $2, 100000000.00, $3)",
        f.contract_id,
        date!(2027 - 01 - 15),
        f.actor_id
    )
    .execute(&mut *tx)
    .await
    .expect("акт приемки");

    sqlx::query!(
        "UPDATE core.investment_contracts
         SET prolonged_at = now(), prolongation_months = 84 WHERE contract_id = $1",
        f.contract_id
    )
    .execute(&mut *tx)
    .await
    .expect("пролонгация при объеме от 100 млн ₸");
}
