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
use tou_db::RowCursor;
use tou_db::reports::{self, Period, RegistryRow};
use tou_domain::policy::{Action, Compound};
use tou_domain::report::{Registry, to_csv};
use utoipa::{IntoParams, ToSchema};

use crate::announcement::format_decimal_ru;
use crate::dto::cursor;
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
    /// Курсор продолжения; `null` - реестр показан до конца
    pub next_after: Option<String>,
    /// Показана не вся выборка: за строками ответа есть еще
    pub truncated: bool,
}

fn parse_period(from: Option<&str>, to: Option<&str>) -> Result<Period, ApiError> {
    let parse = |value: Option<&str>| -> Result<Option<Date>, ApiError> {
        match value.filter(|raw| !raw.is_empty()) {
            None => Ok(None),
            Some(raw) => Date::parse(raw, &time::format_description::well_known::Iso8601::DATE)
                .map(Some)
                .map_err(|_| ApiError::Validation(format!("дата «{raw}» - ISO 8601"))),
        }
    };
    Ok(Period {
        from: parse(from)?,
        to: parse(to)?,
    })
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
        parse_period(self.from.as_deref(), self.to.as_deref())
    }
}

/// Параметры экрана реестра: период плюс страница (ТЗ § 7).
#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct RegistryParams {
    /// Начало периода включительно (ISO 8601)
    pub from: Option<String>,
    /// Конец периода включительно (ISO 8601)
    pub to: Option<String>,
    /// Курсор следующей страницы - значение `next_after` предыдущей
    pub after: Option<String>,
    pub limit: Option<i64>,
}

impl RegistryParams {
    fn period(&self) -> Result<Period, ApiError> {
        parse_period(self.from.as_deref(), self.to.as_deref())
    }
}

/// Реестр видят те, кто его ведет: решения - Правление и подразделение,
/// договоры - организатор и финансы, поступления - финансы.
///
/// Реестр не отдельная область ответственности, а срез трех уже разведенных:
/// поэтому права на него нет, а есть право на саму область (INV-POL-01).
fn require_access(user: &CurrentUser, registry: Registry) -> Result<(), ApiError> {
    match registry {
        Registry::Decisions => user.require_any(Compound::SPECIAL_DECISION_ACCESS),
        Registry::Contracts => user.require_any(Compound::CONTRACT_REGISTRY_READ),
        Registry::Receipts => user.require(Action::LedgerRead),
    }
}

/// Страница реестра до превращения в DTO: строки уже отформатированы,
/// а курсор еще жив - после `map` в `Vec<String>` его взять неоткуда.
struct RegistryPage {
    rows: Vec<Vec<String>>,
    next: Option<RowCursor>,
    truncated: bool,
}

/// Курсор последней строки страницы и признак усечения - одинаково для
/// всех трех реестров, у которых строки разного типа.
fn page_tail<T: RegistryRow>(page: &tou_db::Page<T>) -> (Option<RowCursor>, bool) {
    (page.last().map(RegistryRow::cursor), page.truncated)
}

async fn build(
    state: &AppState,
    registry: Registry,
    period: Period,
    after: Option<RowCursor>,
    limit: i64,
) -> Result<RegistryPage, ApiError> {
    let mut conn = state.db.acquire().await?;

    let (rows, next, truncated) = match registry {
        Registry::Decisions => {
            let page = reports::decisions_page(&mut conn, period, after, limit).await?;
            let (next, truncated) = page_tail(&page);
            let rows = page
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
                .collect();
            (rows, next, truncated)
        }
        Registry::Contracts => {
            let page = reports::contracts_page(&mut conn, period, after, limit).await?;
            let (next, truncated) = page_tail(&page);
            let rows = page
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
                .collect();
            (rows, next, truncated)
        }
        Registry::Receipts => {
            let page = reports::receipts_page(&mut conn, period, after, limit).await?;
            let (next, truncated) = page_tail(&page);
            let rows = page
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
                .collect();
            (rows, next, truncated)
        }
    };

    Ok(RegistryPage {
        rows,
        next,
        truncated,
    })
}

/// Весь реестр за период - страница за страницей до конца выборки.
///
/// Выгрузка - это документ, а не экран: обрезанный на тысяче строк CSV и
/// есть та ошибка, ради которой признак усечения появился, - в нем нехватку
/// записей заметят последней. Цикл конечен по построению: ключ курсора
/// строго убывает, поэтому каждый следующий запрос берет строки, которых
/// в предыдущих не было, и упирается в конец выборки.
async fn build_all(
    state: &AppState,
    registry: Registry,
    period: Period,
) -> Result<Vec<Vec<String>>, ApiError> {
    let mut rows = Vec::new();
    let mut after = None;

    loop {
        let page = build(state, registry, period, after, tou_db::MAX_ROWS).await?;
        rows.extend(page.rows);
        match (page.truncated, page.next) {
            (true, Some(cursor)) => after = Some(cursor),
            _ => return Ok(rows),
        }
    }
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
        RegistryParams
    ),
    responses(
        (status = 200, description = "Страница реестра", body = RegistryDto),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
        (status = 422, description = "Неизвестный реестр, дата или курсор",
         body = crate::error::Problem),
    )
)]
pub async fn registry(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(registry): Path<String>,
    Query(params): Query<RegistryParams>,
) -> Result<Json<RegistryDto>, ApiError> {
    let registry: Registry = registry
        .parse()
        .map_err(|_| ApiError::Validation(format!("неизвестный реестр: {registry}")))?;
    require_access(&user, registry)?;

    let after = params.after.as_deref().map(cursor::parse).transpose()?;
    let page = build(
        &state,
        registry,
        params.period()?,
        after,
        crate::page_limit(params.limit),
    )
    .await?;

    Ok(Json(RegistryDto {
        registry: registry.as_str().to_owned(),
        title_ru: registry.title_ru().to_owned(),
        columns: registry
            .columns()
            .iter()
            .map(|column| (*column).to_owned())
            .collect(),
        rows: page.rows,
        next_after: cursor::next(page.truncated, page.next),
        truncated: page.truncated,
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

    let rows = build_all(&state, registry, params.period()?).await?;
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
