//! Изменение документации и отмена тендера (М3, FR-304, FR-305, FR-1004).
//!
//! Новая редакция - публикация, а не правка формы: срок приема продлевается
//! (п. 27), печатная форма объявления сохраняется снимком, участники
//! извещаются и вправе отказаться с возвратом взноса (п. 26.5). Отмена
//! возможна до заключения договора, только с основанием, и тоже извещает
//! участников (п. 78–79).

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use object_store::ObjectStoreExt as _;
use object_store::PutPayload;
use object_store::path::Path as ObjectPath;
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;
use tou_db::amendments::{self, AmendmentError};
use tou_db::tenders;
use tou_domain::amendment::CancellationScope;
use tou_domain::notification::NotificationKind;
use tou_domain::obligation::ObligationAction;
use tou_domain::policy::Action;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::announcement::render_announcement;
use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::request::{Json, Path};
use crate::state::AppState;

fn amendment_error(err: AmendmentError) -> ApiError {
    match err {
        AmendmentError::NotFound => ApiError::NotFound,
        AmendmentError::Rejected(reason) => ApiError::RuleViolation(reason),
        AmendmentError::Db(db) => db.into(),
    }
}

/// Редакция тендерной документации (FR-304): что изменено и как сдвинут срок.
#[derive(Debug, Serialize, ToSchema)]
pub struct AmendmentDto {
    pub id: Uuid,
    pub tender_id: Uuid,
    /// Номер редакции документации
    pub version: i32,
    pub summary: String,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub previous_deadline: Option<OffsetDateTime>,
    /// Новый срок приема заявок: не менее 10 календарных дней от публикации
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub new_deadline: OffsetDateTime,
    pub has_doc: bool,
    pub created_by_name: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}

fn amendment_dto(record: amendments::AmendmentRecord) -> AmendmentDto {
    AmendmentDto {
        id: record.id,
        tender_id: record.tender_id,
        version: record.version,
        summary: record.summary,
        previous_deadline: record.previous_deadline,
        new_deadline: record.new_deadline,
        has_doc: record.doc_key.is_some(),
        created_by_name: record.created_by_name,
        created_at: record.created_at,
    }
}

/// Редакции документации тендера (FR-304): баннер изменений участника.
#[utoipa::path(
    get,
    path = "/api/v1/tenders/{id}/amendments",
    tag = "amendments",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses((status = 200, description = "Редакции документации", body = [AmendmentDto]))
)]
pub async fn tender_amendments(
    user: Option<CurrentUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<AmendmentDto>>, ApiError> {
    // Изменения объявленного тендера - публичная информация (п. 27, FR-1401)
    if let Some(user) = user.as_ref() {
        user.require(Action::TenderRead)?;
    }

    let records = amendments::list_for_tender(&state.db, id).await?;
    Ok(Json(records.into_iter().map(amendment_dto).collect()))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AmendRequest {
    /// Существо изменений - попадает в баннер и в извещение участников
    pub summary: String,
    /// Новый срок приема заявок (п. 27: ≥ 10 календарных дней от публикации)
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub new_deadline: OffsetDateTime,
}

/// Публикация новой редакции документации (FR-304, п. 27): срок приема
/// продлевается, печатная форма сохраняется снимком, участники извещаются
/// и получают право отказаться с возвратом взноса (п. 26.5).
#[utoipa::path(
    post,
    path = "/api/v1/tenders/{id}/amendments",
    tag = "amendments",
    params(("id" = Uuid, Path, description = "Тендер")),
    request_body = AmendRequest,
    responses(
        (status = 201, description = "Редакция опубликована", body = AmendmentDto),
        (status = 404, description = "Тендер не найден", body = crate::error::Problem),
        (status = 409, description = "Изменение невозможно (п. 27)", body = crate::error::Problem),
    )
)]
pub async fn amend_tender(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AmendRequest>,
) -> Result<(StatusCode, Json<AmendmentDto>), ApiError> {
    user.require(Action::TenderManage)?;

    if body.summary.trim().is_empty() {
        return Err(ApiError::Validation(
            "редакция публикуется с описанием изменений (п. 27)".to_owned(),
        ));
    }

    let record = amendments::amend(&state.db, user.id(), id, &body.summary, body.new_deadline)
        .await
        .map_err(amendment_error)?;

    // Печатная форма новой редакции - снимок объявления после продления срока
    let pdf_bytes = render_announcement(&state, id).await?;
    let doc_key = format!("tenders/{id}/announcement-v{}.pdf", record.version);
    state
        .storage
        .put(
            &ObjectPath::from(doc_key.as_str()),
            PutPayload::from(pdf_bytes),
        )
        .await
        .map_err(ApiError::internal)?;
    amendments::attach_doc(&state.db, user.id(), record.id, &doc_key)
        .await
        .map_err(amendment_error)?;

    let tender = tenders::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    notify_participants(
        &state,
        &user,
        id,
        NotificationKind::TenderAmended,
        json!({
            "tender_id": id,
            "tender_title": tender.title,
            "version": record.version,
            "summary": record.summary,
            "new_deadline": rfc3339(record.new_deadline),
            "rule_ref": "п. 26.5, 27",
        }),
    )
    .await?;
    amendments::complete_notice(&state.db, user.id(), ObligationAction::NotifyAmendment, id)
        .await?;

    let record = amendments::get(&state.db, record.id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok((StatusCode::CREATED, Json(amendment_dto(record))))
}

/// Печатная форма редакции (Прил. 1 на момент изменения) из RustFS.
#[utoipa::path(
    get,
    path = "/api/v1/tender-amendments/{id}/announcement.pdf",
    tag = "amendments",
    params(("id" = Uuid, Path, description = "Редакция документации")),
    responses(
        (status = 200, description = "PDF редакции", content_type = "application/pdf"),
        (status = 404, description = "Редакция не найдена", body = crate::error::Problem),
    )
)]
pub async fn amendment_pdf(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let record = amendments::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let doc_key = record.doc_key.ok_or(ApiError::NotFound)?;

    let object = state
        .storage
        .get(&ObjectPath::from(doc_key.as_str()))
        .await
        .map_err(ApiError::internal)?;
    let bytes = object.bytes().await.map_err(ApiError::internal)?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/pdf".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!(
                    "inline; filename=\"tender-{}-announcement-v{}.pdf\"",
                    record.tender_id, record.version
                ),
            ),
        ],
        bytes,
    )
        .into_response())
}

/// Отказ участника от участия из-за изменения условий (FR-1004, п. 26.5):
/// заявка отзывается, взнос ставится на возврат.
#[utoipa::path(
    post,
    path = "/api/v1/applications/{id}/decline-amendment",
    tag = "amendments",
    params(("id" = Uuid, Path, description = "Заявка")),
    responses(
        (status = 200, description = "Заявка отозвана, взнос на возврате"),
        (status = 404, description = "Заявка не найдена", body = crate::error::Problem),
        (status = 409, description = "Условия тендера не изменялись", body = crate::error::Problem),
    )
)]
pub async fn decline_amendment(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    user.require(Action::ApplicationWithdraw)?;

    amendments::decline_amendment(&state.db, user.id(), id)
        .await
        .map_err(amendment_error)?;
    Ok(StatusCode::OK)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CancelRequest {
    /// Нарушение, повлекшее отмену (п. 78): отмена по усмотрению невозможна
    pub reason: String,
}

/// Отмена тендера (FR-305, п. 78–79): взносы участников идут на возврат,
/// участники извещаются, срок извещения - три рабочих дня.
#[utoipa::path(
    post,
    path = "/api/v1/tenders/{id}/cancel",
    tag = "amendments",
    params(("id" = Uuid, Path, description = "Тендер")),
    request_body = CancelRequest,
    responses(
        (status = 200, description = "Тендер отменен"),
        (status = 404, description = "Тендер не найден", body = crate::error::Problem),
        (status = 409, description = "Отмена невозможна (п. 78)", body = crate::error::Problem),
    )
)]
pub async fn cancel_tender(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<CancelRequest>,
) -> Result<StatusCode, ApiError> {
    user.require(Action::TenderManage)?;

    amendments::cancel_tender(&state.db, user.id(), id, &body.reason)
        .await
        .map_err(amendment_error)?;

    let tender = tenders::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    notify_participants(
        &state,
        &user,
        id,
        NotificationKind::TenderCancelled,
        json!({
            "tender_id": id,
            "tender_title": tender.title,
            "scope": CancellationScope::Tender.as_str(),
            "reason": body.reason.trim(),
            "rule_ref": "п. 78–79",
        }),
    )
    .await?;
    amendments::complete_notice(
        &state.db,
        user.id(),
        ObligationAction::NotifyCancellation,
        id,
    )
    .await?;

    Ok(StatusCode::OK)
}

/// Отмена отдельного лота (FR-305): тендер продолжается, объект лота
/// освобождается (FR-103), взносы по лоту идут на возврат.
#[utoipa::path(
    post,
    path = "/api/v1/lots/{id}/cancel",
    tag = "amendments",
    params(("id" = Uuid, Path, description = "Лот")),
    request_body = CancelRequest,
    responses(
        (status = 200, description = "Лот отменен"),
        (status = 404, description = "Лот не найден", body = crate::error::Problem),
        (status = 409, description = "Отмена невозможна (п. 78)", body = crate::error::Problem),
    )
)]
pub async fn cancel_lot(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<CancelRequest>,
) -> Result<StatusCode, ApiError> {
    user.require(Action::TenderManage)?;

    let tender_id = amendments::cancel_lot(&state.db, user.id(), id, &body.reason)
        .await
        .map_err(amendment_error)?;

    let tender = tenders::get(&state.db, tender_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    notify_participants(
        &state,
        &user,
        tender_id,
        NotificationKind::TenderCancelled,
        json!({
            "tender_id": tender_id,
            "tender_title": tender.title,
            "scope": CancellationScope::Lot.as_str(),
            "lot_id": id,
            "reason": body.reason.trim(),
            "rule_ref": "п. 78–79",
        }),
    )
    .await?;
    amendments::complete_notice(
        &state.db,
        user.id(),
        ObligationAction::NotifyCancellation,
        tender_id,
    )
    .await?;

    Ok(StatusCode::OK)
}

/// Извещение участников тендера (п. 27, 79): получатели - все, чьи заявки
/// не отозваны; повторные извещения об одном событии не рассылаются.
async fn notify_participants(
    state: &AppState,
    user: &CurrentUser,
    tender_id: Uuid,
    kind: NotificationKind,
    payload: serde_json::Value,
) -> Result<(), ApiError> {
    let participants = amendments::participants_of(&state.db, tender_id).await?;
    if participants.is_empty() {
        return Ok(());
    }

    let notices: Vec<tou_db::notifications::NewNotification> = participants
        .into_iter()
        .map(
            |(participant_id, application_id)| tou_db::notifications::NewNotification {
                user_id: participant_id,
                payload: {
                    let mut value = payload.clone();
                    if let Some(object) = value.as_object_mut() {
                        object.insert("application_id".to_owned(), json!(application_id));
                    }
                    value
                },
            },
        )
        .collect();

    tou_db::notifications::insert(&state.db, user.id(), kind.as_str(), &notices).await?;
    Ok(())
}

fn rfc3339(value: OffsetDateTime) -> String {
    value
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}
