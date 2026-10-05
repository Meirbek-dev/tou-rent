//! Подтвержденная администратором корректировка несостоявшегося тендера.
//!
//! Маршрут намеренно не является универсальным редактором статуса: он
//! принимает подписанный документ комиссии и победителя/цену каждого лота,
//! а БД повторно проверяет весь снимок и выполняет один аудируемый UPDATE.

use axum::extract::State;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tou_domain::{policy::Action, rule::RuleViolation};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    admin_data::require_purge_enabled,
    error::ApiError,
    extract::CurrentUser,
    request::{Json, Path},
    state::AppState,
};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdminOutcomeApplicationDto {
    pub id: Uuid,
    pub applicant: String,
    pub status: String,
    pub price: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdminOutcomeLotDto {
    pub id: Uuid,
    pub seq: i32,
    pub purpose: String,
    pub applications: Vec<AdminOutcomeApplicationDto>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdminOutcomeProtocolDto {
    pub id: Uuid,
    pub title: String,
    pub number: String,
    pub document_date: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdminSuccessfulOutcomeStateDto {
    pub tender_id: Uuid,
    pub title: String,
    pub status: String,
    pub eligible: bool,
    pub protocols: Vec<AdminOutcomeProtocolDto>,
    pub lots: Vec<AdminOutcomeLotDto>,
    pub recorded_protocol_id: Option<Uuid>,
    pub recorded_lots: Option<serde_json::Value>,
    pub recorded_reason: Option<String>,
    pub recorded_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdminOutcomeLotDecisionDto {
    pub lot_id: Uuid,
    pub application_id: Uuid,
    /// Итоговая цена в тенге, строкой без потери точности.
    pub price: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RecordAdminSuccessfulOutcomeRequest {
    pub protocol_id: Uuid,
    pub reason: String,
    pub confirmed_signed_protocol: bool,
    pub lots: Vec<AdminOutcomeLotDecisionDto>,
}

async fn load_state(
    app: &AppState,
    actor: Uuid,
    id: Uuid,
) -> Result<AdminSuccessfulOutcomeStateDto, ApiError> {
    let value = tou_db::admin_outcomes::state(&app.db, actor, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    serde_json::from_value(value).map_err(ApiError::internal)
}

#[utoipa::path(
    get,
    path="/api/v1/admin/tenders/{id}/successful-outcome",
    operation_id="admin_successful_outcome_state",
    tag="admin",
    params(("id"=Uuid,Path,description="Несостоявшийся тендер")),
    responses(
        (status=200,body=AdminSuccessfulOutcomeStateDto),
        (status=403,body=crate::error::Problem),
        (status=404,body=crate::error::Problem)
    )
)]
pub async fn state(
    user: CurrentUser,
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<AdminSuccessfulOutcomeStateDto>, ApiError> {
    user.require(Action::DataPurge)?;
    Ok(Json(load_state(&app, user.id(), id).await?))
}

#[utoipa::path(
    post,
    path="/api/v1/admin/tenders/{id}/successful-outcome",
    operation_id="record_admin_successful_outcome",
    tag="admin",
    params(("id"=Uuid,Path,description="Несостоявшийся тендер")),
    request_body=RecordAdminSuccessfulOutcomeRequest,
    responses(
        (status=200,body=AdminSuccessfulOutcomeStateDto),
        (status=403,body=crate::error::Problem),
        (status=404,body=crate::error::Problem),
        (status=409,body=crate::error::Problem),
        (status=422,body=crate::error::Problem)
    )
)]
pub async fn record(
    user: CurrentUser,
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<RecordAdminSuccessfulOutcomeRequest>,
) -> Result<Json<AdminSuccessfulOutcomeStateDto>, ApiError> {
    user.require(Action::DataPurge)?;
    require_purge_enabled(&app)?;

    let reason = body.reason.trim();
    let prices_valid = body.lots.iter().all(|lot| {
        Decimal::from_str_exact(lot.price.trim()).is_ok_and(|price| price > Decimal::ZERO)
    });
    if !body.confirmed_signed_protocol
        || reason.is_empty()
        || reason.chars().count() > 2000
        || body.lots.is_empty()
        || body.lots.len() > tou_db::MAX_ROWS as usize
        || !prices_valid
    {
        return Err(ApiError::rule(
            RuleViolation::TenderFailureGround,
            "подтвердите подписанный протокол, основание и положительные цены по всем лотам",
        ));
    }

    let lots = serde_json::to_value(&body.lots).map_err(ApiError::internal)?;
    let updated =
        tou_db::admin_outcomes::record(&app.db, user.id(), id, body.protocol_id, reason, &lots)
            .await
            .map_err(|err| match err {
                tou_db::failure::FailureError::NotFound => ApiError::NotFound,
                tou_db::failure::FailureError::Rejected(reason) => ApiError::RuleViolation(reason),
                tou_db::failure::FailureError::Db(err) => err.into(),
            })?;
    if !updated {
        return Err(ApiError::rule(
            RuleViolation::TenderStatusTransition,
            "тендер уже исправлен либо больше не находится в статусе failed",
        ));
    }

    tracing::warn!(
        actor=%user.id(),
        tender_id=%id,
        protocol_id=%body.protocol_id,
        "несостоявшийся тендер исправлен как состоявшийся по подписанному протоколу"
    );
    Ok(Json(load_state(&app, user.id(), id).await?))
}

#[cfg(test)]
mod tests {
    #[test]
    fn admin_outcome_routes_are_registered() {
        let json = crate::openapi().to_json().expect("serialize OpenAPI");
        assert!(json.contains("/api/v1/admin/tenders/{id}/successful-outcome"));
        assert!(json.contains("RecordAdminSuccessfulOutcomeRequest"));
        assert!(json.contains("AdminSuccessfulOutcomeStateDto"));
    }
}
