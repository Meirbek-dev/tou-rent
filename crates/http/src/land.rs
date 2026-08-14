//! Земельные участки (М18: FR-1801, INV-105, п. 104–107).
//!
//! Характеристики участка публикуются на портале и открыты гостю (FR-1401),
//! заявку подает инвестор (тот же внешний заявитель, что и на тендер, -
//! A-067), решает Правление, а договор с особыми условиями ведет организатор.
//! INV-105: особые условия закрепляются при составлении договора, и без
//! полного комплекта договор не подписывается - правило стоит триггером БД.

use axum::extract::State;
use axum::http::StatusCode;
use garde::Validate as _;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tou_db::land::{self, ApplicationRecord, LandError, PlotRecord};
use tou_domain::land::{Covenant, LandDecision, LandDesignation};
use tou_domain::policy::{Action, Compound};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::request::{Json, Path};
use crate::state::AppState;
use tou_domain::rule::RuleViolation;

fn land_error(err: LandError) -> ApiError {
    match err {
        LandError::NotFound => ApiError::NotFound,
        LandError::Rejected(reason) => ApiError::RuleViolation(reason),
        LandError::Db(db) => db.into(),
    }
}

fn positive(value: &Decimal, _ctx: &()) -> garde::Result {
    if *value > Decimal::ZERO {
        Ok(())
    } else {
        Err(garde::Error::new("значение должно быть > 0"))
    }
}

/// Характеристики земельного участка (FR-1801, п. 104).
#[derive(Debug, Serialize, ToSchema)]
pub struct LandPlotDto {
    pub object_id: Uuid,
    pub name: String,
    pub address: String,
    #[schema(value_type = String, example = "1200.00")]
    pub area_m2: Decimal,
    pub cadastral_number: String,
    /// `dormitory` - под общежитие, `other` - иное назначение (п. 104)
    pub designation: String,
    pub designation_label: String,
    pub permitted_use: String,
    #[schema(value_type = Option<String>, example = "100000000.00")]
    pub min_investment: Option<Decimal>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub published_at: Option<OffsetDateTime>,
}

fn plot_dto(record: PlotRecord) -> LandPlotDto {
    LandPlotDto {
        object_id: record.object_id,
        name: record.name,
        address: record.address,
        area_m2: record.area_m2,
        cadastral_number: record.cadastral_number,
        designation: record.designation,
        designation_label: record.designation_label,
        permitted_use: record.permitted_use,
        min_investment: record.min_investment,
        published_at: record.published_at,
    }
}

/// Участки на портале (FR-1801, п. 104): опубликованные характеристики
/// открыты всем, включая гостя. Организатор видит и неопубликованные.
#[utoipa::path(
    get,
    path = "/api/v1/land-plots",
    tag = "land",
    responses((status = 200, description = "Земельные участки", body = [LandPlotDto]))
)]
pub async fn list_land_plots(
    user: Option<CurrentUser>,
    State(state): State<AppState>,
) -> Result<Json<Vec<LandPlotDto>>, ApiError> {
    let manages = user
        .as_ref()
        .is_some_and(|user| user.require(Action::LandManage).is_ok());

    let records = if manages {
        land::list_all(&state.db).await?
    } else {
        land::list_published(&state.db).await?
    };
    Ok(Json(records.into_iter().map(plot_dto).collect()))
}

#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct LandPlotRequest {
    /// Объект реестра вида `land_plot` (FR-101)
    #[garde(skip)]
    pub object_id: Uuid,
    #[garde(length(chars, min = 1, max = 100))]
    pub cadastral_number: String,
    /// Назначение из перечня п. 104: `dormitory` | `other`
    #[garde(skip)]
    pub designation: String,
    #[garde(length(chars, min = 1, max = 2000))]
    pub permitted_use: String,
    #[garde(inner(custom(positive)))]
    #[schema(value_type = Option<String>, example = "100000000.00")]
    pub min_investment: Option<Decimal>,
}

/// Характеристики участка (п. 104): заводит и уточняет организатор.
#[utoipa::path(
    post,
    path = "/api/v1/land-plots",
    tag = "land",
    request_body = LandPlotRequest,
    responses(
        (status = 201, description = "Характеристики сохранены", body = LandPlotDto),
        (status = 409, description = "Объект не является земельным участком", body = crate::error::Problem),
        (status = 422, description = "Данные не прошли проверку", body = crate::error::Problem),
    )
)]
pub async fn save_land_plot(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<LandPlotRequest>,
) -> Result<(StatusCode, Json<LandPlotDto>), ApiError> {
    user.require(Action::LandManage)?;
    body.validate()
        .map_err(|r| ApiError::Validation(r.to_string()))?;

    // Назначение - из закрытого перечня п. 104
    let designation: LandDesignation = body.designation.parse().map_err(|_| {
        ApiError::Validation(format!("неизвестное назначение: {}", body.designation))
    })?;

    let record = land::upsert_plot(
        &state.db,
        user.id(),
        land::NewPlot {
            object_id: body.object_id,
            cadastral_number: body.cadastral_number.trim(),
            designation: designation.as_str(),
            permitted_use: body.permitted_use.trim(),
            min_investment: body.min_investment,
        },
    )
    .await
    .map_err(land_error)?;

    Ok((StatusCode::CREATED, Json(plot_dto(record))))
}

/// Публикация характеристик участка (п. 104): с нее начинается прием заявок.
#[utoipa::path(
    post,
    path = "/api/v1/land-plots/{id}/publish",
    tag = "land",
    params(("id" = Uuid, Path, description = "Участок")),
    responses(
        (status = 200, description = "Участок опубликован", body = LandPlotDto),
        (status = 404, description = "Участок не найден", body = crate::error::Problem),
    )
)]
pub async fn publish_land_plot(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<LandPlotDto>, ApiError> {
    user.require(Action::LandManage)?;

    let record = land::publish_plot(&state.db, user.id(), id)
        .await
        .map_err(land_error)?;
    Ok(Json(plot_dto(record)))
}

/// Заявка инвестора на участок (FR-1801, п. 105–107).
#[derive(Debug, Serialize, ToSchema)]
pub struct LandApplicationDto {
    pub id: Uuid,
    pub plot_id: Uuid,
    pub plot_name: String,
    pub investor_name: Option<String>,
    pub project: String,
    #[schema(value_type = String, example = "100000000.00")]
    pub investment_amount: Decimal,
    pub term_months: i32,
    /// `submitted` | `granted` | `refused` | `withdrawn`
    pub status: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub submitted_at: OffsetDateTime,
    /// Решение Правления (п. 106): `grant` | `refuse`
    pub decision: Option<String>,
    pub rationale: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub decided_at: Option<OffsetDateTime>,
    /// Договор по удовлетворенной заявке (п. 107)
    pub contract_id: Option<Uuid>,
    /// Особые условия договора (INV-105): коды закрепленных условий
    pub covenants: Vec<String>,
    /// Условия, которых договору не хватает для подписания (INV-105)
    pub missing_covenants: Vec<String>,
}

fn application_dto(record: ApplicationRecord, covenants: Vec<String>) -> LandApplicationDto {
    let present: Vec<Covenant> = covenants
        .iter()
        .filter_map(|code| code.parse().ok())
        .collect();
    let missing = if record.contract_id.is_some() {
        Covenant::missing(&present)
            .into_iter()
            .map(|covenant| covenant.as_str().to_owned())
            .collect()
    } else {
        Vec::new()
    };

    LandApplicationDto {
        id: record.id,
        plot_id: record.plot_id,
        plot_name: record.plot_name,
        investor_name: record.investor_name,
        project: record.project,
        investment_amount: record.investment_amount,
        term_months: record.term_months,
        status: record.status,
        submitted_at: record.submitted_at,
        decision: record.decision,
        rationale: record.rationale,
        decided_at: record.decided_at,
        contract_id: record.contract_id,
        covenants,
        missing_covenants: missing,
    }
}

async fn with_covenants(
    state: &AppState,
    records: Vec<ApplicationRecord>,
) -> Result<Vec<LandApplicationDto>, ApiError> {
    let mut items = Vec::with_capacity(records.len());
    for record in records {
        let covenants = match record.contract_id {
            Some(contract_id) => land::covenants_of(&state.db, contract_id).await?,
            None => Vec::new(),
        };
        items.push(application_dto(record, covenants));
    }
    Ok(items)
}

#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct LandApplicationRequest {
    #[garde(skip)]
    pub plot_id: Uuid,
    /// Проект инвестора (п. 105)
    #[garde(length(chars, min = 1, max = 4000))]
    pub project: String,
    #[garde(custom(positive))]
    #[schema(value_type = String, example = "100000000.00")]
    pub investment_amount: Decimal,
    #[garde(range(min = 1, max = 600))]
    pub term_months: i32,
}

/// Подача заявки инвестором (п. 105): участок обязан быть опубликован.
#[utoipa::path(
    post,
    path = "/api/v1/land-applications",
    tag = "land",
    request_body = LandApplicationRequest,
    responses(
        (status = 201, description = "Заявка подана", body = LandApplicationDto),
        (status = 409, description = "Участок не опубликован (п. 104–105)", body = crate::error::Problem),
        (status = 422, description = "Данные не прошли проверку", body = crate::error::Problem),
    )
)]
pub async fn submit_land_application(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<LandApplicationRequest>,
) -> Result<(StatusCode, Json<LandApplicationDto>), ApiError> {
    user.require(Action::ApplicationSubmit)?;
    body.validate()
        .map_err(|r| ApiError::Validation(r.to_string()))?;

    let record = land::submit(
        &state.db,
        user.id(),
        land::NewApplication {
            plot_id: body.plot_id,
            investor_id: user.id(),
            project: body.project.trim(),
            investment_amount: body.investment_amount,
            term_months: body.term_months,
        },
    )
    .await
    .map_err(land_error)?;

    Ok((
        StatusCode::CREATED,
        Json(application_dto(record, Vec::new())),
    ))
}

/// Мои заявки на участки (кабинет инвестора, п. 105).
#[utoipa::path(
    get,
    path = "/api/v1/land-applications/my",
    tag = "land",
    responses((status = 200, description = "Заявки инвестора", body = [LandApplicationDto]))
)]
pub async fn my_land_applications(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<LandApplicationDto>>, ApiError> {
    user.require(Action::ApplicationReadOwn)?;

    let records = land::list_own(&state.db, user.id()).await?;
    Ok(Json(with_covenants(&state, records).await?))
}

/// Рабочий список заявок (п. 105–107): Правление решает, организатор ведет
/// договор.
#[utoipa::path(
    get,
    path = "/api/v1/land-applications",
    tag = "land",
    responses(
        (status = 200, description = "Заявки на участки", body = [LandApplicationDto]),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
    )
)]
pub async fn list_land_applications(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<LandApplicationDto>>, ApiError> {
    user.require_any(Compound::LAND_APPLICATION_REVIEW)?;

    let records = land::list_all_applications(&state.db).await?;
    Ok(Json(with_covenants(&state, records).await?))
}

/// Отзыв заявки инвестором, пока решение не принято (п. 105).
#[utoipa::path(
    post,
    path = "/api/v1/land-applications/{id}/withdraw",
    tag = "land",
    params(("id" = Uuid, Path, description = "Заявка на участок")),
    responses(
        (status = 200, description = "Заявка отозвана", body = LandApplicationDto),
        (status = 404, description = "Заявка не найдена или решение принято", body = crate::error::Problem),
    )
)]
pub async fn withdraw_land_application(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<LandApplicationDto>, ApiError> {
    user.require(Action::ApplicationWithdraw)?;

    let record = land::withdraw(&state.db, user.id(), id)
        .await
        .map_err(land_error)?;
    Ok(Json(application_dto(record, Vec::new())))
}

#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct LandDecisionRequest {
    /// Решение Правления: `grant` | `refuse` (п. 106)
    #[garde(skip)]
    pub decision: String,
    #[garde(length(chars, min = 1, max = 4000))]
    pub rationale: String,
}

/// Решение Правления по заявке на участок (п. 106).
#[utoipa::path(
    post,
    path = "/api/v1/land-applications/{id}/decision",
    tag = "land",
    params(("id" = Uuid, Path, description = "Заявка на участок")),
    request_body = LandDecisionRequest,
    responses(
        (status = 201, description = "Решение принято", body = LandApplicationDto),
        (status = 409, description = "Заявка уже не рассматривается (п. 105–106)", body = crate::error::Problem),
        (status = 422, description = "Данные не прошли проверку", body = crate::error::Problem),
    )
)]
pub async fn decide_land_application(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<LandDecisionRequest>,
) -> Result<(StatusCode, Json<LandApplicationDto>), ApiError> {
    user.require(Action::BoardDecide)?;
    body.validate()
        .map_err(|r| ApiError::Validation(r.to_string()))?;

    let decision: LandDecision = body
        .decision
        .parse()
        .map_err(|_| ApiError::Validation(format!("неизвестное решение: {}", body.decision)))?;

    // Порядок п. 105–106 в домене: решают по рассматриваемой заявке.
    // Тот же порядок стережет триггер БД - вставку мимо приложения тоже.
    let current = land::get_application(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let status = current
        .status
        .parse()
        .map_err(|err: tou_domain::land::UnknownLandStatus| ApiError::internal(err))?;
    decision
        .take(status)
        .map_err(|err| ApiError::rule(RuleViolation::LandApplication, err.to_string()))?;

    let record = land::decide(
        &state.db,
        user.id(),
        id,
        decision.as_str(),
        body.rationale.trim(),
    )
    .await
    .map_err(land_error)?;

    Ok((
        StatusCode::CREATED,
        Json(application_dto(record, Vec::new())),
    ))
}

#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct LandContractRequest {
    #[garde(skip)]
    pub land_application_id: Uuid,
    /// Опции коэффициентов Прил. 4: ставку считает сервер (FR-201)
    #[garde(skip)]
    #[serde(default)]
    pub rate_options: crate::rates::RateOptionsDto,
}

/// Договор на участок с особыми условиями (п. 107). Особые условия
/// закрепляются сразу и целиком (INV-105): без них договор не подписывается,
/// а снять внесенное условие нельзя.
#[utoipa::path(
    post,
    path = "/api/v1/land-contracts",
    tag = "land",
    request_body = LandContractRequest,
    responses(
        (status = 201, description = "Договор составлен", body = LandApplicationDto),
        (status = 409, description = "Заявка не удовлетворена (п. 106–107)", body = crate::error::Problem),
        (status = 422, description = "Данные не прошли проверку", body = crate::error::Problem),
    )
)]
pub async fn draft_land_contract(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<LandContractRequest>,
) -> Result<(StatusCode, Json<LandApplicationDto>), ApiError> {
    user.require(Action::LandManage)?;
    body.validate()
        .map_err(|r| ApiError::Validation(r.to_string()))?;

    let application = land::get_application(&state.db, body.land_application_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let plot = tou_db::objects::get(&state.db, application.plot_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    // Ставка - снимок расчета Прил. 4 на дату составления (FR-201, FR-202)
    let calculation =
        crate::rates::build_calculation(&state.db, plot.area_m2, &body.rate_options).await?;

    // INV-105: особые условия п. 107 закрепляются полным перечнем
    let covenants: Vec<&str> = Covenant::ALL
        .into_iter()
        .map(|covenant| covenant.as_str())
        .collect();

    land::create_contract(
        &state.db,
        user.id(),
        body.land_application_id,
        calculation.monthly.amount(),
        &covenants,
    )
    .await
    .map_err(land_error)?;

    let record = land::get_application(&state.db, body.land_application_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let covenants = match record.contract_id {
        Some(contract_id) => land::covenants_of(&state.db, contract_id).await?,
        None => Vec::new(),
    };

    Ok((
        StatusCode::CREATED,
        Json(application_dto(record, covenants)),
    ))
}

/// Позиция справочника раздела 14 (назначение либо особое условие).
#[derive(Debug, Serialize, ToSchema)]
pub struct LandRefdataDto {
    pub code: String,
    pub label_ru: String,
    pub label_kk: Option<String>,
    pub label_en: Option<String>,
    pub rule_ref: String,
}

/// Справочники раздела 14 (FR-1801): назначения участков (п. 104)
/// и особые условия договора (п. 107, INV-105).
#[utoipa::path(
    get,
    path = "/api/v1/refdata/land",
    tag = "land",
    responses((status = 200, description = "Справочники раздела 14", body = LandRefdataResponse))
)]
pub async fn land_refdata(
    State(state): State<AppState>,
) -> Result<Json<LandRefdataResponse>, ApiError> {
    let designations = land::list_designations(&state.db).await?;
    let covenants = land::list_covenants(&state.db).await?;

    let map = |record: tou_db::land::CovenantRecord| LandRefdataDto {
        code: record.code,
        label_ru: record.label_ru,
        label_kk: record.label_kk,
        label_en: record.label_en,
        rule_ref: record.rule_ref,
    };

    Ok(Json(LandRefdataResponse {
        designations: designations.into_iter().map(map).collect(),
        covenants: covenants.into_iter().map(map).collect(),
    }))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LandRefdataResponse {
    /// Назначения участков (п. 104)
    pub designations: Vec<LandRefdataDto>,
    /// Особые условия договора (п. 107, INV-105)
    pub covenants: Vec<LandRefdataDto>,
}
