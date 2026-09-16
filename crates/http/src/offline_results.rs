//! Explicit import of a signed offline decision; no fabricated electronic actions.
use axum::extract::State;
use serde::{Deserialize, Serialize};
use tou_domain::{policy::Action, rule::RuleViolation};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    error::ApiError,
    extract::CurrentUser,
    request::{Json, Path},
    state::AppState,
};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OfflineResolution {
    NoApplications,
    Rejected,
    SingleSource,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OfflineLotDecision {
    pub lot_id: Uuid,
    pub application_id: Option<Uuid>,
    pub resolution: OfflineResolution,
    /// Excerpt from the signed protocol; for rejection, include its reason.
    pub note: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RecordOfflineResults {
    pub protocol_id: Uuid,
    pub confirmed_signed_protocol: bool,
    pub lots: Vec<OfflineLotDecision>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OfflineLotResult {
    pub lot_id: Uuid,
    pub seq: i32,
    pub application_id: Option<Uuid>,
    pub applicant: Option<String>,
    pub price: Option<String>,
    pub ground: Option<String>,
    pub resolution: Option<OfflineResolution>,
    pub note: String,
    pub application_status: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OfflineResultsState {
    pub recorded: bool,
    pub eligible: bool,
    /// Corrects an earlier standard failure without deleting its protocol.
    pub correcting: bool,
    /// The generated failure protocol must first be hidden from participant cabinets.
    pub correction_requires_hidden_protocol: bool,
    pub protocol_id: Option<Uuid>,
    pub superseded_protocol_id: Option<Uuid>,
    pub recorded_by: Option<Uuid>,
    pub recorded_at: Option<String>,
    pub lots: Vec<OfflineLotResult>,
}

#[utoipa::path(get, path="/api/v1/tenders/{id}/offline-results", tag="failure",
    params(("id"=Uuid,Path)), responses((status=200,body=OfflineResultsState),(status=404,body=crate::error::Problem)))]
pub async fn state(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<OfflineResultsState>, ApiError> {
    user.require(Action::ApplicationReadAll)?;
    let data = tou_db::offline_results::state(&state.db, user.id(), id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(
        serde_json::from_value(data).map_err(ApiError::internal)?,
    ))
}

#[utoipa::path(post, path="/api/v1/tenders/{id}/offline-results", tag="failure",
    params(("id"=Uuid,Path)), request_body=RecordOfflineResults,
    responses((status=200,body=OfflineResultsState),(status=409,body=crate::error::Problem)))]
pub async fn record(
    user: CurrentUser,
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<RecordOfflineResults>,
) -> Result<Json<OfflineResultsState>, ApiError> {
    user.require(Action::AdmissionDecide)?;
    user.require(Action::ApplicationReadAll)?;
    if !body.confirmed_signed_protocol
        || body.lots.is_empty()
        || body.lots.len() > tou_db::MAX_ROWS as usize
    {
        return Err(ApiError::rule(
            RuleViolation::TenderFailureGround,
            "подтвердите подписанный протокол и решения по всем лотам",
        ));
    }
    let lots = serde_json::to_value(&body.lots).map_err(ApiError::internal)?;
    tou_db::offline_results::record(&app.db, user.id(), id, body.protocol_id, &lots)
        .await
        .map_err(|err| match err {
            tou_db::failure::FailureError::NotFound => ApiError::NotFound,
            tou_db::failure::FailureError::Rejected(reason) => ApiError::RuleViolation(reason),
            tou_db::failure::FailureError::Db(err) => err.into(),
        })?;
    state(user, State(app), Path(id)).await
}
