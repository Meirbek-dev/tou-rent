//! Допсоглашения к договору (М9: FR-906, FR-901, п. 125).
//!
//! Допсоглашение - отдельная сущность с diff-контролем: оно фиксирует, какое
//! поле договора и на что меняется. Существенные условия им не меняются -
//! это проверяет домен ([`tou_domain::contract::check_changes`]), закрытый
//! перечень справочника и триггер БД. Печатная форма формируется сразу
//! и вместе с фактом ложится в досье (FR-1602).

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use garde::Validate as _;
use object_store::ObjectStoreExt as _;
use object_store::PutPayload;
use object_store::path::Path as ObjectPath;
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::{Date, OffsetDateTime};
use tou_db::contract_amendments::{self, AmendmentError, AmendmentRecord};
use tou_domain::contract::{ContractField, FieldChange, check_changes};
use tou_domain::policy::Action;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::admission::{format_almaty, short_number};
use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::pdf;
use crate::request::{Json, Path};
use crate::state::AppState;

const TEMPLATE: &str = include_str!("templates/contract_amendment.typ");

fn amendment_error(err: AmendmentError) -> ApiError {
    match err {
        AmendmentError::NotFound => ApiError::NotFound,
        AmendmentError::Rejected(reason) => ApiError::RuleViolation(reason),
        AmendmentError::Db(db) => db.into(),
    }
}

/// Правка допсоглашения (FR-906): поле и его редакции.
#[derive(Debug, Serialize, ToSchema)]
pub struct AmendmentChangeDto {
    /// Код поля из перечня п. 125
    pub field_code: String,
    pub field_label: String,
    pub old_value: String,
    pub new_value: String,
}

/// Допсоглашение к договору (FR-906, п. 125).
#[derive(Debug, Serialize, ToSchema)]
pub struct ContractAmendmentDto {
    pub id: Uuid,
    pub contract_id: Uuid,
    /// Номер в рамках договора
    pub seq: i32,
    pub ground: String,
    #[schema(value_type = String, format = Date)]
    pub effective_on: String,
    pub has_pdf: bool,
    pub created_by_name: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    pub changes: Vec<AmendmentChangeDto>,
}

fn amendment_dto(record: AmendmentRecord) -> ContractAmendmentDto {
    ContractAmendmentDto {
        id: record.id,
        contract_id: record.contract_id,
        seq: record.seq,
        ground: record.ground,
        effective_on: record.effective_on.to_string(),
        has_pdf: record.pdf_key.is_some(),
        created_by_name: record.created_by_name,
        created_at: record.created_at,
        changes: record
            .changes
            .into_iter()
            .map(|change| AmendmentChangeDto {
                field_code: change.field_code,
                field_label: change.field_label,
                old_value: change.old_value,
                new_value: change.new_value,
            })
            .collect(),
    }
}

/// Допсоглашения договора (FR-906): их видят те, кто ведет договор,
/// и сам наниматель.
#[utoipa::path(
    get,
    path = "/api/v1/contracts/{id}/amendments",
    tag = "contract-amendments",
    params(("id" = Uuid, Path, description = "Договор")),
    responses(
        (status = 200, description = "Допсоглашения договора", body = [ContractAmendmentDto]),
        (status = 404, description = "Договор не найден", body = crate::error::Problem),
    )
)]
pub async fn contract_amendments(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<ContractAmendmentDto>>, ApiError> {
    let contract = tou_db::contracts::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if contract.tenant_id != user.id() {
        user.require(Action::TenderManage)
            .or_else(|_| user.require(Action::ApplicationReadAll))?;
    }

    let records = contract_amendments::list_for_contract(&state.db, id).await?;
    Ok(Json(records.into_iter().map(amendment_dto).collect()))
}

#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct AmendmentChangeRequest {
    /// Поле из перечня п. 125: существенные условия им не меняются (FR-901)
    #[garde(length(chars, min = 1, max = 64))]
    pub field_code: String,
    #[garde(length(chars, max = 2000))]
    pub old_value: String,
    #[garde(length(chars, min = 1, max = 2000))]
    pub new_value: String,
}

#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct CreateAmendmentRequest {
    /// Основание допсоглашения (п. 125)
    #[garde(length(chars, min = 1, max = 4000))]
    pub ground: String,
    /// Дата вступления в силу
    #[garde(skip)]
    #[schema(value_type = String, format = Date, example = "2026-09-01")]
    pub effective_on: String,
    #[garde(dive)]
    #[garde(length(min = 1))]
    pub changes: Vec<AmendmentChangeRequest>,
}

/// Допсоглашение к договору (FR-906, п. 125): diff-контроль пропускает
/// только то, что Правила менять разрешают, - существенные условия
/// отклоняются и доменом, и триггером БД (FR-901).
#[utoipa::path(
    post,
    path = "/api/v1/contracts/{id}/amendments",
    tag = "contract-amendments",
    params(("id" = Uuid, Path, description = "Договор")),
    request_body = CreateAmendmentRequest,
    responses(
        (status = 201, description = "Допсоглашение заключено", body = ContractAmendmentDto),
        (status = 409, description = "Изменение невозможно (FR-901, п. 125–126)", body = crate::error::Problem),
        (status = 422, description = "Данные не прошли проверку", body = crate::error::Problem),
    )
)]
pub async fn create_amendment(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<CreateAmendmentRequest>,
) -> Result<(StatusCode, Json<ContractAmendmentDto>), ApiError> {
    user.require(Action::TenderManage)?;
    body.validate()
        .map_err(|r| ApiError::Validation(r.to_string()))?;

    let effective_on = Date::parse(
        &body.effective_on,
        &time::format_description::well_known::Iso8601::DATE,
    )
    .map_err(|_| ApiError::Validation("дата вступления в силу - ISO 8601".to_owned()))?;

    // Diff-контроль в домене: защищенное поле значением не станет (FR-901)
    let changes: Vec<FieldChange> = body
        .changes
        .iter()
        .map(|change| {
            let field: ContractField = change.field_code.parse().map_err(|_| {
                ApiError::Validation(format!("неизвестное поле договора: {}", change.field_code))
            })?;
            Ok(FieldChange {
                field,
                old_value: change.old_value.trim().to_owned(),
                new_value: change.new_value.trim().to_owned(),
            })
        })
        .collect::<Result<_, ApiError>>()?;
    check_changes(&changes).map_err(|err| ApiError::RuleViolation(err.to_string()))?;

    let rows: Vec<contract_amendments::NewChange<'_>> = changes
        .iter()
        .map(|change| contract_amendments::NewChange {
            field_code: change.field.as_str(),
            old_value: change.old_value.as_str(),
            new_value: change.new_value.as_str(),
        })
        .collect();

    let record = contract_amendments::create(
        &state.db,
        user.id(),
        id,
        body.ground.trim(),
        effective_on,
        &rows,
    )
    .await
    .map_err(amendment_error)?;

    // Печатная форма сразу: соглашение без документа сторонам не вручить
    let record = render_pdf(&state, &user, record).await?;

    Ok((StatusCode::CREATED, Json(amendment_dto(record))))
}

/// Печатная форма допсоглашения (п. 125) и ее ключ в досье (FR-1602).
async fn render_pdf(
    state: &AppState,
    user: &CurrentUser,
    record: AmendmentRecord,
) -> Result<AmendmentRecord, ApiError> {
    let contract = tou_db::contracts::get(&state.db, record.contract_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    // Отметка «сформировано» - по часам сервера (`core.now()`, ADR-0005)
    let generated_at = tou_db::refdata::now(&state.db).await?;

    let data = json!({
        "number": record.seq,
        "contract_number": contract.reg_number.clone().unwrap_or_else(|| "б/н".to_owned()),
        "effective_on": record.effective_on.to_string(),
        "tenant_name": contract.tenant_name,
        "ground": record.ground,
        "changes": record.changes.iter().map(|change| json!({
            "field": change.field_label,
            "old_value": change.old_value,
            "new_value": change.new_value,
        })).collect::<Vec<_>>(),
        "generated_at": format_almaty(Some(generated_at)),
    });

    let pdf_bytes = tokio::task::spawn_blocking(move || pdf::render(TEMPLATE, &data))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;

    let pdf_key = format!(
        "contracts/{}/amendment-{}.pdf",
        record.contract_id, record.seq
    );
    state
        .storage
        .put(
            &ObjectPath::from(pdf_key.as_str()),
            PutPayload::from(pdf_bytes),
        )
        .await
        .map_err(ApiError::internal)?;

    contract_amendments::attach_pdf(&state.db, user.id(), record.id, &pdf_key)
        .await
        .map_err(amendment_error)?;

    contract_amendments::get(&state.db, record.id)
        .await?
        .ok_or(ApiError::NotFound)
}

/// PDF допсоглашения (п. 125): нанимателю и тем, кто ведет договор.
#[utoipa::path(
    get,
    path = "/api/v1/amendments/{id}/pdf",
    tag = "contract-amendments",
    params(("id" = Uuid, Path, description = "Допсоглашение")),
    responses(
        (status = 200, description = "PDF допсоглашения", content_type = "application/pdf"),
        (status = 404, description = "Допсоглашение или форма не найдены", body = crate::error::Problem),
    )
)]
pub async fn contract_amendment_pdf(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let record = contract_amendments::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let contract = tou_db::contracts::get(&state.db, record.contract_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if contract.tenant_id != user.id() {
        user.require(Action::TenderManage)
            .or_else(|_| user.require(Action::ApplicationReadAll))?;
    }

    let pdf_key = record.pdf_key.ok_or(ApiError::NotFound)?;
    let object = state
        .storage
        .get(&ObjectPath::from(pdf_key.as_str()))
        .await
        .map_err(ApiError::internal)?;
    let bytes = object.bytes().await.map_err(ApiError::internal)?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/pdf".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!(
                    "inline; filename=\"amendment-{}-{}.pdf\"",
                    short_number(record.contract_id),
                    record.seq
                ),
            ),
        ],
        bytes,
    )
        .into_response())
}

/// Поле договора, изменяемое допсоглашением (FR-906, п. 125).
#[derive(Debug, Serialize, ToSchema)]
pub struct AmendableFieldDto {
    pub code: String,
    pub label_ru: String,
    pub label_kk: Option<String>,
    pub label_en: Option<String>,
    pub rule_ref: String,
}

/// Перечень изменяемых полей (FR-906, п. 125): существенных условий в нем
/// нет - их не меняет ни приложение, ни допсоглашение (FR-901).
#[utoipa::path(
    get,
    path = "/api/v1/refdata/amendable-fields",
    tag = "contract-amendments",
    responses((status = 200, description = "Изменяемые поля договора", body = [AmendableFieldDto]))
)]
pub async fn amendable_fields(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<AmendableFieldDto>>, ApiError> {
    // Перечень нужен тому, кто составляет допсоглашение (п. 125)
    user.require(Action::TenderManage)?;

    let records = contract_amendments::list_fields(&state.db).await?;
    Ok(Json(
        records
            .into_iter()
            .map(|record| AmendableFieldDto {
                code: record.code,
                label_ru: record.label_ru,
                label_kk: record.label_kk,
                label_en: record.label_en,
                rule_ref: record.rule_ref,
            })
            .collect(),
    ))
}
