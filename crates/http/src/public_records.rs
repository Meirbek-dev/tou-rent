//! Публикации особого порядка на публичном портале
//! (М12, М14: FR-1403, FR-1202, INV-076, п. 90, 92, 97).
//!
//! Публикуются три материала раздела 12: результат рассмотрения заявки
//! с обоснованием (п. 90, 97), обоснование ставки договора - расчет Прил. 4
//! (п. 97) и акт приемки инвестиций (п. 92). Механика та же, что у протоколов
//! (FR-702): публикуется сформированный документ, доступ длится шесть месяцев
//! и снимается джобом, а материал остается в досье решения (FR-1206).
//!
//! Публикует уполномоченное подразделение (в модели ролей - организатор,
//! A-068); список публикаций портала открыт всем, включая гостя (FR-1401).

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use object_store::ObjectStoreExt as _;
use object_store::path::Path as ObjectPath;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tou_db::RowCursor;
use tou_db::public_records::{self, NewPublicRecord, PublicRecord, PublicationError};
use tou_domain::policy::Action;
use tou_domain::publication::PublicRecordKind;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::dto::cursor;
use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::request::{Json, Path, Query};
use crate::state::AppState;
use tou_domain::rule::RuleViolation;

fn publication_error(err: PublicationError) -> ApiError {
    match err {
        PublicationError::NotFound => ApiError::NotFound,
        PublicationError::Rejected(reason) => ApiError::RuleViolation(reason),
        PublicationError::Db(db) => db.into(),
    }
}

/// Публикация особого порядка (FR-1403).
#[derive(Debug, Serialize, ToSchema)]
pub struct PublicRecordDto {
    pub id: Uuid,
    /// `decision` | `rate` | `investment_act`
    pub kind: String,
    pub kind_title_ru: String,
    /// Пункт Правил, по которому материал публикуется
    pub rule_ref: String,
    pub title: String,
    pub has_file: bool,
    /// Обоснование ставки - расчет Прил. 4 (FR-201); у прочих видов пусто
    #[schema(value_type = Object)]
    pub payload: serde_json::Value,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub published_at: OffsetDateTime,
    /// Момент автоматического снятия: публикация + 6 месяцев (INV-076)
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub unpublish_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub unpublished_at: Option<OffsetDateTime>,
    pub is_public: bool,
}

fn record_dto(record: PublicRecord) -> PublicRecordDto {
    let is_public = record.facts().is_public();
    PublicRecordDto {
        id: record.id,
        kind: record.kind.as_str().to_owned(),
        kind_title_ru: record.kind.title_ru().to_owned(),
        rule_ref: record.kind.rule_ref().to_owned(),
        title: record.title,
        has_file: record.file_key.is_some(),
        payload: record.payload,
        published_at: record.published_at,
        unpublish_at: record.unpublish_at,
        unpublished_at: record.unpublished_at,
        is_public,
    }
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct RecordsPageParams {
    /// Курсор следующей страницы - значение `next_after` предыдущей
    pub after: Option<String>,
    pub limit: Option<i64>,
}

/// Страница реестра портала (ТЗ § 7).
#[derive(Debug, Serialize, ToSchema)]
pub struct PublicRecordPage {
    pub items: Vec<PublicRecordDto>,
    /// Курсор продолжения; `null` - реестр показан до конца
    pub next_after: Option<String>,
    /// Показана не вся выборка
    pub truncated: bool,
}

/// Реестр публикаций особого порядка (FR-1403, п. 90, 92, 97): портал
/// показывает материалы, доступные сейчас - снятые по истечении шести
/// месяцев остаются только в досье (INV-076).
#[utoipa::path(
    get,
    path = "/api/v1/public-records",
    tag = "public-records",
    params(RecordsPageParams),
    responses((status = 200, description = "Страница публикаций особого порядка",
        body = PublicRecordPage))
)]
pub async fn list_public_records(
    State(state): State<AppState>,
    Query(params): Query<RecordsPageParams>,
) -> Result<Json<PublicRecordPage>, ApiError> {
    let after = params.after.as_deref().map(cursor::parse).transpose()?;
    let limit = crate::page_limit(params.limit);

    let page = public_records::list_public(&state.db, after, limit).await?;
    let truncated = page.truncated;
    let next_after = cursor::next(
        truncated,
        page.last()
            .map(|record| RowCursor::new(record.published_at, record.id)),
    );

    Ok(Json(PublicRecordPage {
        items: page.into_iter().map(record_dto).collect(),
        next_after,
        truncated,
    }))
}

/// Печатная форма публикации (FR-1403): открыта, пока материал публичен.
#[utoipa::path(
    get,
    path = "/api/v1/public-records/{id}/pdf",
    tag = "public-records",
    params(("id" = Uuid, Path, description = "Публикация")),
    responses(
        (status = 200, description = "PDF материала", content_type = "application/pdf"),
        (status = 404, description = "Материал не найден или снят", body = crate::error::Problem),
    )
)]
pub async fn public_record_pdf(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let record = public_records::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;

    // Снятый материал публично не отдается: он хранится в досье (п. 76)
    if !record.facts().is_public() {
        return Err(ApiError::NotFound);
    }
    let file_key = record.file_key.ok_or(ApiError::NotFound)?;

    let object = state
        .storage
        .get(&ObjectPath::from(file_key.as_str()))
        .await
        .map_err(ApiError::internal)?;
    let bytes = object.bytes().await.map_err(ApiError::internal)?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/pdf".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("inline; filename=\"public-record-{id}.pdf\""),
            ),
        ],
        bytes,
    )
        .into_response())
}

/// Материал, который Правила велят опубликовать (FR-1403).
#[derive(Debug, Serialize, ToSchema)]
pub struct PendingPublicationDto {
    /// `decision` | `rate` | `investment_act`
    pub kind: String,
    pub kind_title_ru: String,
    pub rule_ref: String,
    /// Заявка, договор или акт - предмет будущей публикации
    pub source_id: Uuid,
    pub title: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub occurred_at: OffsetDateTime,
    /// Материал готов: печатная форма сформирована либо расчет заморожен
    pub ready: bool,
}

/// Рабочий список ожидающих публикации (ТЗ § 7).
///
/// `next_after` здесь всегда `null`: продолжения у списка нет - он не
/// реестр, а очередь работы, и `truncated` означает не «есть вторая
/// страница», а «работы накопилось больше, чем система показывает».
#[derive(Debug, Serialize, ToSchema)]
pub struct PendingPublicationPage {
    pub items: Vec<PendingPublicationDto>,
    pub next_after: Option<String>,
    /// Показана не вся выборка
    pub truncated: bool,
}

/// Что ждет публикации (FR-1403, п. 97): рабочий список уполномоченного
/// подразделения - решения по публикуемым категориям, обоснования ставок
/// и акты приемки, по которым публикации еще нет.
#[utoipa::path(
    get,
    path = "/api/v1/public-records/pending",
    tag = "public-records",
    responses(
        (status = 200, description = "Ожидают публикации", body = PendingPublicationPage),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
    )
)]
pub async fn pending_publications(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<PendingPublicationPage>, ApiError> {
    user.require(Action::RecordPublish)?;

    let page = public_records::pending(&state.db).await?;
    let truncated = page.truncated;
    Ok(Json(PendingPublicationPage {
        items: page
            .into_iter()
            .map(|item| PendingPublicationDto {
                kind: item.kind.as_str().to_owned(),
                kind_title_ru: item.kind.title_ru().to_owned(),
                rule_ref: item.kind.rule_ref().to_owned(),
                source_id: item.source_id,
                title: item.title,
                occurred_at: item.occurred_at,
                ready: item.ready,
            })
            .collect(),
        next_after: None,
        truncated,
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PublishRecordRequest {
    /// Вид публикации: `decision` | `rate` | `investment_act`
    pub kind: String,
    /// Заявка (результат), договор (обоснование ставки) либо акт приемки
    pub source_id: Uuid,
}

/// Публикация материала особого порядка (FR-1403, п. 97): условия -
/// публикуемость категории, сформированная печатная форма и однократность -
/// проверяет БД, срок публичного доступа она же и считает (INV-076).
#[utoipa::path(
    post,
    path = "/api/v1/public-records",
    tag = "public-records",
    request_body = PublishRecordRequest,
    responses(
        (status = 201, description = "Материал опубликован", body = PublicRecordDto),
        (status = 404, description = "Материал не найден", body = crate::error::Problem),
        (status = 409, description = "Публикация невозможна (п. 87, 92, 97)", body = crate::error::Problem),
        (status = 422, description = "Неизвестный вид публикации", body = crate::error::Problem),
    )
)]
pub async fn publish_record(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<PublishRecordRequest>,
) -> Result<(StatusCode, Json<PublicRecordDto>), ApiError> {
    user.require(Action::RecordPublish)?;

    let kind: PublicRecordKind = body
        .kind
        .parse()
        .map_err(|_| ApiError::Validation(format!("неизвестный вид публикации: {}", body.kind)))?;

    let new = match kind {
        PublicRecordKind::Decision => decision_material(&state, body.source_id).await?,
        PublicRecordKind::Rate => rate_material(&state, body.source_id).await?,
        PublicRecordKind::InvestmentAct => act_material(&state, body.source_id).await?,
    };

    let record = public_records::publish(
        &state.db,
        user.id(),
        NewPublicRecord {
            kind,
            special_request_id: new.special_request_id,
            contract_id: new.contract_id,
            acceptance_id: new.acceptance_id,
            title: &new.title,
            file_key: new.file_key.as_deref(),
            payload: new.payload,
        },
    )
    .await
    .map_err(publication_error)?;

    Ok((StatusCode::CREATED, Json(record_dto(record))))
}

/// Собранный материал публикации: что именно ложится на портал.
struct Material {
    special_request_id: Option<Uuid>,
    contract_id: Option<Uuid>,
    acceptance_id: Option<Uuid>,
    title: String,
    file_key: Option<String>,
    payload: serde_json::Value,
}

/// Результат рассмотрения заявки (п. 90, 97): решение с обоснованием
/// и его протокол. Публикуемость категории проверяет БД (INV-087).
async fn decision_material(state: &AppState, request_id: Uuid) -> Result<Material, ApiError> {
    let request = tou_db::special::get(&state.db, request_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let decision = tou_db::special::decision_of(&state.db, request_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    Ok(Material {
        special_request_id: Some(request_id),
        contract_id: None,
        acceptance_id: None,
        title: format!(
            "Результат рассмотрения заявки особого порядка: {} ({})",
            request.category_label, request.category_rule_ref
        ),
        file_key: decision.pdf_key,
        // Обоснование публикуется вместе с результатом (п. 97)
        payload: serde_json::json!({
            "decision": decision.decision,
            "rationale": decision.rationale,
            "decided_at": decision.decided_at.unix_timestamp(),
            "object": request.object_name,
            "purpose": request.purpose,
        }),
    })
}

/// Обоснование ставки договора особого порядка (п. 97): публикуется снимок
/// расчета Прил. 4, замороженный при составлении договора (FR-201).
async fn rate_material(state: &AppState, contract_id: Uuid) -> Result<Material, ApiError> {
    let contracts = tou_db::investment::list(&state.db).await?;
    let contract = contracts
        .into_iter()
        .find(|record| record.contract_id == contract_id)
        .ok_or(ApiError::NotFound)?;

    let calculation = contract.rate_calculation.ok_or_else(|| {
        ApiError::rule(
            RuleViolation::SpecialPublication,
            "FR-1403: расчет ставки договора не заморожен - публиковать нечего (п. 97)",
        )
    })?;

    Ok(Material {
        special_request_id: None,
        contract_id: Some(contract_id),
        acceptance_id: None,
        title: format!(
            "Обоснование ставки договора особого порядка: {}",
            contract.object_name.as_deref().unwrap_or("объект")
        ),
        file_key: None,
        payload: serde_json::json!({
            "monthly_rate": contract.monthly_rate,
            "term_months": contract.term_months,
            "calculation": calculation,
        }),
    })
}

/// Акт приемки инвестиций (п. 92): публикуется его печатная форма.
async fn act_material(state: &AppState, acceptance_id: Uuid) -> Result<Material, ApiError> {
    let acceptance = tou_db::investment::acceptance(&state.db, acceptance_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    Ok(Material {
        special_request_id: None,
        contract_id: None,
        acceptance_id: Some(acceptance_id),
        title: format!("Акт приемки инвестиций от {}", acceptance.act_date),
        file_key: acceptance.pdf_key,
        payload: serde_json::json!({
            "accepted_amount": acceptance.accepted_amount,
            "act_date": acceptance.act_date.to_string(),
        }),
    })
}
