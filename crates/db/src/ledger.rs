//! Депозитная книга (М10, FR-405, FR-1001–1004): счета взносов и депозитов,
//! проводки двойной записи, подтверждение поступлений и возвраты.
//!
//! Банк-интеграции нет (арх. § 1, ответ № 11): поступление подтверждает
//! оператор финблока вручную, а система следит за правилами - сумма равна
//! взносу лота (FR-206), деньги пришли не позже чем за два рабочих дня до
//! первого этапа (п. 23), баланс счета не уходит в минус (INV-DB-05).

use rust_decimal::Decimal;
use time::OffsetDateTime;
use tou_domain::ledger::{AccountKind, LedgerOp, RefundReason};
use tou_domain::obligation::ObligationAction;
use tou_domain::rule::{RuleRejection, RuleViolation};
use uuid::Uuid;

use crate::Db;

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("заявка или счет не найдены")]
    NotFound,
    /// Отказ правила: сумма, срок п. 23, баланс INV-DB-05, статус заявки
    #[error("{0}")]
    Rejected(RuleRejection),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

fn map_rule(err: sqlx::Error) -> LedgerError {
    if let sqlx::Error::Database(db_err) = &err
        && matches!(
            db_err.code().as_deref(),
            Some("P0001") | Some("23514") | Some("23503") | Some("23505")
        )
    {
        return LedgerError::Rejected(crate::rule::rejection(db_err.as_ref()));
    }
    LedgerError::Db(err)
}

pub struct AccountRow {
    pub id: Uuid,
    pub kind: String,
    pub application_id: Option<Uuid>,
    pub contract_id: Option<Uuid>,
    pub owner_user_id: Uuid,
    pub owner_name: String,
    /// Тендер и лот счета взноса - книга читается по процессу, а не по id
    pub tender_id: Option<Uuid>,
    pub tender_title: Option<String>,
    pub lot_seq: Option<i32>,
    pub balance: Decimal,
    /// Сколько должно быть на счете: у депозита - месячная плата договора
    /// (FR-1003, п. 132). У счета взноса участника поля нет: там размер
    /// задает лот (FR-206)
    pub required_amount: Option<Decimal>,
}

/// Выборка счета: общий список столбцов + хвост запроса (см. `acts.rs`).
///
/// `!` стоит там, где планировщик считает выражение потенциально NULL, хотя
/// значение есть всегда: `::text` от перечисления и посчитанный баланс.
/// Столбцы из `LEFT JOIN` получают `?`: счет взноса не знает договора,
/// а счет депозита не знает тендера, но sqlx выводит nullability по самому
/// столбцу (все они NOT NULL), а не по виду соединения.
macro_rules! account_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            AccountRow,
            r#"SELECT acc.id, acc.kind::text AS "kind!", acc.application_id, acc.contract_id,
                      acc.owner_user_id, u.full_name AS owner_name,
                      a.tender_id AS "tender_id?", t.title AS "tender_title?",
                      l.seq AS "lot_seq?",
                      COALESCE((SELECT sum(e.credit - e.debit) FROM core.ledger_entries e
                                WHERE e.account_id = acc.id), 0)::numeric(14,2) AS "balance!",
                      c.monthly_rate AS "required_amount?"
               FROM core.ledger_accounts acc
               JOIN core.users u ON u.id = acc.owner_user_id
               LEFT JOIN core.applications a ON a.id = acc.application_id
               LEFT JOIN core.tenders t ON t.id = a.tender_id
               LEFT JOIN core.lots l ON l.id = a.lot_id
               LEFT JOIN core.contracts c ON c.id = acc.contract_id"# + $tail
            $(, $arg)*
        )
    };
}

pub struct EntryRow {
    pub id: Uuid,
    pub op: String,
    pub debit: Decimal,
    pub credit: Decimal,
    pub rule_ref: Option<String>,
    pub refund_reason: Option<String>,
    pub paid_at: Option<time::Date>,
    pub note: Option<String>,
    pub recorded_by_name: Option<String>,
    pub occurred_at: OffsetDateTime,
}

/// Депозитная книга целиком (FR-1001): счета с балансами, свежие сверху.
pub async fn accounts(db: &Db, kind: Option<AccountKind>) -> Result<Vec<AccountRow>, sqlx::Error> {
    account_query!(
        " WHERE ($1::text IS NULL OR acc.kind::text = $1)
          ORDER BY acc.created_at DESC",
        kind.map(|k| k.as_str())
    )
    .fetch_all(db)
    .await
}

/// Выписка по счету (FR-1001): проводки в хронологическом порядке, страницей.
///
/// Журнал счета растет и не сокращается: проводка - факт, ее не удаляют.
/// Поэтому выписка идет курсором вперед по паре «момент проводки + строка»:
/// у проводок одной транзакции `occurred_at` совпадает, и курсора по одному
/// времени не хватило бы.
pub async fn entries(
    db: &Db,
    account_id: Uuid,
    after: Option<crate::RowCursor>,
    limit: i64,
) -> Result<crate::Page<EntryRow>, sqlx::Error> {
    let (after_at, after_id) = crate::RowCursor::parts(after);
    let rows = sqlx::query_as!(
        EntryRow,
        r#"SELECT e.id, e.op::text AS "op!", e.debit, e.credit, e.rule_ref,
                  e.refund_reason, e.paid_at, e.note,
                  u.full_name AS "recorded_by_name?", e.occurred_at
           FROM core.ledger_entries e
           LEFT JOIN core.users u ON u.id = e.recorded_by
           WHERE e.account_id = $1
             AND ($2::timestamptz IS NULL OR (e.occurred_at, e.id) > ($2, $3::uuid))
           ORDER BY e.occurred_at, e.id LIMIT $4"#,
        account_id,
        after_at,
        after_id,
        crate::probe_limit(limit)
    )
    .fetch_all(db)
    .await?;
    let page = crate::Page::probe(rows, limit);
    crate::warn_if_truncated(page.truncated, "ledger::entries");
    Ok(page)
}

/// Счет взноса по заявке (кабинет участника: «мой взнос»).
pub async fn account_of_application(
    db: &Db,
    application_id: Uuid,
) -> Result<Option<AccountRow>, sqlx::Error> {
    account_query!(" WHERE acc.application_id = $1", application_id)
        .fetch_optional(db)
        .await
}

/// Подтверждение поступления гарантийного взноса (FR-405, п. 23).
///
/// Одной транзакцией: счет участника, проводка-поступление и статус заявки
/// «взнос подтвержден». Правила проверяются до записи и объясняются словами:
/// оператор должен понимать, почему платеж не принят.
pub async fn confirm_fee(
    db: &Db,
    actor: Uuid,
    application_id: Uuid,
    amount: Decimal,
    paid_at: time::Date,
) -> Result<AccountRow, LedgerError> {
    crate::with_actor(db, actor, async |tx| {
        // Взнос лота (FR-206 = месячная ставка), участник и время заседания
        let row = sqlx::query!(
            r#"SELECT a.participant_id, a.status::text AS "status!", l.guarantee_fee,
                      t.opening_at,
                      refdata.add_business_days($2::date, 2)
                        <= (t.opening_at AT TIME ZONE 'Asia/Almaty')::date AS in_time
               FROM core.applications a
               JOIN core.lots l ON l.id = a.lot_id
               JOIN core.tenders t ON t.id = a.tender_id
               WHERE a.id = $1"#,
            application_id,
            paid_at
        )
        .fetch_optional(&mut *tx)
        .await?;
        let row = row.ok_or(LedgerError::NotFound)?;

        let participant_id = row.participant_id;
        let status = row.status;
        let guarantee_fee = row.guarantee_fee;
        let opening_at = row.opening_at;
        let in_time = row.in_time;

        if status == "withdrawn" {
            return Err(LedgerError::Rejected(RuleRejection::new(
                RuleViolation::GuaranteeDeposit,
                "заявка отозвана - взнос по ней не подтверждается (п. 43–45)",
            )));
        }
        // Неполный взнос - основание отклонения заявки (п. 52.2), а не «почти оплата»
        if amount != guarantee_fee {
            return Err(LedgerError::Rejected(RuleRejection::new(
                RuleViolation::GuaranteeDeposit,
                format!(
                    "сумма {amount} не равна гарантийному взносу лота {guarantee_fee} - \
                     неполное внесение является основанием отклонения (п. 25, 52.2)"
                ),
            )));
        }
        // Деньги должны прийти не позже чем за два рабочих дня до первого этапа
        if opening_at.is_some() && in_time == Some(false) {
            return Err(LedgerError::Rejected(RuleRejection::new(
                RuleViolation::GuaranteeDeposit,
                "поступление позже установленного срока: взнос вносится не позднее чем \
                 за два рабочих дня до первого этапа (п. 23)",
            )));
        }

        let account_id = sqlx::query_scalar!(
            "INSERT INTO core.ledger_accounts (kind, application_id, owner_user_id)
             VALUES ('participant_fee', $1, $2)
             ON CONFLICT (application_id) DO UPDATE SET application_id = EXCLUDED.application_id
             RETURNING id",
            application_id,
            participant_id
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_rule)?;

        // Повторное подтверждение того же взноса - не вторая оплата
        let already = sqlx::query_scalar!(
            r#"SELECT COALESCE(sum(credit), 0)::numeric(14,2) AS "confirmed!"
               FROM core.ledger_entries
               WHERE account_id = $1 AND op = 'receipt_confirmed'"#,
            account_id
        )
        .fetch_one(&mut *tx)
        .await?;
        if already > Decimal::ZERO {
            return Err(LedgerError::Rejected(RuleRejection::new(
                RuleViolation::GuaranteeDeposit,
                "поступление взноса по этой заявке уже подтверждено",
            )));
        }

        sqlx::query!(
            "INSERT INTO core.ledger_entries
               (account_id, op, credit, rule_ref, paid_at, recorded_by, note)
             VALUES ($1, 'receipt_confirmed', $2, 'п. 23, 25', $3, $4, $5)",
            account_id,
            amount,
            paid_at,
            actor,
            format!("подтверждено оператором финблока {paid_at}")
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;

        sqlx::query!(
            "UPDATE core.applications
             SET status = 'fee_confirmed', fee_confirmed_at = core.now(), fee_confirmed_by = $2
             WHERE id = $1 AND status = 'submitted'",
            application_id,
            actor
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;

        fetch_account(&mut *tx, account_id).await.map_err(map_rule)
    })
    .await
}

async fn fetch_account(
    conn: &mut sqlx::PgConnection,
    account_id: Uuid,
) -> Result<AccountRow, sqlx::Error> {
    account_query!(" WHERE acc.id = $1", account_id)
        .fetch_one(conn)
        .await
}

/// Возврат взноса по основанию п. 26 (FR-1002). Сумма - весь остаток счета:
/// частичных возвратов Правила не знают, а остаток после удержаний уже учтен
/// проводками.
pub async fn refund_fee(
    db: &Db,
    actor: Uuid,
    application_id: Uuid,
    reason: RefundReason,
    note: Option<&str>,
) -> Result<AccountRow, LedgerError> {
    crate::with_actor(db, actor, async |tx| {
        let account = account_query!(" WHERE acc.application_id = $1", application_id)
            .fetch_optional(&mut *tx)
            .await?;
        let account = account.ok_or(LedgerError::NotFound)?;

        if account.balance <= Decimal::ZERO {
            return Err(LedgerError::Rejected(RuleRejection::new(
                RuleViolation::LedgerBalanceNegative,
                "на счете нет остатка: возвращать нечего",
            )));
        }

        let rule_ref = sqlx::query_scalar!(
            "SELECT rule_ref FROM refdata.refund_reasons WHERE code = $1",
            reason.as_str()
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_rule)?;

        sqlx::query!(
            "INSERT INTO core.ledger_entries
               (account_id, op, debit, rule_ref, refund_reason, recorded_by, note)
             VALUES ($1, 'refund', $2, $3, $4, $5, $6)",
            account.id,
            account.balance,
            rule_ref,
            reason.as_str(),
            actor,
            note
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;

        // Возврат исполнен - срок п. 26 закрыт фактом (FR-1702)
        crate::obligations::complete(
            &mut *tx,
            ObligationAction::FeeRefund,
            crate::obligations::Subject {
                application_id: Some(application_id),
                ..Default::default()
            },
        )
        .await?;

        fetch_account(&mut *tx, account.id).await.map_err(map_rule)
    })
    .await
}

/// Прочие операции книги (FR-1001): удержание при уклонении (п. 116), зачет
/// в счет депозита или платы (п. 26.6, 133), списание долга и восполнение
/// депозита (п. 134–135). Направление задает тип операции (домен), баланс
/// стережет триггер (INV-DB-05).
pub async fn record(
    db: &Db,
    actor: Uuid,
    account_id: Uuid,
    op: LedgerOp,
    amount: Decimal,
    rule_ref: &str,
    note: Option<&str>,
) -> Result<AccountRow, LedgerError> {
    if matches!(op, LedgerOp::ReceiptConfirmed | LedgerOp::Refund) {
        return Err(LedgerError::Rejected(RuleRejection::new(
            RuleViolation::LedgerEntry,
            "поступление и возврат оформляются своими операциями (FR-405, FR-1002)",
        )));
    }
    if amount <= Decimal::ZERO {
        return Err(LedgerError::Rejected(RuleRejection::new(
            RuleViolation::LedgerEntry,
            "сумма проводки должна быть положительной",
        )));
    }

    crate::with_actor(db, actor, async |tx| {
        record_on(tx, actor, account_id, op, amount, rule_ref, note).await
    })
    .await
}

/// То же в транзакции вызывающего - вариант `*_on` (арх. v3 § 6).
pub async fn record_on(
    tx: &mut sqlx::PgConnection,
    actor: Uuid,
    account_id: Uuid,
    op: LedgerOp,
    amount: Decimal,
    rule_ref: &str,
    note: Option<&str>,
) -> Result<AccountRow, LedgerError> {
    use tou_domain::ledger::Side;

    {
        let (debit, credit) = match op.side() {
            Side::Debit => (amount, Decimal::ZERO),
            Side::Credit => (Decimal::ZERO, amount),
        };

        sqlx::query!(
            "INSERT INTO core.ledger_entries
               (account_id, op, debit, credit, rule_ref, recorded_by, note)
             VALUES ($1, $2::text::core.ledger_op, $3, $4, $5, $6, $7)",
            account_id,
            op.as_str(),
            debit,
            credit,
            rule_ref,
            actor,
            note
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;

        // Депозит по договору: движение денег двигает и сроки (FR-1003,
        // FR-1702). Списание в счет долга (п. 134) открывает срок
        // восполнения (п. 135), восполнение и зачет его закрывают.
        // Для счета взноса участника ничего этого не происходит - там
        // свои основания (п. 26)
        let contract_id = sqlx::query_scalar!(
            "SELECT contract_id FROM core.ledger_accounts
              WHERE id = $1 AND kind = 'contract_deposit'",
            account_id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_rule)?
        .flatten();

        if let Some(contract_id) = contract_id {
            let subject = crate::obligations::Subject::contract(contract_id);
            match op {
                LedgerOp::Writeoff => {
                    crate::obligations::schedule(&mut *tx, ObligationAction::DepositTopUp, subject)
                        .await?;
                }
                LedgerOp::Replenish => {
                    crate::obligations::complete(&mut *tx, ObligationAction::DepositTopUp, subject)
                        .await?;
                }
                // Зачет взноса в счет депозита (п. 26.6, 133) - тот же
                // способ внести депозит, что и платеж
                LedgerOp::Offset => {
                    crate::obligations::complete(
                        &mut *tx,
                        ObligationAction::DepositPayment,
                        subject,
                    )
                    .await?;
                }
                _ => {}
            }
        }

        fetch_account(&mut *tx, account_id).await.map_err(map_rule)
    }
}

/// Остаток счета в открытой транзакции: баланс считается из проводок,
/// отдельного поля у счета нет (INV-DB-05).
async fn balance_of(tx: &mut sqlx::PgConnection, account_id: Uuid) -> Result<Decimal, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT COALESCE(sum(credit - debit), 0)::numeric(14,2) AS "balance!"
             FROM core.ledger_entries WHERE account_id = $1"#,
        account_id
    )
    .fetch_one(tx)
    .await
}

/// Внесение депозита по договору (FR-1003, п. 132): депозит равен месячной
/// плате, и это правило проверяется здесь, а не доверяется оператору.
///
/// Отдельно от [`record`]: у депозита своя проверка суммы и свой срок,
/// который платеж закрывает. Частичный платеж отклоняется словами правила -
/// оператор должен понимать, почему сумма не принята.
pub async fn pay_deposit(
    db: &Db,
    actor: Uuid,
    contract_id: Uuid,
    amount: Decimal,
    paid_at: time::Date,
    note: Option<&str>,
) -> Result<AccountRow, LedgerError> {
    crate::with_actor(db, actor, async |tx| {
        pay_deposit_on(tx, actor, contract_id, amount, paid_at, note).await
    })
    .await
}

/// То же в транзакции вызывающего - вариант `*_on` (арх. v3 § 6): тест
/// выполняет сценарий целиком и откатывает его, не засоряя стенд.
pub async fn pay_deposit_on(
    tx: &mut sqlx::PgConnection,
    actor: Uuid,
    contract_id: Uuid,
    amount: Decimal,
    paid_at: time::Date,
    note: Option<&str>,
) -> Result<AccountRow, LedgerError> {
    {
        let contract = sqlx::query!(
            r#"SELECT monthly_rate, tenant_id, registered_at IS NOT NULL AS "registered!"
                 FROM core.contracts WHERE id = $1"#,
            contract_id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_rule)?
        .ok_or(LedgerError::NotFound)?;
        let (monthly_rate, tenant_id) = (contract.monthly_rate, contract.tenant_id);

        if !contract.registered {
            return Err(LedgerError::Rejected(RuleRejection::new(
                RuleViolation::ContractDeposit,
                "депозит вносится по заключенному договору (п. 126, 132)",
            )));
        }

        let account_id = open_deposit_account_on(&mut *tx, contract_id, tenant_id).await?;
        let balance = balance_of(&mut *tx, account_id).await.map_err(map_rule)?;
        let due = monthly_rate - balance;

        if amount != due {
            return Err(LedgerError::Rejected(RuleRejection::new(
                RuleViolation::ContractDeposit,
                format!(
                    "депозит равен месячной плате (п. 132): к внесению {due}, получено {amount}"
                ),
            )));
        }

        sqlx::query!(
            "INSERT INTO core.ledger_entries
               (account_id, op, debit, credit, rule_ref, paid_at, recorded_by, note)
             VALUES ($1, 'receipt_confirmed', 0, $2, 'п. 132', $3, $4, $5)",
            account_id,
            amount,
            paid_at,
            actor,
            note
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;

        crate::obligations::complete(
            &mut *tx,
            ObligationAction::DepositPayment,
            crate::obligations::Subject::contract(contract_id),
        )
        .await?;

        fetch_account(&mut *tx, account_id).await.map_err(map_rule)
    }
}

/// Возврат депозита после возврата объекта (FR-1003, п. 136): возвращается
/// весь остаток - частичных возвратов Правила не знают, а удержания уже
/// учтены проводками списания (п. 134).
pub async fn refund_deposit(
    db: &Db,
    actor: Uuid,
    contract_id: Uuid,
    note: Option<&str>,
) -> Result<AccountRow, LedgerError> {
    crate::with_actor(db, actor, async |tx| {
        refund_deposit_on(tx, actor, contract_id, note).await
    })
    .await
}

/// То же в транзакции вызывающего (см. [`pay_deposit_on`]).
pub async fn refund_deposit_on(
    tx: &mut sqlx::PgConnection,
    actor: Uuid,
    contract_id: Uuid,
    note: Option<&str>,
) -> Result<AccountRow, LedgerError> {
    {
        let returned = sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM core.acts
                               WHERE contract_id = $1 AND kind = 'return') AS "returned!""#,
            contract_id
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_rule)?;

        if !returned {
            return Err(LedgerError::Rejected(RuleRejection::new(
                RuleViolation::ContractDeposit,
                "депозит возвращается после возврата объекта по акту (п. 136)",
            )));
        }

        let account_id = sqlx::query_scalar!(
            "SELECT id FROM core.ledger_accounts
              WHERE kind = 'contract_deposit' AND contract_id = $1",
            contract_id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_rule)?
        .ok_or(LedgerError::NotFound)?;

        let balance = balance_of(&mut *tx, account_id).await.map_err(map_rule)?;
        if balance <= Decimal::ZERO {
            return Err(LedgerError::Rejected(RuleRejection::new(
                RuleViolation::LedgerBalanceNegative,
                "на депозитном счете нет остатка к возврату",
            )));
        }

        sqlx::query!(
            "INSERT INTO core.ledger_entries
               (account_id, op, debit, credit, rule_ref, recorded_by, note)
             VALUES ($1, 'refund', $2, 0, 'п. 136', $3, $4)",
            account_id,
            balance,
            actor,
            note
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;

        crate::obligations::complete(
            &mut *tx,
            ObligationAction::DepositRefund,
            crate::obligations::Subject::contract(contract_id),
        )
        .await?;

        fetch_account(&mut *tx, account_id).await.map_err(map_rule)
    }
}

/// Счет депозита по договору (FR-1003, п. 132): депозит равен месячной плате.
/// Открывается при заключении договора; операции - теми же проводками.
pub async fn open_deposit_account(
    db: &Db,
    actor: Uuid,
    contract_id: Uuid,
    owner_user_id: Uuid,
) -> Result<AccountRow, LedgerError> {
    crate::with_actor(db, actor, async |tx| {
        let account_id = open_deposit_account_on(&mut *tx, contract_id, owner_user_id).await?;
        fetch_account(&mut *tx, account_id).await.map_err(map_rule)
    })
    .await
}

/// То же в транзакции вызывающего: счет открывается тем же фактом, что и
/// регистрация договора (FR-905, FR-1003), и обязан жить или умереть
/// вместе с ней.
pub async fn open_deposit_account_on(
    tx: &mut sqlx::PgConnection,
    contract_id: Uuid,
    owner_user_id: Uuid,
) -> Result<Uuid, LedgerError> {
    sqlx::query_scalar!(
        "INSERT INTO core.ledger_accounts (kind, contract_id, owner_user_id)
         VALUES ('contract_deposit', $1, $2)
         ON CONFLICT (contract_id) DO UPDATE SET contract_id = EXCLUDED.contract_id
         RETURNING id",
        contract_id,
        owner_user_id
    )
    .fetch_one(tx)
    .await
    .map_err(map_rule)
}

/// Счет депозита по договору - для кабинета нанимателя и финблока.
pub async fn account_of_contract(
    db: &Db,
    contract_id: Uuid,
) -> Result<Option<AccountRow>, sqlx::Error> {
    account_query!(
        " WHERE acc.kind = 'contract_deposit' AND acc.contract_id = $1",
        contract_id
    )
    .fetch_optional(db)
    .await
}

/// Основания возврата (FR-1002) для формы оператора.
pub async fn refund_reasons(db: &Db) -> Result<Vec<RefundReasonRow>, sqlx::Error> {
    sqlx::query_as!(
        RefundReasonRow,
        "SELECT code, label_ru, label_kk, label_en, rule_ref
         FROM refdata.refund_reasons ORDER BY rule_ref"
    )
    .fetch_all(db)
    .await
}

pub struct RefundReasonRow {
    pub code: String,
    pub label_ru: String,
    pub label_kk: Option<String>,
    pub label_en: Option<String>,
    pub rule_ref: String,
}
