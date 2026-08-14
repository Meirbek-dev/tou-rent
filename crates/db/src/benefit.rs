//! Льготные схемы договоров особого порядка (М12, FR-1205, п. 95–96).
//!
//! Параметры схем - данные справочника (доля второго года, требование
//! согласования Ученого совета, минимум кредитов, квота стажировок), а их
//! соблюдение стережет триггер (INV-095, INV-096). Расписание платы считает
//! домен; та же формула есть в БД функцией `core.benefit_monthly` -
//! паритет проверяет тест.

use rust_decimal::Decimal;
use time::{Date, OffsetDateTime};
use tou_domain::rule::RuleRejection;
use uuid::Uuid;

use crate::Db;

#[derive(Debug, thiserror::Error)]
pub enum BenefitError {
    #[error("договор не найден")]
    NotFound,
    /// Отказ правила п. 95–96 (триггер условий льготы)
    #[error("{0}")]
    Rejected(RuleRejection),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

fn map_rule(err: sqlx::Error) -> BenefitError {
    if let sqlx::Error::Database(db_err) = &err
        && matches!(
            db_err.code().as_deref(),
            Some("P0001") | Some("23514") | Some("23503") | Some("23505")
        )
    {
        return BenefitError::Rejected(crate::rule::rejection(db_err.as_ref()));
    }
    BenefitError::Db(err)
}

/// Льготная схема со своими параметрами (FR-1205)
pub struct SchemeRecord {
    pub code: String,
    pub label_ru: String,
    pub label_kk: Option<String>,
    pub label_en: Option<String>,
    pub rule_ref: String,
    pub has_schedule: bool,
    pub later_share_pct: Decimal,
    pub requires_council: bool,
    pub min_study_credits: i32,
    pub internship_quota: i32,
}

/// Каталог льготных схем (FR-1205, п. 95–96, Прил. 4).
pub async fn list_schemes(db: &Db) -> Result<Vec<SchemeRecord>, sqlx::Error> {
    sqlx::query_as!(
        SchemeRecord,
        "SELECT code, label_ru, label_kk, label_en, rule_ref, has_schedule,
                later_share_pct, requires_council, min_study_credits, internship_quota
         FROM refdata.benefit_schemes ORDER BY code"
    )
    .fetch_all(db)
    .await
}

pub struct GrantRecord {
    pub id: Uuid,
    pub contract_id: Uuid,
    pub scheme: String,
    pub communal_monthly: Decimal,
    pub council_decision: Option<String>,
    pub council_date: Option<Date>,
    pub study_credits: i32,
    pub internships: i32,
    pub granted_by_name: Option<String>,
    pub granted_at: OffsetDateTime,
    /// Ставка договора (Прил. 4) - база расписания
    pub base_monthly: Decimal,
    /// Квота стажировок схемы на момент чтения (п. 95)
    pub internship_quota: i32,
}

/// Выборка льготы: общий список столбцов + хвост запроса (см. `acts.rs`).
///
/// `granted_by_name` получает `?`: имя приходит `LEFT JOIN`'ом, а
/// `core.users.full_name` - NOT NULL, и без аннотации sqlx вывел бы non-null.
macro_rules! grant_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            GrantRecord,
            r#"SELECT g.id, g.contract_id, g.scheme, g.communal_monthly,
                    g.council_decision, g.council_date, g.study_credits, g.internships,
                    u.full_name AS "granted_by_name?", g.granted_at,
                    c.monthly_rate AS base_monthly, s.internship_quota
             FROM core.benefit_grants g
             JOIN core.contracts c ON c.id = g.contract_id
             JOIN refdata.benefit_schemes s ON s.code = g.scheme
             LEFT JOIN core.users u ON u.id = g.granted_by"# + $tail
            $(, $arg)*
        )
    };
}

pub async fn of_contract(db: &Db, contract_id: Uuid) -> Result<Option<GrantRecord>, sqlx::Error> {
    grant_query!(" WHERE g.contract_id = $1", contract_id)
        .fetch_optional(db)
        .await
}

pub struct NewGrant<'a> {
    pub contract_id: Uuid,
    pub scheme: &'a str,
    pub communal_monthly: Decimal,
    pub council_decision: Option<&'a str>,
    pub council_date: Option<Date>,
    pub study_credits: i32,
    pub internships: i32,
}

/// Применение льготы к договору (п. 95–96). Повторное применение обновляет
/// условия - схема у договора одна.
pub async fn grant(db: &Db, actor: Uuid, new: NewGrant<'_>) -> Result<GrantRecord, BenefitError> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query!(
            "INSERT INTO core.benefit_grants
               (contract_id, scheme, communal_monthly, council_decision, council_date,
                study_credits, internships, granted_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (contract_id) DO UPDATE
               SET scheme = EXCLUDED.scheme,
                   communal_monthly = EXCLUDED.communal_monthly,
                   council_decision = EXCLUDED.council_decision,
                   council_date = EXCLUDED.council_date,
                   study_credits = EXCLUDED.study_credits,
                   internships = EXCLUDED.internships,
                   granted_by = EXCLUDED.granted_by",
            new.contract_id,
            new.scheme,
            new.communal_monthly,
            new.council_decision,
            new.council_date,
            new.study_credits,
            new.internships,
            actor
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;

        let record = grant_query!(" WHERE g.contract_id = $1", new.contract_id)
            .fetch_one(&mut *tx)
            .await?;
        Ok(record)
    })
    .await
}

/// Плата за год найма глазами БД (п. 95–96) - вторая половина паритета
/// с `domain::benefit`; ею же считает отчетность.
pub async fn monthly_for_year(
    db: &Db,
    contract_id: Uuid,
    year: i32,
) -> Result<Option<Decimal>, sqlx::Error> {
    sqlx::query_scalar!("SELECT core.benefit_monthly($1, $2)", contract_id, year)
        .fetch_one(db)
        .await
}
