//! Тендеры (М3): создание с лотами (снимок ставки считает сервер - FR-202,
//! FR-301), публикация и переходы (законность решает триггер INV-021/FR-303),
//! публичный реестр (п. 5–6).

use std::collections::HashMap;

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use garde::Validate as _;
use object_store::path::Path as ObjectPath;
use object_store::{ObjectStoreExt as _, PutPayload};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tou_db::tenders::{
    self, DraftFields, LotRecord, NewLot, TenderDocRecord, TenderRecord, TransitionError,
};
use tou_domain::policy::Action;
use tou_domain::rates::RateUnit;
use tou_domain::tender::TenderStatus;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::dto::TenderStatusDto;
use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::rates::{RateOptionsDto, build_calculation, build_hourly_calculation};
use crate::request::{Json, Multipart, Path, Query};
use crate::state::AppState;
use crate::upload;
use tou_domain::rule::RuleViolation;

#[derive(Debug, Serialize, ToSchema)]
pub struct LotDto {
    pub id: Uuid,
    pub seq: i32,
    pub object_id: Uuid,
    /// Целевое назначение (Прил. 1 табл. 2)
    pub purpose: String,
    pub purpose_kk: String,
    pub lease_months: i32,
    /// Месячная базовая ставка - снимок FR-202
    #[schema(value_type = String, example = "21000")]
    pub base_rate_monthly: Decimal,
    /// Гарантийный взнос = месячная ставка (FR-206)
    #[schema(value_type = String, example = "21000")]
    pub guarantee_fee: Decimal,
    /// Полный RateCalculation (объяснимость, FR-201)
    #[schema(value_type = Object)]
    pub rate_calculation: serde_json::Value,
    pub viewing_terms: Option<String>,
    /// Единица ставки (FR-205): `monthly` - за месяц, `hourly` - за час (п. 97)
    pub rate_unit: String,
    /// Объем разыгрываемых часов почасового лота
    pub hours_total: Option<i32>,
    /// Лот отменен (FR-305, п. 78): причина обязательна
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub cancelled_at: Option<OffsetDateTime>,
    pub cancel_reason: Option<String>,
}

impl From<LotRecord> for LotDto {
    fn from(r: LotRecord) -> Self {
        Self {
            id: r.id,
            seq: r.seq,
            object_id: r.object_id,
            purpose: r.purpose,
            purpose_kk: r.purpose_kk,
            lease_months: r.lease_months,
            base_rate_monthly: r.base_rate_monthly,
            guarantee_fee: r.guarantee_fee,
            rate_calculation: r.rate_calculation,
            viewing_terms: r.viewing_terms,
            rate_unit: r.rate_unit,
            hours_total: r.hours_total,
            cancelled_at: r.cancelled_at,
            cancel_reason: r.cancel_reason,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TenderDto {
    pub id: Uuid,
    pub status: TenderStatusDto,
    pub title: String,
    pub organizer_id: Uuid,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub announced_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub submission_deadline: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub opening_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub opened_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub trading_at: Option<OffsetDateTime>,
    /// Ссылка на Zoom-конференцию торгов (FR-306)
    pub zoom_url: Option<String>,
    pub zoom_recording_url: Option<String>,
    pub repeat_of: Option<Uuid>,
    pub lots: Vec<LotDto>,
}

impl TenderDto {
    pub(crate) fn from_record(r: TenderRecord, lots: Vec<LotRecord>) -> Result<Self, ApiError> {
        Ok(Self {
            id: r.id,
            status: TenderStatusDto::from_db(&r.status)?,
            title: r.title,
            organizer_id: r.organizer_id,
            announced_at: r.announced_at,
            submission_deadline: r.submission_deadline,
            opening_at: r.opening_at,
            opened_at: r.opened_at,
            trading_at: r.trading_at,
            zoom_url: r.zoom_url,
            zoom_recording_url: r.zoom_recording_url,
            repeat_of: r.repeat_of,
            lots: lots.into_iter().map(LotDto::from).collect(),
        })
    }

    /// Ссылки на конференцию и запись торгов (FR-306) - не публичные сведения:
    /// по ним попадают в комнату, где идут торги допущенных (п. 62–65).
    /// Анонимный читатель публичного реестра их не получает; допущенные
    /// узнают ссылку из уведомления FR-504, остальные роли - из карточки.
    fn without_meeting_links(mut self) -> Self {
        self.zoom_url = None;
        self.zoom_recording_url = None;
        self
    }
}

#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct CreateLotRequest {
    #[garde(skip)]
    pub object_id: Uuid,
    #[garde(length(chars, min = 1, max = 500))]
    pub purpose: String,
    #[garde(length(chars, min = 1, max = 500))]
    pub purpose_kk: String,
    #[garde(range(min = 1, max = 240))]
    pub lease_months: i32,
    /// Опции коэффициентов Прил. 4; снимок ставки считает сервер (FR-202)
    #[garde(skip)]
    #[serde(default)]
    pub rate_options: RateOptionsDto,
    #[garde(inner(length(chars, max = 1000)))]
    pub viewing_terms: Option<String>,
    /// FR-205: `monthly` (по умолчанию) либо `hourly` - почасовая аренда (п. 97)
    #[garde(skip)]
    #[serde(default)]
    pub rate_unit: Option<String>,
    /// Объем разыгрываемых часов: обязателен у почасового лота (п. 97)
    #[garde(inner(range(min = 1, max = 10000)))]
    pub hours_total: Option<i32>,
}

impl CreateLotRequest {
    /// Единица ставки лота (FR-205): по умолчанию помесячная (п. 137).
    fn unit(&self) -> Result<RateUnit, ApiError> {
        match self.rate_unit.as_deref() {
            None => Ok(RateUnit::Monthly),
            Some(raw) => raw
                .parse()
                .map_err(|_| ApiError::Validation(format!("неизвестная единица ставки: {raw}"))),
        }
    }
}

#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct CreateTenderRequest {
    #[garde(length(chars, min = 1, max = 300))]
    pub title: String,
    #[garde(length(min = 1, max = 50), dive)]
    pub lots: Vec<CreateLotRequest>,
}

/// Правка черновика: даты процесса и ссылка Zoom (FR-301, FR-303, FR-306).
#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct UpdateTenderRequest {
    #[garde(length(chars, min = 1, max = 300))]
    pub title: String,
    #[garde(skip)]
    #[serde(default, with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub submission_deadline: Option<OffsetDateTime>,
    #[garde(skip)]
    #[serde(default, with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub opening_at: Option<OffsetDateTime>,
    #[garde(skip)]
    #[serde(default, with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub trading_at: Option<OffsetDateTime>,
    #[garde(inner(url, length(max = 500)))]
    pub zoom_url: Option<String>,
}

/// Ссылка на запись состоявшихся торгов (FR-306, п. 72). `null` или пустая
/// строка снимают ошибочно внесенную ссылку.
#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct SetRecordingRequest {
    #[garde(inner(url, length(max = 500)))]
    pub recording_url: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListTendersParams {
    pub after: Option<Uuid>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TenderPage {
    pub items: Vec<TenderDto>,
    pub next_after: Option<Uuid>,
}

/// Реестр тендеров: гость видит опубликованные (п. 5–6),
/// organizer - включая черновики.
#[utoipa::path(
    get,
    path = "/api/v1/tenders",
    tag = "tenders",
    params(ListTendersParams),
    responses((status = 200, description = "Страница тендеров", body = TenderPage))
)]
pub async fn list_tenders(
    user: Option<CurrentUser>,
    State(state): State<AppState>,
    Query(params): Query<ListTendersParams>,
) -> Result<Json<TenderPage>, ApiError> {
    let sees_drafts = user
        .as_ref()
        .map(|u| u.require(Action::TenderManage).is_ok())
        .unwrap_or(false);

    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    let records = tenders::list(&state.db, params.after, limit, !sees_drafts).await?;

    let next_after = (records.len() as i64 == limit)
        .then(|| records.last().map(|r| r.id))
        .flatten();

    // Лоты всей страницы одним запросом (без N+1)
    let ids: Vec<Uuid> = records.iter().map(|r| r.id).collect();
    let mut lots_by_tender = HashMap::<Uuid, Vec<LotRecord>>::new();
    for lot in tenders::lots_for(&state.db, &ids).await? {
        lots_by_tender.entry(lot.tender_id).or_default().push(lot);
    }

    let anonymous = user.is_none();
    let items = records
        .into_iter()
        .map(|record| {
            let lots = lots_by_tender.remove(&record.id).unwrap_or_default();
            let dto = TenderDto::from_record(record, lots)?;
            Ok(if anonymous {
                dto.without_meeting_links()
            } else {
                dto
            })
        })
        .collect::<Result<_, ApiError>>()?;
    Ok(Json(TenderPage { items, next_after }))
}

/// Карточка тендера: черновик виден только organizer.
#[utoipa::path(
    get,
    path = "/api/v1/tenders/{id}",
    tag = "tenders",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 200, description = "Тендер с лотами", body = TenderDto),
        (status = 404, description = "Не найден", body = crate::error::Problem),
    )
)]
pub async fn get_tender(
    user: Option<CurrentUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<TenderDto>, ApiError> {
    let record = tenders::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;

    if record.status == TenderStatus::Draft.as_str() {
        let manages = user
            .as_ref()
            .map(|u| u.require(Action::TenderManage).is_ok())
            .unwrap_or(false);
        if !manages {
            return Err(ApiError::NotFound); // черновик не публикуется (п. 5–6)
        }
    }

    let lots = tenders::lots_of(&state.db, record.id).await?;
    let dto = TenderDto::from_record(record, lots)?;
    Ok(Json(if user.is_none() {
        dto.without_meeting_links()
    } else {
        dto
    }))
}

/// Создание тендера с лотами (FR-301): базовая ставка, взнос и полный
/// RateCalculation замораживаются сервером из refdata на дату создания.
#[utoipa::path(
    post,
    path = "/api/v1/tenders",
    tag = "tenders",
    request_body = CreateTenderRequest,
    responses(
        (status = 201, description = "Тендер создан (draft)", body = TenderDto),
        (status = 422, description = "Данные не прошли проверку", body = crate::error::Problem),
    )
)]
pub async fn create_tender(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CreateTenderRequest>,
) -> Result<(StatusCode, Json<TenderDto>), ApiError> {
    user.require(Action::TenderManage)?;
    body.validate()
        .map_err(|r| ApiError::Validation(r.to_string()))?;

    // Снимки ставок до записи: любые ошибки refdata валят запрос целиком
    let mut prepared = Vec::with_capacity(body.lots.len());
    for lot in &body.lots {
        let unit = lot.unit()?;
        let object = tou_db::objects::get(&state.db, lot.object_id)
            .await?
            .ok_or_else(|| ApiError::Validation(format!("объект {} не найден", lot.object_id)))?;

        // Почасовой лот считается по своей методике (FR-205, п. 97): база -
        // 2 МРП за час, площадь в расчет не входит
        let (base_rate, fee, calc_json) = match unit {
            RateUnit::Hourly => {
                let hours = lot.hours_total.ok_or_else(|| {
                    ApiError::Validation(
                        "почасовой лот задается объемом часов (FR-205, п. 97)".to_owned(),
                    )
                })?;
                let rate = build_hourly_calculation(&state.db, &lot.rate_options).await?;
                let total = rate.total_for(hours).ok_or_else(|| {
                    ApiError::Validation("объем часов должен быть положительным".to_owned())
                })?;
                let json = serde_json::to_value(&rate).map_err(ApiError::internal)?;
                (rate.hourly.amount(), total.amount(), json)
            }
            RateUnit::Monthly => {
                let calc = build_calculation(&state.db, object.area_m2, &lot.rate_options).await?;
                let json = serde_json::to_value(&calc).map_err(ApiError::internal)?;
                (calc.monthly.amount(), calc.guarantee_fee.amount(), json)
            }
        };
        prepared.push((lot, unit, base_rate, fee, calc_json));
    }

    let new_lots: Vec<NewLot<'_>> = prepared
        .iter()
        .map(|(lot, unit, base_rate, fee, calc_json)| NewLot {
            object_id: lot.object_id,
            purpose: &lot.purpose,
            purpose_kk: &lot.purpose_kk,
            lease_months: lot.lease_months,
            base_rate_monthly: *base_rate,
            guarantee_fee: *fee, // FR-206; для почасового лота пересчитает БД
            rate_calculation: calc_json,
            viewing_terms: lot.viewing_terms.as_deref(),
            rate_unit: unit.as_str(),
            hours_total: lot.hours_total,
        })
        .collect();

    // Тендер и лоты - одна транзакция: частично созданных тендеров не бывает
    let (record, lots) =
        tenders::create(&state.db, user.id(), &body.title, user.id(), &new_lots).await?;

    Ok((
        StatusCode::CREATED,
        Json(TenderDto::from_record(record, lots)?),
    ))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TenderDocumentDto {
    pub id: Uuid,
    pub tender_id: Uuid,
    pub version: i32,
    pub title: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub published_at: OffsetDateTime,
}

impl From<TenderDocRecord> for TenderDocumentDto {
    fn from(record: TenderDocRecord) -> Self {
        Self {
            id: record.id,
            tender_id: record.tender_id,
            version: record.version,
            title: record.title,
            published_at: record.published_at,
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct UploadTenderDocumentQuery {
    pub title: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/tenders/{id}/documents",
    tag = "tenders",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses((status = 200, description = "Опубликованные PDF-документы", body = [TenderDocumentDto]))
)]
pub async fn list_documents(
    user: Option<CurrentUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<TenderDocumentDto>>, ApiError> {
    let tender = tenders::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if tender.status == TenderStatus::Draft.as_str()
        && !user
            .as_ref()
            .is_some_and(|current| current.require(Action::TenderManage).is_ok())
    {
        return Err(ApiError::NotFound);
    }
    Ok(Json(
        tenders::documents(&state.db, id)
            .await?
            .into_iter()
            .map(TenderDocumentDto::from)
            .collect(),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/tenders/{id}/documents",
    tag = "tenders",
    params(("id" = Uuid, Path, description = "Тендер"), UploadTenderDocumentQuery),
    request_body(content = Vec<u8>, content_type = "multipart/form-data"),
    responses((status = 201, description = "Документ добавлен", body = TenderDocumentDto))
)]
pub async fn upload_document(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<UploadTenderDocumentQuery>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<TenderDocumentDto>), ApiError> {
    user.require(Action::TenderManage)?;
    let title = query.title.trim();
    if title.is_empty() || title.chars().count() > 300 {
        return Err(ApiError::Validation(
            "название документа должно содержать от 1 до 300 символов".to_owned(),
        ));
    }
    let tender = tenders::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if tender.status != TenderStatus::Draft.as_str() {
        return Err(ApiError::rule(
            RuleViolation::TenderDocumentationChange,
            "документацию можно загрузить только до публикации тендера",
        ));
    }
    let file = upload::take_file(
        &mut multipart,
        "file",
        "tender-document.pdf",
        upload::MAX_FILE_BYTES,
    )
    .await?;
    if file.content_type != "application/pdf" {
        return Err(ApiError::UnsupportedMediaType(
            "тендерная документация принимается только в формате PDF".to_owned(),
        ));
    }
    let file_key = format!("tenders/{id}/documents/{}.pdf", Uuid::now_v7());
    state
        .storage
        .put(
            &ObjectPath::from(file_key.as_str()),
            PutPayload::from_bytes(file.bytes),
        )
        .await
        .map_err(ApiError::internal)?;
    let record = tenders::add_document(&state.db, user.id(), id, title, &file_key)
        .await?
        .ok_or_else(|| {
            ApiError::rule(
                RuleViolation::TenderDocumentationChange,
                "тендер уже опубликован; состав документации зафиксирован",
            )
        })?;
    Ok((StatusCode::CREATED, Json(record.into())))
}

#[utoipa::path(
    get,
    path = "/api/v1/tenders/{id}/documents/{document_id}",
    tag = "tenders",
    params(("id" = Uuid, Path, description = "Тендер"), ("document_id" = Uuid, Path, description = "Документ")),
    responses((status = 200, description = "Тендерный PDF", content_type = "application/pdf"))
)]
pub async fn download_document(
    State(state): State<AppState>,
    Path((id, document_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, ApiError> {
    tenders::get(&state.db, id)
        .await?
        .filter(|record| record.status != TenderStatus::Draft.as_str())
        .ok_or(ApiError::NotFound)?;
    let record = tenders::document(&state.db, document_id)
        .await?
        .filter(|record| record.tender_id == id)
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
            (
                header::CONTENT_DISPOSITION,
                format!(
                    "inline; filename=\"tender-document-v{}.pdf\"",
                    record.version
                ),
            ),
        ],
        bytes,
    )
        .into_response())
}

/// Правка черновика (даты, Zoom-ссылка).
#[utoipa::path(
    put,
    path = "/api/v1/tenders/{id}",
    tag = "tenders",
    params(("id" = Uuid, Path, description = "Тендер")),
    request_body = UpdateTenderRequest,
    responses(
        (status = 200, description = "Черновик обновлен", body = TenderDto),
        (status = 404, description = "Не найден", body = crate::error::Problem),
        (status = 409, description = "Тендер уже не черновик", body = crate::error::Problem),
    )
)]
pub async fn update_tender(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateTenderRequest>,
) -> Result<Json<TenderDto>, ApiError> {
    user.require(Action::TenderManage)?;
    body.validate()
        .map_err(|r| ApiError::Validation(r.to_string()))?;

    let updated = tenders::update_draft(
        &state.db,
        user.id(),
        id,
        DraftFields {
            title: &body.title,
            submission_deadline: body.submission_deadline,
            opening_at: body.opening_at,
            trading_at: body.trading_at,
            zoom_url: body.zoom_url.as_deref(),
        },
    )
    .await?;

    match updated {
        Some(record) => {
            let lots = tenders::lots_of(&state.db, record.id).await?;
            Ok(Json(TenderDto::from_record(record, lots)?))
        }
        None => match tenders::get(&state.db, id).await? {
            Some(_) => Err(ApiError::rule(
                RuleViolation::TenderDocumentationChange,
                "правка полей возможна только в статусе draft (FR-304 - контур 2)",
            )),
            None => Err(ApiError::NotFound),
        },
    }
}

/// Ссылка на запись торгов (FR-306, п. 72): вносится после подведения итогов.
#[utoipa::path(
    put,
    path = "/api/v1/tenders/{id}/recording",
    tag = "tenders",
    params(("id" = Uuid, Path, description = "Тендер")),
    request_body = SetRecordingRequest,
    responses(
        (status = 200, description = "Ссылка на запись сохранена", body = TenderDto),
        (status = 404, description = "Не найден", body = crate::error::Problem),
        (status = 409, description = "Торги еще не подведены", body = crate::error::Problem),
    )
)]
pub async fn set_recording(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<SetRecordingRequest>,
) -> Result<Json<TenderDto>, ApiError> {
    // Карточку торгов ведет секретарь (п. 65, 72) - то же право, что и на
    // управление аукционом; организатор ссылку на запись не правит
    user.require(Action::AuctionManage)?;

    // Пустая строка - снятие ошибочно внесенной ссылки, а не пустая ссылка:
    // нормализация идет до проверки, иначе `url` отверг бы очистку поля
    let body = SetRecordingRequest {
        recording_url: body
            .recording_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
    };
    body.validate()
        .map_err(|r| ApiError::Validation(r.to_string()))?;

    match tenders::set_recording_url(&state.db, user.id(), id, body.recording_url.as_deref())
        .await?
    {
        Some(record) => {
            let lots = tenders::lots_of(&state.db, record.id).await?;
            Ok(Json(TenderDto::from_record(record, lots)?))
        }
        None => match tenders::get(&state.db, id).await? {
            Some(_) => Err(ApiError::rule(
                RuleViolation::PublicRecordLink,
                "ссылка на запись вносится после подведения итогов торгов (FR-306, п. 72)",
            )),
            None => Err(ApiError::NotFound),
        },
    }
}

/// Публикация объявления (FR-303): draft → announced.
/// Правило «≥ 10 календарных дней до вскрытия» проверяет триггер БД.
#[utoipa::path(
    post,
    path = "/api/v1/tenders/{id}/publish",
    tag = "tenders",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 200, description = "Опубликован", body = TenderDto),
        (status = 409, description = "Переход отклонен (INV-021, FR-303)", body = crate::error::Problem),
    )
)]
pub async fn publish_tender(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<TenderDto>, ApiError> {
    user.require(Action::TenderPublish)?;
    transition(&state, user.id(), id, TenderStatus::Announced).await
}

/// Открытие приема заявок (п. 36): announced → accepting.
#[utoipa::path(
    post,
    path = "/api/v1/tenders/{id}/open-acceptance",
    tag = "tenders",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 200, description = "Прием открыт", body = TenderDto),
        (status = 409, description = "Переход отклонен (INV-021)", body = crate::error::Problem),
    )
)]
pub async fn open_acceptance(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<TenderDto>, ApiError> {
    user.require(Action::TenderPublish)?;
    transition(&state, user.id(), id, TenderStatus::Accepting).await
}

// Отмена тендера - `amendments::cancel_tender` (FR-305): она требует
// основания и извещает участников, поэтому живет вместе с изменениями
// документации, а не среди обычных переходов статуса.

async fn transition(
    state: &AppState,
    actor: Uuid,
    id: Uuid,
    to: TenderStatus,
) -> Result<Json<TenderDto>, ApiError> {
    match tenders::transition(&state.db, actor, id, to.as_str()).await {
        Ok(record) => {
            let lots = tenders::lots_of(&state.db, record.id).await?;
            Ok(Json(TenderDto::from_record(record, lots)?))
        }
        Err(TransitionError::NotFound) => Err(ApiError::NotFound),
        Err(TransitionError::Rejected(reason)) => Err(ApiError::RuleViolation(reason)),
        Err(TransitionError::Db(err)) => Err(err.into()),
    }
}
