//! Депозит по договору (T74, FR-1003, п. 132–136) против живой БД.
//!
//! До этой задачи от FR-1003 в системе был только вид счета: депозит
//! нигде не открывался, сроки п. 132, 135 и 136 не ставились, а внести
//! или вернуть его было нечем. Здесь проверяется весь цикл: заключение
//! договора открывает счет и срок, платеж равен месячной плате, списание
//! долга открывает срок восполнения, а возврат возможен только после
//! возврата объекта по акту.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use rust_decimal::Decimal;
use sqlx::Acquire as _;
use tou_db::ledger;
use tou_domain::ledger::LedgerOp;
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - депозит не проверялся");
                return;
            }
        }
    };
}

/// Дата платежа по выписке: в тесте - сегодняшняя по местному календарю
/// (NFR-03). Часы берутся у БД, а не у процесса (ADR-0005).
async fn paid_today(tx: &mut sqlx::PgConnection) -> Result<time::Date, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT (core.now() AT TIME ZONE 'Asia/Almaty')::date AS "day!""#)
        .fetch_one(tx)
        .await
}

/// Месячная плата договора фикстуры - она же размер депозита (п. 132).
const MONTHLY_RATE: i64 = 60_500;

struct Fixture {
    contract_id: Uuid,
    tenant_id: Uuid,
    finance_id: Uuid,
}

/// Договор в состоянии «подписан обеими сторонами»: регистрации еще нет,
/// ее выполняет сам тест - именно она открывает депозит.
async fn fixture(tx: &mut sqlx::PgConnection) -> Result<Fixture, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let mut user = async |role: &str| -> Result<Uuid, sqlx::Error> {
        sqlx::query_scalar!(
            "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
             VALUES ($1::citext, 'x', $2, core.now()) RETURNING id",
            format!("t74-{role}-{tag}@tou.test"),
            format!("T74 {role}")
        )
        .fetch_one(&mut *tx)
        .await
    };
    let organizer = user("org").await?;
    let tenant = user("tenant").await?;
    let finance = user("finance").await?;

    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id)
         VALUES ('T74 тендер', 'summed_up', $1) RETURNING id",
        organizer
    )
    .fetch_one(&mut *tx)
    .await?;

    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'T74 объект', 'адрес', 12.00) RETURNING id"
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

    // Договор с пройденными шагами п. 110–114: регистрация требует подписей
    // обеих сторон (FR-905), а до нее депозита быть не должно
    let contract_id = sqlx::query_scalar!(
        "INSERT INTO core.contracts
           (tender_id, lot_id, object_id, tenant_id, monthly_rate, lease_months,
            status, place, drafted_at, tenant_signed_at, documents_received_at)
         VALUES ($1, $2, $3, $4, $5, 12, 'draft', 'winner',
                 core.now(), core.now(), core.now())
         RETURNING id",
        tender_id,
        lot_id,
        object_id,
        tenant,
        Decimal::from(MONTHLY_RATE)
    )
    .fetch_one(&mut *tx)
    .await?;

    // Подпись наймодателя - только после завершенной сверки (INV-115):
    // договор, рожденный подписанным, обходил бы этот рубеж
    sqlx::query!(
        "INSERT INTO core.contract_checklists (contract_id, item_code, checked_at)
         VALUES ($1, 'bank_details', core.now())",
        contract_id
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "UPDATE core.contracts SET landlord_signed_at = core.now() WHERE id = $1",
        contract_id
    )
    .execute(&mut *tx)
    .await?;

    Ok(Fixture {
        contract_id,
        tenant_id: tenant,
        finance_id: finance,
    })
}

async fn deposit_balance(
    tx: &mut sqlx::PgConnection,
    contract_id: Uuid,
) -> Result<Option<Decimal>, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT COALESCE(sum(e.credit - e.debit), 0)::numeric(14,2) AS "balance!"
             FROM core.ledger_accounts acc
             LEFT JOIN core.ledger_entries e ON e.account_id = acc.id
            WHERE acc.kind = 'contract_deposit' AND acc.contract_id = $1
            GROUP BY acc.id"#,
        contract_id
    )
    .fetch_optional(tx)
    .await
}

async fn open_obligation(
    tx: &mut sqlx::PgConnection,
    contract_id: Uuid,
    action: &str,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "open!" FROM core.obligations
            WHERE contract_id = $1 AND action = $2 AND status <> 'done'"#,
        contract_id,
        action
    )
    .fetch_one(tx)
    .await
}

/// Регистрация договора открывает депозитный счет и срок внесения
/// (FR-1003, FR-905, п. 126, 132).
#[tokio::test]
async fn registration_opens_the_deposit_and_its_term() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("fixture");

    assert_eq!(
        deposit_balance(&mut tx, f.contract_id).await.expect("счет"),
        None,
        "до заключения договора депозитного счета нет"
    );

    tou_db::contracts::register_on(
        &mut tx,
        f.tenant_id,
        f.contract_id,
        &format!("Д-{}", Uuid::now_v7().simple()),
    )
    .await
    .expect("регистрация договора");

    assert_eq!(
        deposit_balance(&mut tx, f.contract_id).await.expect("счет"),
        Some(Decimal::ZERO),
        "счет открыт и пуст: обязанность есть, денег еще нет"
    );
    assert_eq!(
        open_obligation(&mut tx, f.contract_id, "deposit_payment")
            .await
            .expect("сроки"),
        1,
        "срок внесения депозита открыт (п. 132)"
    );
}

/// Депозит равен месячной плате (п. 132): частичный платеж отклоняется,
/// полный - закрывает срок.
#[tokio::test]
async fn deposit_equals_the_monthly_rate() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("fixture");
    let today = paid_today(&mut tx).await.expect("дата");
    tou_db::contracts::register_on(&mut tx, f.tenant_id, f.contract_id, "Д-T74-2")
        .await
        .expect("регистрация");

    // Частичный платеж: правило объясняется словами, а не кодом ошибки
    let mut sp = tx.begin().await.expect("savepoint");
    let partial = ledger::pay_deposit_on(
        &mut sp,
        f.finance_id,
        f.contract_id,
        Decimal::from(MONTHLY_RATE / 2),
        today,
        None,
    )
    .await;
    let Err(partial) = partial else {
        panic!("частичный депозит принимать нельзя (п. 132)")
    };
    assert!(
        partial.to_string().contains("п. 132"),
        "отказ обязан ссылаться на правило: {partial}"
    );
    sp.rollback().await.expect("rollback");

    let account = ledger::pay_deposit_on(
        &mut tx,
        f.finance_id,
        f.contract_id,
        Decimal::from(MONTHLY_RATE),
        today,
        Some("платежное поручение № 1"),
    )
    .await
    .expect("полный депозит принимается");
    assert_eq!(account.balance, Decimal::from(MONTHLY_RATE));
    assert_eq!(
        open_obligation(&mut tx, f.contract_id, "deposit_payment")
            .await
            .expect("сроки"),
        0,
        "внесение закрывает свой срок (FR-1702)"
    );
}

/// Списание в счет долга (п. 134) открывает срок восполнения (п. 135),
/// а восполнение его закрывает.
#[tokio::test]
async fn writeoff_opens_the_top_up_term() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("fixture");
    let today = paid_today(&mut tx).await.expect("дата");
    tou_db::contracts::register_on(&mut tx, f.tenant_id, f.contract_id, "Д-T74-3")
        .await
        .expect("регистрация");
    let account = ledger::pay_deposit_on(
        &mut tx,
        f.finance_id,
        f.contract_id,
        Decimal::from(MONTHLY_RATE),
        today,
        None,
    )
    .await
    .expect("депозит внесен");

    ledger::record_on(
        &mut tx,
        f.finance_id,
        account.id,
        LedgerOp::Writeoff,
        Decimal::from(10_000),
        "п. 134",
        Some("долг по плате за август"),
    )
    .await
    .expect("списание долга");

    assert_eq!(
        open_obligation(&mut tx, f.contract_id, "deposit_top_up")
            .await
            .expect("сроки"),
        1,
        "списание открывает срок восполнения (п. 135)"
    );

    ledger::record_on(
        &mut tx,
        f.finance_id,
        account.id,
        LedgerOp::Replenish,
        Decimal::from(10_000),
        "п. 135",
        None,
    )
    .await
    .expect("восполнение");

    assert_eq!(
        open_obligation(&mut tx, f.contract_id, "deposit_top_up")
            .await
            .expect("сроки"),
        0,
        "восполнение закрывает свой срок"
    );
    assert_eq!(
        deposit_balance(&mut tx, f.contract_id).await.expect("счет"),
        Some(Decimal::from(MONTHLY_RATE)),
        "депозит снова равен месячной плате"
    );
}

/// Возврат депозита возможен только после возврата объекта (п. 136).
#[tokio::test]
async fn deposit_returns_only_after_the_object_is_back() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("fixture");
    let today = paid_today(&mut tx).await.expect("дата");
    tou_db::contracts::register_on(&mut tx, f.tenant_id, f.contract_id, "Д-T74-4")
        .await
        .expect("регистрация");
    ledger::pay_deposit_on(
        &mut tx,
        f.finance_id,
        f.contract_id,
        Decimal::from(MONTHLY_RATE),
        today,
        None,
    )
    .await
    .expect("депозит внесен");

    let mut sp = tx.begin().await.expect("savepoint");
    let early = ledger::refund_deposit_on(&mut sp, f.finance_id, f.contract_id, None).await;
    let Err(early) = early else {
        panic!("возврат до возврата объекта невозможен (п. 136)")
    };
    assert!(
        early.to_string().contains("п. 136"),
        "отказ ссылается на правило: {early}"
    );
    sp.rollback().await.expect("rollback");

    // Объект передан и возвращен - акты идут в своем порядке (FR-904)
    for kind in ["handover", "return"] {
        sqlx::query!(
            "INSERT INTO core.acts (contract_id, kind, act_date, created_by)
             VALUES ($1, $2::text::core.act_kind,
                     (core.now() AT TIME ZONE 'Asia/Almaty')::date, $3)",
            f.contract_id,
            kind,
            f.tenant_id
        )
        .execute(&mut *tx)
        .await
        .expect("акт");
    }

    let account = ledger::refund_deposit_on(&mut tx, f.finance_id, f.contract_id, None)
        .await
        .expect("возврат после возврата объекта");
    assert_eq!(
        account.balance,
        Decimal::ZERO,
        "возвращается весь остаток (п. 136)"
    );
}
