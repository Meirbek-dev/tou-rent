//! Акты приема-передачи и возврата (М9, FR-904, Прил. 7–8).
//!
//! Составление акта меняет состояние аренды: с даты передачи начисляется
//! плата (п. 128–129), возврат закрывает договор и освобождает объект
//! (FR-103). Порядок актов стерегут домен и триггер БД.

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use object_store::ObjectStoreExt as _;
use object_store::PutPayload;
use object_store::path::Path as ObjectPath;
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;
use tou_db::acts::{self, ActError};
use tou_db::{contracts, objects, tenders};
use tou_domain::act::ActKind;
use tou_domain::policy::Action;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::admission::format_almaty;
use crate::announcement::format_decimal_ru;
use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::pdf;
use crate::request::{Json, Multipart, Path};
use crate::state::AppState;
use crate::upload;

const TEMPLATE: &str = include_str!("templates/act.typ");

fn act_error(err: ActError) -> ApiError {
    match err {
        ActError::NotFound => ApiError::NotFound,
        ActError::Rejected(reason) => ApiError::RuleViolation(reason),
        ActError::Db(db) => db.into(),
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ActDto {
    pub id: Uuid,
    pub contract_id: Uuid,
    /// `handover` - прием-передача (Прил. 7), `return` - возврат (Прил. 8)
    pub kind: String,
    pub title_ru: String,
    /// Приложение Правил с формой акта
    pub appendix: String,
    #[serde(with = "crate::dto::iso_date")]
    #[schema(value_type = String, format = Date)]
    pub act_date: time::Date,
    pub note: Option<String>,
    pub has_pdf: bool,
    pub has_scan: bool,
    /// Способ подписания (ТЗ § 2): `unsigned` | `paper` | `electronic`
    pub signature_status: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}

fn act_dto(record: acts::ActRecord) -> ActDto {
    ActDto {
        id: record.id,
        contract_id: record.contract_id,
        kind: record.kind.as_str().to_owned(),
        title_ru: record.kind.title_ru().to_owned(),
        appendix: record.kind.appendix().to_owned(),
        act_date: record.act_date,
        note: record.note,
        has_pdf: record.pdf_key.is_some(),
        has_scan: record.signed_scan_key.is_some(),
        signature_status: record.signature_status,
        created_at: record.created_at,
    }
}

/// Акты договора (FR-904).
#[utoipa::path(
    get,
    path = "/api/v1/contracts/{id}/acts",
    tag = "acts",
    params(("id" = Uuid, Path, description = "Договор")),
    responses((status = 200, description = "Акты договора", body = [ActDto]))
)]
pub async fn contract_acts(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<ActDto>>, ApiError> {
    let contract = contracts::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    // Свои акты видит наниматель, чужие - ведущие процесс: акт называет
    // стороны и объект, это не публичная часть тендера (R-13)
    if contract.tenant_id != user.id() {
        user.require(Action::ContractRead)?;
    }

    let records = acts::list_for_contract(&state.db, id).await?;
    Ok(Json(records.into_iter().map(act_dto).collect()))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateActRequest {
    /// `handover` либо `return`
    pub kind: String,
    /// Дата акта: с даты приема-передачи начисляется плата (п. 128–129)
    #[serde(with = "crate::dto::iso_date")]
    #[schema(value_type = String, format = Date)]
    pub act_date: time::Date,
    /// Состояние объекта и претензии сторон
    pub note: Option<String>,
}

/// Составление акта (FR-904, Прил. 7–8): передача включает начисление платы
/// и делает объект сданным, возврат закрывает договор (FR-103).
#[utoipa::path(
    post,
    path = "/api/v1/contracts/{id}/acts",
    tag = "acts",
    params(("id" = Uuid, Path, description = "Договор")),
    request_body = CreateActRequest,
    responses(
        (status = 201, description = "Акт составлен", body = ActDto),
        (status = 409, description = "Порядок актов нарушен (FR-904)", body = crate::error::Problem),
        (status = 422, description = "Неизвестный вид акта", body = crate::error::Problem),
    )
)]
pub async fn create_act(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<CreateActRequest>,
) -> Result<(StatusCode, Json<ActDto>), ApiError> {
    user.require(Action::ActManage)?;

    let kind: ActKind = body
        .kind
        .parse()
        .map_err(|_| ApiError::Validation(format!("неизвестный вид акта: {}", body.kind)))?;

    let record = acts::create(
        &state.db,
        user.id(),
        id,
        kind,
        body.act_date,
        body.note.as_deref().filter(|note| !note.is_empty()),
    )
    .await
    .map_err(act_error)?;

    let act_id = record.id;
    render_and_store(&state, &user, &record).await?;

    let record = acts::get(&state.db, act_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok((StatusCode::CREATED, Json(act_dto(record))))
}

/// Печатная форма акта (Прил. 7–8) в RustFS.
async fn render_and_store(
    state: &AppState,
    user: &CurrentUser,
    record: &acts::ActRecord,
) -> Result<(), ApiError> {
    let contract = contracts::get(&state.db, record.contract_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let object_id = sqlx::query_scalar!(
        "SELECT object_id FROM core.contracts WHERE id = $1",
        contract.id
    )
    .fetch_one(&state.db)
    .await?;
    let object = objects::get(&state.db, object_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let purpose = match contract.tender_id {
        Some(tender_id) => tenders::lots_of(&state.db, tender_id)
            .await?
            .into_iter()
            .find(|lot| Some(lot.id) == contract.lot_id)
            .map(|lot| lot.purpose)
            .unwrap_or_else(|| "-".to_owned()),
        None => "-".to_owned(),
    };

    let (transfer_text, effect_text) = match record.kind {
        ActKind::Handover => (
            "Наймодатель передал, а Наниматель принял во временное владение и пользование \
             указанный ниже объект.",
            "С даты настоящего акта начисляется арендная плата по договору (п. 122, 128–129).",
        ),
        ActKind::Return => (
            "Наниматель возвратил, а Наймодатель принял указанный ниже объект.",
            "Договор считается исполненным, объект возвращен наймодателю; депозит возвращается \
             в течение 5 рабочих дней при отсутствии претензий (п. 129, 136).",
        ),
    };

    // Отметка «сформировано» - по часам сервера (`core.now()`, ADR-0005),
    // а не процесса: при сдвинутых часах стенда документ обязан быть датирован
    // тем же днем, которым его считают правила
    let generated_at = tou_db::refdata::now(&state.db).await?;

    let data = json!({
        "title": record.kind.title_ru(),
        "appendix": record.kind.appendix(),
        "number": crate::admission::short_number(record.id),
        "contract_number": contract.reg_number.clone().unwrap_or_else(|| "б/н".to_owned()),
        "act_date": record.act_date.to_string(),
        "tenant_name": contract.tenant_name,
        "object_name": object.name,
        "object_address": object.address,
        "object_area": format_decimal_ru(object.area_m2),
        "purpose": purpose,
        "note": record.note.clone().unwrap_or_default(),
        "transfer_text": transfer_text,
        "effect_text": effect_text,
        "generated_at": format_almaty(Some(generated_at)),
    });

    let pdf_bytes = tokio::task::spawn_blocking(move || pdf::render(TEMPLATE, &data))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;

    let pdf_key = format!("contracts/{}/act-{}.pdf", contract.id, record.kind.as_str());
    state
        .storage
        .put(
            &ObjectPath::from(pdf_key.as_str()),
            PutPayload::from(pdf_bytes),
        )
        .await
        .map_err(ApiError::internal)?;

    acts::attach_pdf(&state.db, user.id(), record.id, &pdf_key)
        .await
        .map_err(act_error)
}

/// Скан подписанного акта (без ЭЦП).
#[utoipa::path(
    post,
    path = "/api/v1/acts/{id}/scan",
    tag = "acts",
    params(("id" = Uuid, Path, description = "Акт")),
    request_body(content = String, content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "Скан загружен", body = ActDto),
        (status = 422, description = "Файл не приложен", body = crate::error::Problem),
    )
)]
pub async fn upload_act_scan(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<ActDto>, ApiError> {
    user.require(Action::ActManage)?;

    // `attach_scan` - это UPDATE без проверки существования: по несуществующему
    // id он молча обновлял ноль строк, а объект к этому моменту уже лежал
    // в бакете под Object Lock (INV-042) навсегда. Поэтому акт ищется до `put`
    acts::get(&state.db, id).await?.ok_or(ApiError::NotFound)?;

    // Прежний разбор клал в бакет каждую часть формы, а ключ в БД записывал
    // от последней - остальные становились несносимыми сиротами
    let file = upload::take_file(&mut multipart, "file", "act.pdf", upload::MAX_FILE_BYTES).await?;

    let key = format!("acts/{id}/signed-{}", file.filename);
    state
        .storage
        .put(
            &ObjectPath::from(key.as_str()),
            PutPayload::from_bytes(file.bytes),
        )
        .await
        .map_err(ApiError::internal)?;

    acts::attach_scan(&state.db, user.id(), id, &key)
        .await
        .map_err(act_error)?;

    let record = acts::get(&state.db, id).await?.ok_or(ApiError::NotFound)?;
    Ok(Json(act_dto(record)))
}

/// PDF акта из RustFS.
#[utoipa::path(
    get,
    path = "/api/v1/acts/{id}/pdf",
    tag = "acts",
    params(("id" = Uuid, Path, description = "Акт")),
    responses(
        (status = 200, description = "PDF акта", content_type = "application/pdf"),
        (status = 404, description = "Печатная форма не сформирована", body = crate::error::Problem),
    )
)]
pub async fn act_pdf(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let record = acts::get(&state.db, id).await?.ok_or(ApiError::NotFound)?;
    let contract = contracts::get(&state.db, record.contract_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if contract.tenant_id != user.id() {
        user.require(Action::TenderRead)?;
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
                format!("inline; filename=\"act-{id}.pdf\""),
            ),
        ],
        bytes,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Обе формы (Прил. 7 и 8) компилируются в PDF.
    #[test]
    fn act_forms_render_pdf() {
        for kind in ActKind::ALL {
            let data = json!({
                "title": kind.title_ru(),
                "appendix": kind.appendix(),
                "number": "0TEST",
                "contract_number": "Д-2026/001",
                "act_date": "2026-08-20",
                "tenant_name": "ТОО «Тест» - спецсимволы *#_$@«»",
                "object_name": "Помещение 42 м²",
                "object_address": "Павлодар, Ломова 64",
                "object_area": "42,00",
                "purpose": "офис",
                "note": "",
                "transfer_text": "Наймодатель передал, а Наниматель принял объект.",
                "effect_text": "С даты акта начисляется арендная плата.",
                "generated_at": "20.08.2026 10:05",
            });

            let bytes = pdf::render(TEMPLATE, &data).unwrap();
            assert!(bytes.starts_with(b"%PDF"), "{kind:?}");
            assert!(bytes.len() > 1_000, "{kind:?}");
        }
    }
}
