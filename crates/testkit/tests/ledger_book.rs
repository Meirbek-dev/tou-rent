//! Депозитная книга против живой БД (T21, FR-1001–1004).
//!
//! Проверяется то, что должна стеречь СУБД, а не приложение: двойная запись
//! (INV-DB-05), неотрицательный баланс, неизменяемость проводок и связь
//! возврата с закрытым перечнем оснований п. 26.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use rust_decimal::Decimal;
use sqlx::Acquire as _;
use tou_db::ledger;
use tou_domain::ledger::{LedgerOp, RefundReason};
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - книга не проверялась");
                return;
            }
        }
    };
}

/// Ожидаемый отказ БД внутри вложенной транзакции: без savepoint первый же
/// отказ обрывает транзакцию теста и следующие проверки не выполнить.
macro_rules! rejected {
    ($tx:expr, $query:expr, $why:expr) => {{
        let mut sp = $tx.begin().await.expect("savepoint");
        let error = $query.execute(&mut *sp).await.expect_err($why);
        sp.rollback().await.expect("rollback savepoint");
        error.to_string()
    }};
}

/// Полная фикстура книги: участник, тендер с лотом, заявка, счет взноса
/// и подтвержденное поступление. Счет по построению привязан к заявке
/// (CHECK `account_binding`), поэтому «пустых» счетов в тестах нет.
struct Fixture {
    tender_id: Uuid,
    application_id: Uuid,
    account_id: Uuid,
    owner_id: Uuid,
}

async fn fixture(tx: &mut sqlx::PgConnection, amount: Decimal) -> Result<Fixture, sqlx::Error> {
    let mut user = async |tag: &str| -> Result<Uuid, sqlx::Error> {
        sqlx::query_scalar!(
            "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
             VALUES ($1, 'x', $2, now()) RETURNING id",
            format!("t21-{tag}-{}@tou.test", Uuid::now_v7().simple()),
            format!("Т21 {tag}")
        )
        .fetch_one(&mut *tx)
        .await
    };
    let owner_id = user("участник").await?;
    let organizer_id = user("организатор").await?;

    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id)
         VALUES ('Т21 тендер', 'draft', $1) RETURNING id",
        organizer_id
    )
    .fetch_one(&mut *tx)
    .await?;

    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Т21 объект', 'адрес', 10.00) RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await?;

    let lot_id = sqlx::query_scalar!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                base_rate_monthly, guarantee_fee, rate_calculation)
         VALUES ($1, 1, $2, 'офис', 12, $3, $3, '{}'::jsonb) RETURNING id",
        tender_id,
        object_id,
        amount
    )
    .fetch_one(&mut *tx)
    .await?;

    let application_id = sqlx::query_scalar!(
        "INSERT INTO core.applications
           (tender_id, lot_id, participant_id, status, applicant_kind, applicant_details)
         VALUES ($1, $2, $3, 'submitted', 'legal_entity', '{}'::jsonb) RETURNING id",
        tender_id,
        lot_id,
        owner_id
    )
    .fetch_one(&mut *tx)
    .await?;

    let account_id = sqlx::query_scalar!(
        "INSERT INTO core.ledger_accounts (kind, application_id, owner_user_id)
         VALUES ('participant_fee', $1, $2) RETURNING id",
        application_id,
        owner_id
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query!(
        "INSERT INTO core.ledger_entries (account_id, op, credit, rule_ref, paid_at)
         VALUES ($1, 'receipt_confirmed', $2, 'п. 23, 25', current_date)",
        account_id,
        amount
    )
    .execute(&mut *tx)
    .await?;

    Ok(Fixture {
        tender_id,
        application_id,
        account_id,
        owner_id,
    })
}

/// INV-DB-05: баланс счета не уходит в минус, сколько бы ни списывали.
#[tokio::test]
async fn inv_db05_balance_never_goes_negative() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let account = fixture(&mut tx, Decimal::from(50_000))
        .await
        .expect("счет с поступлением")
        .account_id;

    // Списание в пределах остатка проходит
    sqlx::query!(
        "INSERT INTO core.ledger_entries (account_id, op, debit, rule_ref)
         VALUES ($1, 'writeoff', 20000, 'п. 134')",
        account
    )
    .execute(&mut *tx)
    .await
    .expect("списание в пределах остатка");

    let error = rejected!(
        tx,
        sqlx::query!(
            "INSERT INTO core.ledger_entries (account_id, op, debit, rule_ref, refund_reason)
             VALUES ($1, 'refund', 40000, 'п. 26.3', 'not_admitted')",
            account
        ),
        "списание сверх остатка обязано быть отклонено"
    );
    assert!(
        error.contains("INV-DB-05"),
        "ожидали отказ INV-DB-05, получили: {error}"
    );
}

/// Двойная запись: у проводки заполнена ровно одна сторона (INV-DB-05),
/// а направление задано типом операции.
#[tokio::test]
async fn inv_db05_entry_has_exactly_one_side() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let account = fixture(&mut tx, Decimal::from(10_000))
        .await
        .expect("счет")
        .account_id;

    let both = rejected!(
        tx,
        sqlx::query!(
            "INSERT INTO core.ledger_entries (account_id, op, debit, credit, rule_ref)
             VALUES ($1, 'offset', 100, 100, 'п. 133')",
            account
        ),
        "обе стороны сразу - не двойная запись"
    );
    assert!(both.contains("debit_xor_credit"), "{both}");

    // Возврат идет только в дебет: «возврат в кредит» отклоняет CHECK
    let wrong_side = rejected!(
        tx,
        sqlx::query!(
            "INSERT INTO core.ledger_entries (account_id, op, credit, rule_ref, refund_reason)
             VALUES ($1, 'refund', 100, 'п. 26.3', 'not_admitted')",
            account
        ),
        "направление операции задано ее типом"
    );
    assert!(wrong_side.contains("op_direction"), "{wrong_side}");
}

/// Проводки неизменяемы: книга - доказательная база (append-only).
/// Первый рубеж - отзыв прав у роли приложения, второй - триггер
/// `forbid_mutation` (он же защищает от правки владельцем БД).
#[tokio::test]
async fn ledger_entries_are_append_only() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let account = fixture(&mut tx, Decimal::from(10_000))
        .await
        .expect("счет")
        .account_id;

    let updated = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.ledger_entries SET credit = 1 WHERE account_id = $1",
            account
        ),
        "правка проводки обязана быть отклонена"
    );
    assert!(
        updated.contains("INV-DB-05") || updated.contains("permission denied"),
        "правка проводки должна отклоняться правами или триггером: {updated}"
    );

    let deleted = rejected!(
        tx,
        sqlx::query!(
            "DELETE FROM core.ledger_entries WHERE account_id = $1",
            account
        ),
        "удаление проводки обязано быть отклонено"
    );
    assert!(
        deleted.contains("INV-DB-05") || deleted.contains("permission denied"),
        "удаление проводки должно отклоняться правами или триггером: {deleted}"
    );

    // Проводка осталась на месте - независимо от того, какой рубеж сработал
    let credit = sqlx::query_scalar!(
        "SELECT credit FROM core.ledger_entries WHERE account_id = $1 AND op = 'receipt_confirmed'",
        account
    )
    .fetch_one(&mut *tx)
    .await
    .expect("проводка");
    assert_eq!(credit, Decimal::from(10_000));
}

/// FR-1002: возврат оформляется только с основанием из перечня п. 26,
/// а прочие операции - без него.
#[tokio::test]
async fn refund_requires_a_reason_from_the_closed_list() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let account = fixture(&mut tx, Decimal::from(10_000))
        .await
        .expect("счет")
        .account_id;

    let no_reason = rejected!(
        tx,
        sqlx::query!(
            "INSERT INTO core.ledger_entries (account_id, op, debit, rule_ref)
             VALUES ($1, 'refund', 100, 'п. 26')",
            account
        ),
        "возврат без основания обязан быть отклонен"
    );
    // Правило то же, что и было; изменился только его уровень: с T74 оно
    // живет в триггере, потому что смотрит на тип счета (у депозита свои
    // основания - п. 136, FR-1003), а CHECK подзапросов не допускает
    assert!(no_reason.contains("п. 26"), "{no_reason}");

    let unknown = rejected!(
        tx,
        sqlx::query!(
            "INSERT INTO core.ledger_entries (account_id, op, debit, rule_ref, refund_reason)
             VALUES ($1, 'refund', 100, 'п. 26', 'выдуманное')",
            account
        ),
        "основание вне справочника обязано быть отклонено"
    );
    assert!(unknown.contains("refund_reason"), "{unknown}");

    // Перечень закрыт и совпадает с enum домена (FR-1002)
    let codes = sqlx::query_scalar!("SELECT code FROM refdata.refund_reasons ORDER BY code")
        .fetch_all(&mut *tx)
        .await
        .expect("справочник");
    let mut expected: Vec<String> = RefundReason::ALL
        .iter()
        .map(|reason| reason.as_str().to_owned())
        .collect();
    expected.sort();
    assert_eq!(codes, expected, "справочник и enum домена разошлись");
}

/// Паритет операций книги: enum домена и enum БД описывают одно и то же.
#[tokio::test]
async fn ledger_ops_match_the_database_enum() {
    let db = require_db!();

    let ops = sqlx::query_scalar!(
        r#"SELECT unnest(enum_range(NULL::core.ledger_op))::text AS "op!" ORDER BY 1"#
    )
    .fetch_all(&db)
    .await
    .expect("enum БД");

    let mut expected: Vec<String> = LedgerOp::ALL
        .iter()
        .map(|op| op.as_str().to_owned())
        .collect();
    expected.sort();
    assert_eq!(ops, expected);
}

/// Возврат через слой данных закрывает срок п. 26 (FR-1702 + FR-1002).
#[tokio::test]
async fn refund_closes_the_deadline() {
    let db = require_db!();

    // Транзакция здесь не годится: возврат ходит своим подключением
    let mut setup = db.begin().await.expect("begin");
    let f = fixture(&mut setup, Decimal::from(50_000))
        .await
        .expect("фикстура");
    tou_db::obligations::schedule(
        &mut setup,
        tou_domain::obligation::ObligationAction::FeeRefund,
        tou_db::obligations::Subject {
            application_id: Some(f.application_id),
            ..Default::default()
        },
    )
    .await
    .expect("срок возврата");
    setup.commit().await.expect("commit");

    let account = ledger::refund_fee(
        &db,
        f.owner_id,
        f.application_id,
        RefundReason::NotAdmitted,
        None,
    )
    .await
    .expect("возврат");
    assert_eq!(account.balance, Decimal::ZERO, "возвращается весь остаток");
    assert_eq!(account.id, f.account_id);

    let status = sqlx::query_scalar!(
        r#"SELECT status::text AS "status!" FROM core.obligations WHERE application_id = $1"#,
        f.application_id
    )
    .fetch_one(&db)
    .await
    .expect("срок");
    assert_eq!(status, "done", "исполненный возврат закрывает срок п. 26");

    sqlx::query!("DELETE FROM core.tenders WHERE id = $1", f.tender_id)
        .execute(&db)
        .await
        .ok();
}
