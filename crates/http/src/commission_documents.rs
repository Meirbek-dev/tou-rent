//! Manual PDF documents, distinct from generated protocols and electronic votes.
use axum::{
    extract::State,
    http::header,
    response::{IntoResponse, Response},
};
use object_store::{ObjectStoreExt, PutPayload, path::Path as ObjectPath};
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime, macros::format_description};
use tou_db::commission_documents::{self as db, Document, NewDocument};
use tou_domain::policy::Action;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::{
    error::ApiError,
    extract::CurrentUser,
    request::{Json, Multipart, Path, Query},
    state::AppState,
    upload,
};

#[derive(Serialize, ToSchema)]
pub struct CommissionDocumentDto {
    pub id: Uuid,
    pub tender_id: Uuid,
    pub application_id: Option<Uuid>,
    pub title: String,
    pub number: String,
    pub document_date: String,
    pub filename: String,
    pub size_bytes: i64,
    pub uploaded_by: Uuid,
    #[schema(value_type = String, format = DateTime)]
    pub uploaded_at: OffsetDateTime,
    #[schema(value_type = Option<String>, format = DateTime)]
    pub shared_at: Option<OffsetDateTime>,
}
impl From<Document> for CommissionDocumentDto {
    fn from(d: Document) -> Self {
        Self {
            id: d.id,
            tender_id: d.tender_id,
            application_id: d.application_id,
            title: d.title,
            number: d.number,
            document_date: d.document_date.to_string(),
            filename: d.filename,
            size_bytes: d.size_bytes,
            uploaded_by: d.uploaded_by,
            uploaded_at: d.uploaded_at,
            shared_at: d.shared_at,
        }
    }
}
#[derive(Serialize, ToSchema)]
pub struct CommissionDocumentPage {
    pub items: Vec<CommissionDocumentDto>,
    pub truncated: bool,
}
fn page(rows: tou_db::Page<Document>) -> CommissionDocumentPage {
    CommissionDocumentPage {
        truncated: rows.truncated,
        items: rows.into_iter().map(Into::into).collect(),
    }
}

#[utoipa::path(get, path = "/api/v1/tenders/{id}/commission-documents", tag = "commission-documents",
    params(("id" = Uuid, Path)), responses((status = 200, body = CommissionDocumentPage)))]
pub async fn list_commission_documents(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<CommissionDocumentPage>, ApiError> {
    let manager = user.require(Action::ProtocolGenerate).is_ok();
    Ok(Json(page(
        db::list(&state.db, user.id(), manager, Some(id)).await?,
    )))
}

#[utoipa::path(get, path = "/api/v1/commission-documents/my", tag = "commission-documents",
    responses((status = 200, body = CommissionDocumentPage)))]
pub async fn my_commission_documents(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<CommissionDocumentPage>, ApiError> {
    user.require(Action::ApplicationReadOwn)?;
    Ok(Json(page(
        db::list(&state.db, user.id(), false, None).await?,
    )))
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct UploadCommissionDocument {
    pub application_id: Option<Uuid>,
    pub title: String,
    pub number: String,
    pub document_date: String,
}
fn text(value: &str, max: usize) -> Result<&str, ApiError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max || value.chars().any(char::is_control) {
        return Err(ApiError::Validation("invalid document metadata".into()));
    }
    Ok(value)
}

#[utoipa::path(post, path = "/api/v1/tenders/{id}/commission-documents", tag = "commission-documents",
    params(("id" = Uuid, Path), UploadCommissionDocument),
    request_body(content = String, content_type = "multipart/form-data"),
    responses((status = 200, body = CommissionDocumentDto), (status = 422, body = crate::error::Problem)))]
pub async fn upload_commission_document(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(tender_id): Path<Uuid>,
    Query(meta): Query<UploadCommissionDocument>,
    mut multipart: Multipart,
) -> Result<Json<CommissionDocumentDto>, ApiError> {
    user.require(Action::ProtocolGenerate)?;
    let title = text(&meta.title, 200)?;
    let number = text(&meta.number, 100)?;
    let date = Date::parse(
        &meta.document_date,
        format_description!("[year]-[month]-[day]"),
    )
    .map_err(|_| ApiError::Validation("invalid document date".into()))?;
    if !db::valid_target(&state.db, tender_id, meta.application_id).await? {
        return Err(ApiError::NotFound);
    }
    let file = upload::take_file(
        &mut multipart,
        "file",
        "document.pdf",
        upload::MAX_FILE_BYTES,
    )
    .await?;
    if file.content_type != "application/pdf" {
        return Err(ApiError::Validation("PDF required".into()));
    }
    let id = Uuid::now_v7();
    let key = format!("commission-documents/{tender_id}/{id}.pdf");
    let size_bytes = file.bytes.len() as i64;
    state
        .storage
        .put(
            &ObjectPath::from(key.as_str()),
            PutPayload::from_bytes(file.bytes),
        )
        .await
        .map_err(ApiError::internal)?;
    db::insert(
        &state.db,
        user.id(),
        NewDocument {
            id,
            tender_id,
            application_id: meta.application_id,
            title,
            number,
            document_date: date,
            filename: &file.filename,
            file_key: &key,
            size_bytes,
        },
    )
    .await?;
    let record = db::visible(&state.db, id, user.id(), true)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(record.into()))
}

#[derive(Deserialize, ToSchema)]
pub struct ShareCommissionDocument {
    pub shared: bool,
}

#[utoipa::path(put, path = "/api/v1/commission-documents/{id}/visibility", tag = "commission-documents",
    params(("id" = Uuid, Path)), request_body = ShareCommissionDocument,
    responses((status = 200, body = CommissionDocumentDto)))]
pub async fn share_commission_document(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ShareCommissionDocument>,
) -> Result<Json<CommissionDocumentDto>, ApiError> {
    user.require(Action::ProtocolGenerate)?;
    if !db::share(&state.db, user.id(), id, body.shared).await? {
        return Err(ApiError::NotFound);
    }
    let record = db::visible(&state.db, id, user.id(), true)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(record.into()))
}

#[utoipa::path(get, path = "/api/v1/commission-documents/{id}/pdf", tag = "commission-documents",
    params(("id" = Uuid, Path)), responses((status = 200, content_type = "application/pdf"), (status = 404, body = crate::error::Problem)))]
pub async fn commission_document_pdf(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let manager = user.require(Action::ProtocolGenerate).is_ok();
    let record = db::visible(&state.db, id, user.id(), manager)
        .await?
        .ok_or(ApiError::NotFound)?;
    let object = state
        .storage
        .get(&ObjectPath::from(record.file_key.as_str()))
        .await
        .map_err(ApiError::internal)?;
    let bytes = object.bytes().await.map_err(ApiError::internal)?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/pdf".to_owned()),
            (header::CACHE_CONTROL, "private, no-store".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"document-{id}.pdf\""),
            ),
        ],
        bytes,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_rejects_blank_and_control_characters() {
        for value in ["", "  ", "line\nbreak"] {
            assert!(text(value, 200).is_err());
        }
    }
    #[test]
    fn metadata_accepts_unicode_and_trims() {
        assert_eq!(text("  Протокол № 1  ", 200).unwrap(), "Протокол № 1");
    }
}
