//! Реестры отчетности против живой БД (T43, арх. § 9).
//!
//! Реестр - выборка уже записанных фактов, поэтому проверяется именно это:
//! решение, договор и поступление попадают в свой реестр, период их
//! отсекает, а незарегистрированный договор в реестр не идет (п. 126).
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use time::{Date, Duration};
use tou_db::reports::{self, Period};
use tou_domain::report::{Registry, to_csv};
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - реестры не проверялись");
                return;
            }
        }
    };
}

/// Сегодня по часам сервера (`core.now()`, ADR-0005), а не процесса:
/// со сдвинутыми часами стенда даты разошлись бы, и тест сравнивал бы
/// свой день с чужим.
async fn today(tx: &mut sqlx::PgConnection) -> Result<Date, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT core.now()::date AS "day!""#)
        .fetch_one(tx)
        .await
}

/// Решение Правления по заявке особого порядка (п. 90).
async fn special_decision(tx: &mut sqlx::PgConnection) -> Result<Uuid, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let applicant = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т43 заявитель', now()) RETURNING id",
        format!("t43-applicant-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let request = sqlx::query_scalar!(
        "INSERT INTO core.special_requests
           (applicant_id, category, applicant_kind, applicant_details, purpose)
         VALUES ($1, 'category_4', 'legal_entity', '{}'::jsonb, 'реестр решений')
         RETURNING id",
        applicant
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query!(
        "INSERT INTO core.special_reviews
           (special_request_id, reviewer_id, conclusion, recommendation)
         VALUES ($1, $2, 'соответствует', 'grant')",
        request,
        applicant
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        "INSERT INTO core.special_board_decisions
           (special_request_id, decision, rationale, decided_by)
         VALUES ($1, 'grant', 'Т43 обоснование решения', $2)",
        request,
        applicant
    )
    .execute(&mut *tx)
    .await?;

    Ok(request)
}

/// Зарегистрированный договор (п. 126) и подтвержденное поступление взноса.
async fn registered_contract(tx: &mut sqlx::PgConnection) -> Result<Uuid, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let tenant = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т43 наниматель', now()) RETURNING id",
        format!("t43-tenant-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let object = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Т43 помещение', 'г. Павлодар, ул. Тестовая, 4', 30)
         RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await?;

    let contract = sqlx::query_scalar!(
        "INSERT INTO core.contracts
           (object_id, tenant_id, monthly_rate, status, lease_period,
            drafted_at, tenant_signed_at, documents_received_at)
         VALUES ($1, $2, 79750, 'active',
                 tstzrange(now(), now() + interval '365 days'),
                 core.now(), core.now(), core.now())
         RETURNING id",
        object,
        tenant
    )
    .fetch_one(&mut *tx)
    .await?;

    // Регистрация требует завершенной сверки и обеих подписей (INV-115,
    // FR-905): договор доводится до нее шагами, а не рождается
    // зарегистрированным - иначе фикстура обходит те самые сторожа
    sqlx::query!(
        "INSERT INTO core.contract_checklists (contract_id, item_code, checked_at)
         VALUES ($1, 'bank_details', core.now())",
        contract
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "UPDATE core.contracts
         SET landlord_signed_at = core.now(), registered_at = core.now(), reg_number = $2
         WHERE id = $1",
        contract,
        format!("Д-Т43-{tag}")
    )
    .execute(&mut *tx)
    .await?;

    let account = sqlx::query_scalar!(
        "INSERT INTO core.ledger_accounts (kind, contract_id, owner_user_id)
         VALUES ('contract_deposit', $1, $2) RETURNING id",
        contract,
        tenant
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query!(
        "INSERT INTO core.ledger_entries
           (account_id, op, credit, rule_ref, recorded_by, paid_at)
         VALUES ($1, 'receipt_confirmed', 79750, 'п. 132', $2, current_date)",
        account,
        tenant
    )
    .execute(&mut *tx)
    .await?;

    Ok(contract)
}

/// Арх. § 9: решение попадает в реестр решений и отсекается периодом.
#[tokio::test]
async fn decisions_registry_lists_board_decisions() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    special_decision(&mut tx).await.expect("решение");
    let day = today(&mut tx).await.expect("дата сервера");

    let rows = reports::decisions(
        &mut tx,
        Period {
            from: Some(day),
            to: Some(day),
        },
    )
    .await
    .expect("реестр решений");

    let ours = rows
        .iter()
        .find(|row| row.rationale == "Т43 обоснование решения")
        .expect("решение в реестре за сегодня");
    assert_eq!(ours.order_kind, "special");
    assert_eq!(ours.decision, "grant");
    assert!(ours.subject.contains("п. 87.4"), "{}", ours.subject);

    // Прошлый период сегодняшнего решения не показывает
    let past = reports::decisions(
        &mut tx,
        Period {
            from: Some(day - Duration::days(30)),
            to: Some(day - Duration::days(1)),
        },
    )
    .await
    .expect("реестр решений за прошлый период");
    assert!(
        !past
            .iter()
            .any(|row| row.rationale == "Т43 обоснование решения"),
        "период отсекает решения вне его границ"
    );
}

/// Арх. § 9 (п. 126): в реестр договоров идут зарегистрированные договоры.
#[tokio::test]
async fn contracts_registry_lists_registered_contracts_only() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let contract = registered_contract(&mut tx).await.expect("договор");

    let rows = reports::contracts(&mut tx, Period::default())
        .await
        .expect("реестр договоров");
    let ours = rows
        .iter()
        .find(|row| row.object_name == "Т43 помещение")
        .expect("договор в реестре");
    assert_eq!(ours.status, "active");
    assert_eq!(ours.source, "other", "договор без тендера и раздела 12");
    assert!(ours.reg_number.is_some(), "реестр ведется по номерам");
    assert!(ours.lease_from.is_some() && ours.lease_to.is_some());

    // Снятая регистрация убирает договор из реестра (п. 126)
    sqlx::query!(
        "UPDATE core.contracts SET registered_at = NULL WHERE id = $1",
        contract
    )
    .execute(&mut *tx)
    .await
    .expect("снятие регистрации");

    let rows = reports::contracts(&mut tx, Period::default())
        .await
        .expect("реестр договоров");
    assert!(
        !rows.iter().any(|row| row.object_name == "Т43 помещение"),
        "незарегистрированный договор в реестр не идет"
    );
}

/// Арх. § 9 (FR-1001): в реестр поступлений идут приходные проводки.
#[tokio::test]
async fn receipts_registry_lists_incoming_entries() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    registered_contract(&mut tx).await.expect("договор и взнос");

    let rows = reports::receipts(&mut tx, Period::default())
        .await
        .expect("реестр поступлений");

    let ours = rows
        .iter()
        .find(|row| row.payer.as_deref() == Some("Т43 наниматель"))
        .expect("поступление в реестре");
    assert_eq!(ours.account_kind, "contract_deposit");
    assert_eq!(ours.amount.to_string(), "79750.00");
    assert_eq!(ours.rule_ref.as_deref(), Some("п. 132"));
}

/// Выгрузка складывается из тех же колонок и строк (арх. § 9).
#[tokio::test]
async fn csv_export_matches_the_registry_columns() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    registered_contract(&mut tx).await.expect("договор");

    let rows: Vec<Vec<String>> = reports::contracts(&mut tx, Period::default())
        .await
        .expect("реестр договоров")
        .into_iter()
        .map(|row| {
            vec![
                row.reg_number.unwrap_or_default(),
                row.registered_at
                    .map(|at| at.to_string())
                    .unwrap_or_default(),
                row.object_name,
                row.tenant_name.unwrap_or_default(),
                row.monthly_rate.to_string(),
                String::new(),
                row.status,
                row.source,
            ]
        })
        .collect();

    let csv = to_csv(Registry::Contracts, &rows);
    let lines: Vec<&str> = csv.trim_end().split("\r\n").collect();
    assert!(lines[0].contains("Рег. номер"), "шапка на месте");
    assert_eq!(
        lines.len(),
        rows.len() + 1,
        "в выгрузке столько же строк, сколько в реестре"
    );
    assert!(
        csv.contains("Т43 помещение"),
        "строка реестра попала в выгрузку"
    );
}
