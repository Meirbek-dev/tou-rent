//! Отчетность (арх. § 9, контур 3): реестры решений, договоров и поступлений
//! с выгрузкой CSV.
//!
//! Требования в ТЗ нет (Q-012), поэтому реестр отдает то, что система уже
//! записала: колонки, строки и период - без придуманных форм, реквизитов
//! и подписей (A-079). Один контракт на все три реестра: колонки приходят
//! с сервера, кабинет рисует таблицу и ссылку на выгрузку.

use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use time::Date;
use tou_db::reports::{self, Period};
use tou_domain::policy::Action;
use tou_domain::report::{Registry, to_csv};
use utoipa::{IntoParams, ToSchema};

use crate::announcement::format_decimal_ru;
use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::request::{Json, Path, Query};
use crate::state::AppState;

/// Реестр отчетности: колонки и строки в одном порядке (арх. § 9).
#[derive(Debug, Serialize, ToSchema)]
pub struct RegistryDto {
    /// `decisions` | `contracts` | `receipts`
    pub registry: String,
    pub title_ru: String,
    pub columns: Vec<String>,
    /// Строки реестра: значения предформатированы сервером (ru, NFR-01)
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct PeriodParams {
    /// Начало периода включительно (ISO 8601)
    pub from: Option<String>,
    /// Конец периода включительно (ISO 8601)
    pub to: Option<String>,
}

impl PeriodParams {
    fn period(&self) -> Result<Period, ApiError> {
        let parse = |value: &Option<String>| -> Result<Option<Date>, ApiError> {
            match value.as_deref().filter(|raw| !raw.is_empty()) {
                None => Ok(None),
                Some(raw) => Date::parse(raw, &time::format_description::well_known::Iso8601::DATE)
                    .map(Some)
                    .map_err(|_| ApiError::Validation(format!("дата «{raw}» - ISO 8601"))),
            }
        };
        Ok(Period {
            from: parse(&self.from)?,
            to: parse(&self.to)?,
        })
    }
}

/// Реестр видят те, кто его ведет: решения - Правление и подразделение,
/// договоры - организатор и финансы, поступления - финансы.
fn require_access(user: &CurrentUser, registry: Registry) -> Result<(), ApiError> {
    match registry {
        Registry::Decisions => user
            .require(Action::BoardDecide)
            .or_else(|_| user.require(Action::TenderManage)),
        Registry::Contracts => user
            .require(Action::TenderManage)
            .or_else(|_| user.require(Action::LedgerRead)),
        Registry::Receipts => user.require(Action::LedgerRead),
    }
}

async fn build(
    state: &AppState,
    registry: Registry,
    period: Period,
) -> Result<Vec<Vec<String>>, ApiError> {
    let mut conn = state.db.acquire().await?;

    let rows = match registry {
        Registry::Decisions => reports::decisions(&mut conn, period)
            .await?
            .into_iter()
            .map(|row| {
                vec![
                    crate::admission::format_almaty(Some(row.decided_at)),
                    order_kind_ru(&row.order_kind).to_owned(),
                    row.subject,
                    row.applicant.unwrap_or_else(|| "-".to_owned()),
                    decision_ru(&row.decision).to_owned(),
                    row.rationale,
                ]
            })
            .collect(),
        Registry::Contracts => reports::contracts(&mut conn, period)
            .await?
            .into_iter()
            .map(|row| {
                vec![
                    row.reg_number.unwrap_or_else(|| "б/н".to_owned()),
                    crate::admission::format_almaty(row.registered_at),
                    row.object_name,
                    row.tenant_name.unwrap_or_else(|| "-".to_owned()),
                    format_decimal_ru(row.monthly_rate),
                    match (row.lease_from, row.lease_to) {
                        (Some(from), Some(to)) => format!(
                            "{} - {}",
                            crate::admission::format_almaty(Some(from)),
                            crate::admission::format_almaty(Some(to))
                        ),
                        _ => "-".to_owned(),
                    },
                    contract_status_ru(&row.status).to_owned(),
                    source_ru(&row.source).to_owned(),
                ]
            })
            .collect(),
        Registry::Receipts => reports::receipts(&mut conn, period)
            .await?
            .into_iter()
            .map(|row| {
                vec![
                    crate::admission::format_almaty(Some(row.occurred_at)),
                    account_kind_ru(&row.account_kind).to_owned(),
                    row.payer.unwrap_or_else(|| "-".to_owned()),
                    format_decimal_ru(row.amount),
                    row.rule_ref.unwrap_or_else(|| "-".to_owned()),
                    row.recorded_by.unwrap_or_else(|| "-".to_owned()),
                ]
            })
            .collect(),
    };

    Ok(rows)
}

fn order_kind_ru(kind: &str) -> &'static str {
    match kind {
        "special" => "особый порядок (п. 90)",
        "land" => "земельный участок (п. 106)",
        _ => "-",
    }
}

fn decision_ru(decision: &str) -> &'static str {
    match decision {
        "grant" => "предоставить",
        "refuse" => "отказать",
        "redirect" => "направить в общий порядок",
        _ => "-",
    }
}

fn contract_status_ru(status: &str) -> &'static str {
    match status {
        "draft" => "проект",
        "signing" => "на подписании",
        "active" => "действует",
        "completed" => "исполнен",
        "terminated" => "прекращен",
        "cancelled" => "отменен",
        _ => "-",
    }
}

fn source_ru(source: &str) -> &'static str {
    match source {
        "tender" => "тендер",
        "special" => "особый порядок",
        "land" => "земельный участок",
        _ => "иное",
    }
}

fn account_kind_ru(kind: &str) -> &'static str {
    match kind {
        "participant_fee" => "гарантийный взнос",
        "contract_deposit" => "депозит по договору",
        _ => "-",
    }
}

/// Реестр отчетности за период (арх. § 9): колонки и строки.
#[utoipa::path(
    get,
    path = "/api/v1/reports/{registry}",
    tag = "reports",
    params(
        ("registry" = String, Path, description = "decisions | contracts | receipts"),
        PeriodParams
    ),
    responses(
        (status = 200, description = "Реестр", body = RegistryDto),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
        (status = 422, description = "Неизвестный реестр или дата", body = crate::error::Problem),
    )
)]
pub async fn registry(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(registry): Path<String>,
    Query(params): Query<PeriodParams>,
) -> Result<Json<RegistryDto>, ApiError> {
    let registry: Registry = registry
        .parse()
        .map_err(|_| ApiError::Validation(format!("неизвестный реестр: {registry}")))?;
    require_access(&user, registry)?;

    let rows = build(&state, registry, params.period()?).await?;
    Ok(Json(RegistryDto {
        registry: registry.as_str().to_owned(),
        title_ru: registry.title_ru().to_owned(),
        columns: registry
            .columns()
            .iter()
            .map(|column| (*column).to_owned())
            .collect(),
        rows,
    }))
}

/// Выгрузка реестра (арх. § 9): CSV с шапкой из тех же колонок.
#[utoipa::path(
    get,
    path = "/api/v1/reports/{registry}/export.csv",
    tag = "reports",
    params(
        ("registry" = String, Path, description = "decisions | contracts | receipts"),
        PeriodParams
    ),
    responses(
        (status = 200, description = "Выгрузка реестра", content_type = "text/csv"),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
    )
)]
pub async fn registry_csv(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(registry): Path<String>,
    Query(params): Query<PeriodParams>,
) -> Result<Response, ApiError> {
    let registry: Registry = registry
        .parse()
        .map_err(|_| ApiError::Validation(format!("неизвестный реестр: {registry}")))?;
    require_access(&user, registry)?;

    let rows = build(&state, registry, params.period()?).await?;
    let csv = to_csv(registry, &rows);

    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", registry.file_name()),
            ),
        ],
        csv,
    )
        .into_response())
}

/// Перечень реестров (арх. § 9): кабинет показывает те, что доступны роли.
#[utoipa::path(
    get,
    path = "/api/v1/reports",
    tag = "reports",
    responses((status = 200, description = "Доступные реестры", body = [RegistrySummaryDto]))
)]
pub async fn list_registries(user: CurrentUser) -> Result<Json<Vec<RegistrySummaryDto>>, ApiError> {
    Ok(Json(
        Registry::ALL
            .into_iter()
            .filter(|registry| require_access(&user, *registry).is_ok())
            .map(|registry| RegistrySummaryDto {
                registry: registry.as_str().to_owned(),
                title_ru: registry.title_ru().to_owned(),
            })
            .collect(),
    ))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RegistrySummaryDto {
    pub registry: String,
    pub title_ru: String,
}
