//! Земельные участки (М18: FR-1801, INV-105, п. 104–107).
//!
//! Характеристики участка публикуются на портале, инвестор подает заявку,
//! Правление решает, а по удовлетворенной заявке заключается договор
//! с особыми условиями. Условия допуска (публикация участка, состояние
//! заявки, полнота особых условий) проверяет БД - здесь запись и чтение.

use rust_decimal::Decimal;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::Db;

#[derive(Debug, thiserror::Error)]
pub enum LandError {
    #[error("участок или заявка не найдены")]
    NotFound,
    /// Правило п. 104–107 (домен) либо отказ БД (FR-1801, INV-105)
    #[error("{0}")]
    Rejected(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

fn map_rule(err: sqlx::Error) -> LandError {
    if let sqlx::Error::Database(db_err) = &err
        && matches!(
            db_err.code().as_deref(),
            Some("P0001") | Some("23514") | Some("23503") | Some("23505")
        )
    {
        return LandError::Rejected(db_err.message().to_owned());
    }
    LandError::Db(err)
}

/// Участок с характеристиками раздела 14 (п. 104).
pub struct PlotRecord {
    pub object_id: Uuid,
    pub name: String,
    pub address: String,
    pub area_m2: Decimal,
    pub cadastral_number: String,
    pub designation: String,
    pub designation_label: String,
    pub permitted_use: String,
    pub min_investment: Option<Decimal>,
    pub published_at: Option<OffsetDateTime>,
}

/// Выборка участка: общий список столбцов + хвост запроса (см. `acts.rs`).
macro_rules! plot_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            PlotRecord,
            "SELECT p.object_id, o.name, o.address, o.area_m2,
                    p.cadastral_number, p.designation, d.label_ru AS designation_label,
                    p.permitted_use, p.min_investment, p.published_at
             FROM core.land_plots p
             JOIN core.objects o ON o.id = p.object_id
             JOIN refdata.land_designations d ON d.code = p.designation" + $tail
            $(, $arg)*
        )
    };
}

/// Участки портала (FR-1801, п. 104): только опубликованные.
pub async fn list_published(db: &Db) -> Result<Vec<PlotRecord>, sqlx::Error> {
    let rows = plot_query!(
        " WHERE p.published_at IS NOT NULL ORDER BY p.published_at DESC LIMIT $1",
        crate::MAX_ROWS
    )
    .fetch_all(db)
    .await?;
    crate::warn_if_capped(rows.len(), "land::list_published");
    Ok(rows)
}

/// Все участки: рабочий список организатора, включая неопубликованные.
pub async fn list_all(db: &Db) -> Result<Vec<PlotRecord>, sqlx::Error> {
    let rows = plot_query!(" ORDER BY o.name LIMIT $1", crate::MAX_ROWS)
        .fetch_all(db)
        .await?;
    crate::warn_if_capped(rows.len(), "land::list_all");
    Ok(rows)
}

pub async fn get_plot(db: &Db, object_id: Uuid) -> Result<Option<PlotRecord>, sqlx::Error> {
    plot_query!(" WHERE p.object_id = $1", object_id)
        .fetch_optional(db)
        .await
}

pub struct NewPlot<'a> {
    pub object_id: Uuid,
    pub cadastral_number: &'a str,
    pub designation: &'a str,
    pub permitted_use: &'a str,
    pub min_investment: Option<Decimal>,
}

/// Характеристики участка (п. 104): заводятся один раз и уточняются до
/// публикации - вид объекта проверяет триггер БД.
pub async fn upsert_plot(db: &Db, actor: Uuid, new: NewPlot<'_>) -> Result<PlotRecord, LandError> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query!(
            "INSERT INTO core.land_plots
               (object_id, cadastral_number, designation, permitted_use, min_investment)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (object_id) DO UPDATE
               SET cadastral_number = EXCLUDED.cadastral_number,
                   designation      = EXCLUDED.designation,
                   permitted_use    = EXCLUDED.permitted_use,
                   min_investment   = EXCLUDED.min_investment",
            new.object_id,
            new.cadastral_number,
            new.designation,
            new.permitted_use,
            new.min_investment
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;

        let record = plot_query!(" WHERE p.object_id = $1", new.object_id)
            .fetch_one(&mut *tx)
            .await?;
        Ok(record)
    })
    .await
}

/// Публикация характеристик участка (п. 104): с нее начинается прием заявок.
pub async fn publish_plot(db: &Db, actor: Uuid, object_id: Uuid) -> Result<PlotRecord, LandError> {
    crate::with_actor(db, actor, async |tx| {
        let updated = sqlx::query_scalar!(
            "UPDATE core.land_plots SET published_at = coalesce(published_at, core.now())
             WHERE object_id = $1 RETURNING object_id",
            object_id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_rule)?;
        updated.ok_or(LandError::NotFound)?;

        let record = plot_query!(" WHERE p.object_id = $1", object_id)
            .fetch_one(&mut *tx)
            .await?;
        Ok(record)
    })
    .await
}

/// Заявка инвестора на участок (п. 105).
pub struct ApplicationRecord {
    pub id: Uuid,
    pub plot_id: Uuid,
    pub plot_name: String,
    pub investor_id: Uuid,
    pub investor_name: Option<String>,
    pub project: String,
    pub investment_amount: Decimal,
    pub term_months: i32,
    pub status: String,
    pub submitted_at: OffsetDateTime,
    pub withdrawn_at: Option<OffsetDateTime>,
    /// Решение Правления (п. 106), если принято
    pub decision: Option<String>,
    pub rationale: Option<String>,
    pub decided_at: Option<OffsetDateTime>,
    /// Договор по удовлетворенной заявке (п. 107)
    pub contract_id: Option<Uuid>,
}

/// Выборка заявки: общий список столбцов + хвост запроса (см. `acts.rs`).
///
/// `!` у `status` - это `::text`, который планировщик считает потенциально
/// NULL, хотя столбец NOT NULL. У `decision` того же `!` нет: решения может
/// и не быть - оно приходит из LEFT JOIN. Остальные столбцы из LEFT JOIN
/// получают `?`: sqlx выводит nullability по самому столбцу (все они
/// NOT NULL), а не по виду соединения.
macro_rules! application_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            ApplicationRecord,
            r#"SELECT a.id, a.plot_id, o.name AS plot_name, a.investor_id,
                      u.full_name AS "investor_name?", a.project, a.investment_amount,
                      a.term_months, a.status::text AS "status!", a.submitted_at,
                      a.withdrawn_at, d.decision::text AS decision,
                      d.rationale AS "rationale?",
                      d.decided_at AS "decided_at?", l.contract_id AS "contract_id?"
               FROM core.land_applications a
               JOIN core.objects o ON o.id = a.plot_id
               LEFT JOIN core.users u ON u.id = a.investor_id
               LEFT JOIN core.land_decisions d ON d.land_application_id = a.id
               LEFT JOIN core.land_contracts l ON l.land_application_id = a.id"# + $tail
            $(, $arg)*
        )
    };
}

pub struct NewApplication<'a> {
    pub plot_id: Uuid,
    pub investor_id: Uuid,
    pub project: &'a str,
    pub investment_amount: Decimal,
    pub term_months: i32,
}

/// Подача заявки инвестором (п. 105): участок обязан быть опубликован -
/// это проверяет триггер БД.
pub async fn submit(
    db: &Db,
    actor: Uuid,
    new: NewApplication<'_>,
) -> Result<ApplicationRecord, LandError> {
    crate::with_actor(db, actor, async |tx| {
        let id = sqlx::query_scalar!(
            "INSERT INTO core.land_applications
               (plot_id, investor_id, project, investment_amount, term_months)
             VALUES ($1, $2, $3, $4, $5) RETURNING id",
            new.plot_id,
            new.investor_id,
            new.project,
            new.investment_amount,
            new.term_months
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_rule)?;

        let record = application_query!(" WHERE a.id = $1", id)
            .fetch_one(&mut *tx)
            .await?;
        Ok(record)
    })
    .await
}

/// Заявки инвестора (кабинет).
pub async fn list_own(db: &Db, investor: Uuid) -> Result<Vec<ApplicationRecord>, sqlx::Error> {
    let rows = application_query!(
        " WHERE a.investor_id = $1 ORDER BY a.submitted_at DESC LIMIT $2",
        investor,
        crate::MAX_ROWS
    )
    .fetch_all(db)
    .await?;
    crate::warn_if_capped(rows.len(), "land::list_own");
    Ok(rows)
}

/// Рабочий список Правления и организатора (п. 105–107).
pub async fn list_all_applications(db: &Db) -> Result<Vec<ApplicationRecord>, sqlx::Error> {
    let rows = application_query!(" ORDER BY a.submitted_at LIMIT $1", crate::MAX_ROWS)
        .fetch_all(db)
        .await?;
    crate::warn_if_capped(rows.len(), "land::list_all_applications");
    Ok(rows)
}

pub async fn get_application(db: &Db, id: Uuid) -> Result<Option<ApplicationRecord>, sqlx::Error> {
    application_query!(" WHERE a.id = $1", id)
        .fetch_optional(db)
        .await
}

/// Отзыв заявки инвестором, пока решение не принято (п. 105).
pub async fn withdraw(db: &Db, actor: Uuid, id: Uuid) -> Result<ApplicationRecord, LandError> {
    crate::with_actor(db, actor, async |tx| {
        let updated = sqlx::query_scalar!(
            "UPDATE core.land_applications SET status = 'withdrawn'
             WHERE id = $1 AND investor_id = $2 AND status = 'submitted'
             RETURNING id",
            id,
            actor
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_rule)?;
        updated.ok_or(LandError::NotFound)?;

        let record = application_query!(" WHERE a.id = $1", id)
            .fetch_one(&mut *tx)
            .await?;
        Ok(record)
    })
    .await
}

/// Решение Правления (п. 106): следствие для заявки применяет триггер БД.
pub async fn decide(
    db: &Db,
    actor: Uuid,
    application_id: Uuid,
    decision: &str,
    rationale: &str,
) -> Result<ApplicationRecord, LandError> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query!(
            "INSERT INTO core.land_decisions
               (land_application_id, decision, rationale, decided_by)
             VALUES ($1, $2::text::core.land_decision, $3, $4)",
            application_id,
            decision,
            rationale,
            actor
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;

        let record = application_query!(" WHERE a.id = $1", application_id)
            .fetch_one(&mut *tx)
            .await?;
        Ok(record)
    })
    .await
}

/// Договор на участок с особыми условиями (п. 107). Условия закрепляются
/// сразу: их перечень закрыт (INV-105), а подписание без полного комплекта
/// отклонит триггер БД.
pub async fn create_contract(
    db: &Db,
    actor: Uuid,
    application_id: Uuid,
    monthly_rate: Decimal,
    covenants: &[&str],
) -> Result<Uuid, LandError> {
    crate::with_actor(db, actor, async |tx| {
        let application = sqlx::query!(
            "SELECT plot_id, investor_id, investment_amount
             FROM core.land_applications WHERE id = $1",
            application_id
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(LandError::NotFound)?;

        let contract_id = sqlx::query_scalar!(
            "INSERT INTO core.contracts (object_id, tenant_id, monthly_rate)
             VALUES ($1, $2, $3) RETURNING id",
            application.plot_id,
            application.investor_id,
            monthly_rate
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_rule)?;

        sqlx::query!(
            "INSERT INTO core.land_contracts
               (contract_id, land_application_id, investment_amount)
             VALUES ($1, $2, $3)",
            contract_id,
            application_id,
            application.investment_amount
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;

        for code in covenants {
            sqlx::query!(
                "INSERT INTO core.land_contract_covenants (contract_id, code)
                 VALUES ($1, $2) ON CONFLICT DO NOTHING",
                contract_id,
                *code
            )
            .execute(&mut *tx)
            .await
            .map_err(map_rule)?;
        }

        Ok(contract_id)
    })
    .await
}

/// Особые условия договора (INV-105): коды закрепленных условий.
pub async fn covenants_of(db: &Db, contract_id: Uuid) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT k.code FROM core.land_contract_covenants k
         JOIN refdata.land_covenants c ON c.code = k.code
         WHERE k.contract_id = $1 ORDER BY c.ordinal",
        contract_id
    )
    .fetch_all(db)
    .await
}

/// Справочник особых условий (п. 107) - паритет с доменом проверяет тест.
pub struct CovenantRecord {
    pub code: String,
    pub label_ru: String,
    pub label_kk: Option<String>,
    pub label_en: Option<String>,
    pub rule_ref: String,
}

pub async fn list_covenants(db: &Db) -> Result<Vec<CovenantRecord>, sqlx::Error> {
    sqlx::query_as!(
        CovenantRecord,
        "SELECT code, label_ru, label_kk, label_en, rule_ref
         FROM refdata.land_covenants ORDER BY ordinal"
    )
    .fetch_all(db)
    .await
}

/// Назначения участков (п. 104).
pub async fn list_designations(db: &Db) -> Result<Vec<CovenantRecord>, sqlx::Error> {
    sqlx::query_as!(
        CovenantRecord,
        "SELECT code, label_ru, label_kk, label_en, rule_ref
         FROM refdata.land_designations ORDER BY ordinal"
    )
    .fetch_all(db)
    .await
}
