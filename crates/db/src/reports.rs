//! Реестры отчетности (арх. § 9, контур 3): решения, договоры, поступления.
//!
//! Реестр - выборка уже записанных фактов за период, а не отдельная сущность:
//! ничего не хранится и не пересчитывается, поэтому «отчет» всегда сходится
//! с процессом. Формат строк собирает http-слой, здесь - типизированные
//! строки и фильтр периода.
//!
//! Функции работают на соединении, а не на пуле: так их видно из транзакции
//! теста против живой БД (A-021).

use rust_decimal::Decimal;
use time::{Date, OffsetDateTime};

/// Период реестра: границы включительно, обе - необязательные.
#[derive(Debug, Clone, Copy, Default)]
pub struct Period {
    pub from: Option<Date>,
    pub to: Option<Date>,
}

/// Строка реестра решений (п. 90, 106).
pub struct DecisionRow {
    pub decided_at: OffsetDateTime,
    /// `special` - особый порядок (п. 90), `land` - земельный участок (п. 106)
    pub order_kind: String,
    pub subject: String,
    pub applicant: Option<String>,
    pub decision: String,
    pub rationale: String,
}

/// Реестр решений Правления: особый порядок и земельные участки.
///
/// `!` у литерала, конкатенации и `::text`: планировщик считает такие
/// выражения потенциально NULL, хотя ни одно из них NULL не дает.
/// `applicant` получает `?`: sqlx выводит nullability по самому столбцу
/// (`core.users.full_name` - NOT NULL), а не по виду соединения, и без
/// аннотации решил бы, что за LEFT JOIN NULL прийти не может.
pub async fn decisions(
    conn: &mut sqlx::PgConnection,
    period: Period,
) -> Result<Vec<DecisionRow>, sqlx::Error> {
    let rows = sqlx::query_as!(
        DecisionRow,
        r#"SELECT d.decided_at AS "decided_at!", 'special' AS "order_kind!",
                c.label_ru || ' (' || c.rule_ref || ')' AS "subject!",
                u.full_name AS "applicant?", d.decision::text AS "decision!",
                d.rationale AS "rationale!"
         FROM core.special_board_decisions d
         JOIN core.special_requests r ON r.id = d.special_request_id
         JOIN refdata.special_categories c ON c.code = r.category
         LEFT JOIN core.users u ON u.id = r.applicant_id
         WHERE ($1::date IS NULL OR d.decided_at >= $1::date)
           AND ($2::date IS NULL OR d.decided_at < $2::date + 1)
         UNION ALL
         SELECT l.decided_at, 'land', o.name,
                u.full_name, l.decision::text, l.rationale
         FROM core.land_decisions l
         JOIN core.land_applications a ON a.id = l.land_application_id
         JOIN core.objects o ON o.id = a.plot_id
         LEFT JOIN core.users u ON u.id = a.investor_id
         WHERE ($1::date IS NULL OR l.decided_at >= $1::date)
           AND ($2::date IS NULL OR l.decided_at < $2::date + 1)
         -- по порядковому номеру: выходная колонка называется `decided_at!`
         ORDER BY 1 DESC LIMIT $3"#,
        period.from,
        period.to,
        crate::MAX_ROWS
    )
    .fetch_all(conn)
    .await?;
    crate::warn_if_capped(rows.len(), "reports::decisions");
    Ok(rows)
}

/// Строка реестра договоров (п. 126).
pub struct ContractRow {
    pub reg_number: Option<String>,
    pub registered_at: Option<OffsetDateTime>,
    pub object_name: String,
    pub tenant_name: Option<String>,
    pub monthly_rate: Decimal,
    pub lease_from: Option<OffsetDateTime>,
    pub lease_to: Option<OffsetDateTime>,
    pub status: String,
    /// `tender` | `special` | `land` - основание договора
    pub source: String,
}

/// Реестр договоров: период считается по дате регистрации (п. 126),
/// незарегистрированные договоры в реестр не попадают.
pub async fn contracts(
    conn: &mut sqlx::PgConnection,
    period: Period,
) -> Result<Vec<ContractRow>, sqlx::Error> {
    let rows = sqlx::query_as!(
        ContractRow,
        r#"SELECT c.reg_number, c.registered_at, o.name AS object_name,
                u.full_name AS "tenant_name?", c.monthly_rate,
                lower(c.lease_period) AS lease_from, upper(c.lease_period) AS lease_to,
                c.status::text AS "status!",
                CASE
                  WHEN c.tender_id IS NOT NULL THEN 'tender'
                  WHEN EXISTS (SELECT 1 FROM core.land_contracts l WHERE l.contract_id = c.id)
                    THEN 'land'
                  WHEN EXISTS (SELECT 1 FROM core.investment_contracts i WHERE i.contract_id = c.id)
                    THEN 'special'
                  ELSE 'other'
                END AS "source!"
         FROM core.contracts c
         JOIN core.objects o ON o.id = c.object_id
         LEFT JOIN core.users u ON u.id = c.tenant_id
         WHERE c.registered_at IS NOT NULL
           AND ($1::date IS NULL OR c.registered_at >= $1::date)
           AND ($2::date IS NULL OR c.registered_at < $2::date + 1)
         ORDER BY c.registered_at DESC LIMIT $3"#,
        period.from,
        period.to,
        crate::MAX_ROWS
    )
    .fetch_all(conn)
    .await?;
    crate::warn_if_capped(rows.len(), "reports::contracts");
    Ok(rows)
}

/// Строка реестра поступлений (FR-1001).
pub struct ReceiptRow {
    pub occurred_at: OffsetDateTime,
    pub account_kind: String,
    pub payer: Option<String>,
    pub amount: Decimal,
    pub rule_ref: Option<String>,
    pub recorded_by: Option<String>,
}

/// Реестр поступлений: проводки-приходы депозитной книги (credit) -
/// подтвержденные взносы и восполнения (INV-DB-05, FR-1001).
pub async fn receipts(
    conn: &mut sqlx::PgConnection,
    period: Period,
) -> Result<Vec<ReceiptRow>, sqlx::Error> {
    let rows = sqlx::query_as!(
        ReceiptRow,
        r#"SELECT e.occurred_at, a.kind::text AS "account_kind!",
                owner.full_name AS "payer?", e.credit AS amount, e.rule_ref,
                clerk.full_name AS "recorded_by?"
         FROM core.ledger_entries e
         JOIN core.ledger_accounts a ON a.id = e.account_id
         LEFT JOIN core.users owner ON owner.id = a.owner_user_id
         LEFT JOIN core.users clerk ON clerk.id = e.recorded_by
         WHERE e.credit > 0
           AND ($1::date IS NULL OR e.occurred_at >= $1::date)
           AND ($2::date IS NULL OR e.occurred_at < $2::date + 1)
         ORDER BY e.occurred_at DESC LIMIT $3"#,
        period.from,
        period.to,
        crate::MAX_ROWS
    )
    .fetch_all(conn)
    .await?;
    crate::warn_if_capped(rows.len(), "reports::receipts");
    Ok(rows)
}
