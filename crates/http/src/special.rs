//! Особый порядок: каталог категорий и заявка заявителя (М12, FR-1201, п. 87–88).
//!
//! Категория выбирается из закрытого перечня п. 87 (INV-087): каталог отдает
//! ее требования - документы, срок проверки, льготную схему и публикуемость,
//! а FK не дает подать заявку по выдуманной категории. Заявка Прил. 3 живет
//! в кабинете заявителя вместе с вложениями и печатной формой; проверка
//! подразделением и решение Правления - задача T34.
//!
//! Права: заявку особого порядка подает тот же внешний заявитель, что и
//! заявку на тендер, поэтому используются действия заявок (A-066).

use std::collections::HashMap;

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use garde::Validate as _;
use object_store::ObjectStoreExt as _;
use object_store::PutPayload;
use object_store::path::Path as ObjectPath;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;
use tou_db::special::{self, FileRecord, RequestRecord, SpecialError};
use tou_domain::notification::NotificationKind;
use tou_domain::obligation::Term;
use tou_domain::policy::Action;
use tou_domain::special::{
    BoardDecision, Competition, Conclusion, SpecialCategory, SpecialDecision, SpecialRequestStatus,
};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::applications::ApplicantDetailsDto;
use crate::dto::ApplicantKindDto;
use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::pdf;
use crate::request::{Json, Multipart, Path, Query};
use crate::state::AppState;

const TEMPLATE: &str = include_str!("templates/special_request.typ");
const DECISION_TEMPLATE: &str = include_str!("templates/special_decision.typ");

fn special_error(err: SpecialError) -> ApiError {
    match err {
        SpecialError::NotFound => ApiError::NotFound,
        SpecialError::Rejected(reason) => ApiError::RuleViolation(reason),
        SpecialError::Db(db) => db.into(),
    }
}

/// Требуемый документ категории (п. 88)
#[derive(Debug, Serialize, ToSchema)]
pub struct CategoryDocumentDto {
    pub code: String,
    pub label_ru: String,
    pub label_kk: Option<String>,
    pub label_en: Option<String>,
    pub required: bool,
}

/// Категория особого порядка со своими требованиями (FR-1201, п. 87).
#[derive(Debug, Serialize, ToSchema)]
pub struct SpecialCategoryDto {
    pub code: String,
    /// Номер категории в п. 87 - им же ТЗ ее и называет (FR-1203)
    pub ordinal: i32,
    pub label_ru: String,
    pub label_kk: Option<String>,
    pub label_en: Option<String>,
    pub rule_ref: String,
    /// Срок проверки уполномоченным подразделением (FR-1202, п. 89)
    pub review_days: i32,
    /// Вид дней срока: `business` - рабочие, `calendar` - календарные
    pub review_term: String,
    /// Льготная схема категории (FR-1205); `none` - льгота не применяется
    pub benefit_scheme: String,
    /// Публикуются ли результаты по категории (FR-1403, п. 90, 97)
    pub publishable: bool,
    /// Что делать при двух и более заявках: `none` | `redirect` | `highest_amount`
    pub competition: String,
    /// Порог сопоставимости сумм инвестиций в процентах (п. 97)
    #[schema(value_type = String, example = "5.00")]
    pub comparable_margin_pct: Decimal,
    pub documents: Vec<CategoryDocumentDto>,
}

/// Каталог категорий особого порядка (FR-1201, п. 87): закрытый перечень
/// из тринадцати позиций и требования каждой.
#[utoipa::path(
    get,
    path = "/api/v1/refdata/special-categories",
    tag = "special",
    responses((status = 200, description = "Категории п. 87", body = [SpecialCategoryDto]))
)]
pub async fn special_categories(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<SpecialCategoryDto>>, ApiError> {
    let categories = special::list_categories(&state.db).await?;
    let mut documents: HashMap<String, Vec<CategoryDocumentDto>> = HashMap::new();
    for document in special::list_category_documents(&state.db).await? {
        documents
            .entry(document.category_code)
            .or_default()
            .push(CategoryDocumentDto {
                code: document.code,
                label_ru: document.label_ru,
                label_kk: document.label_kk,
                label_en: document.label_en,
                required: document.required,
            });
    }

    Ok(Json(
        categories
            .into_iter()
            .map(|category| SpecialCategoryDto {
                documents: documents.remove(&category.code).unwrap_or_default(),
                code: category.code,
                ordinal: category.ordinal,
                label_ru: category.label_ru,
                label_kk: category.label_kk,
                label_en: category.label_en,
                rule_ref: category.rule_ref,
                review_days: category.review_days,
                review_term: category.review_term,
                benefit_scheme: category.benefit_scheme,
                publishable: category.publishable,
                competition: category.competition,
                comparable_margin_pct: category.comparable_margin_pct,
            })
            .collect(),
    ))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SpecialRequestFileDto {
    pub id: Uuid,
    /// Позиция перечня категории, закрытая файлом (п. 88); null - прочий документ
    pub document_code: Option<String>,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub uploaded_at: OffsetDateTime,
}

impl From<FileRecord> for SpecialRequestFileDto {
    fn from(r: FileRecord) -> Self {
        Self {
            id: r.id,
            document_code: r.document_code,
            filename: r.filename,
            content_type: r.content_type,
            size_bytes: r.size_bytes,
            uploaded_at: r.uploaded_at,
        }
    }
}

/// Заявка особого порядка (Прил. 3, п. 88).
#[derive(Debug, Serialize, ToSchema)]
pub struct SpecialRequestDto {
    pub id: Uuid,
    pub applicant_id: Uuid,
    pub category: String,
    pub category_label: String,
    pub category_rule_ref: String,
    pub status: String,
    pub applicant_kind: ApplicantKindDto,
    /// Сведения Прил. 3 (персональные данные - NFR-07: в логи не попадают)
    #[schema(value_type = Object)]
    pub applicant_details: serde_json::Value,
    pub object_id: Option<Uuid>,
    pub object_name: Option<String>,
    /// Цель использования имущества - основание заявки (п. 88)
    pub purpose: String,
    pub requested_months: Option<i32>,
    /// Объем инвестиций (FR-1203, п. 97): им ранжируются конкурирующие заявки
    #[schema(value_type = Option<String>, example = "30000000.00")]
    pub investment_amount: Option<Decimal>,
    /// Тендер, созданный переводом вопроса в общий порядок (п. 86)
    pub tender_id: Option<Uuid>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub submitted_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub withdrawn_at: Option<OffsetDateTime>,
    pub files: Vec<SpecialRequestFileDto>,
}

impl SpecialRequestDto {
    fn from_record(r: RequestRecord, files: Vec<FileRecord>) -> Result<Self, ApiError> {
        Ok(Self {
            id: r.id,
            applicant_id: r.applicant_id,
            category: r.category,
            category_label: r.category_label,
            category_rule_ref: r.category_rule_ref,
            status: r.status,
            applicant_kind: ApplicantKindDto::from_db(&r.applicant_kind)?,
            // Граница слоя: DTO - ответ уполномоченному читателю (NFR-07)
            applicant_details: r.applicant_details.into_inner(),
            object_id: r.object_id,
            object_name: r.object_name,
            purpose: r.purpose,
            requested_months: r.requested_months,
            investment_amount: r.investment_amount,
            tender_id: r.tender_id,
            submitted_at: r.submitted_at,
            withdrawn_at: r.withdrawn_at,
            files: files.into_iter().map(SpecialRequestFileDto::from).collect(),
        })
    }
}

#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct SubmitSpecialRequest {
    /// Код категории п. 87 из каталога (INV-087)
    #[garde(skip)]
    pub category: String,
    #[garde(skip)]
    pub applicant_kind: ApplicantKindDto,
    #[garde(dive)]
    pub applicant_details: ApplicantDetailsDto,
    /// Объект имущества, если заявитель просит конкретное помещение
    #[garde(skip)]
    pub object_id: Option<Uuid>,
    /// Цель использования - существо заявки (п. 88)
    #[garde(length(chars, min = 1, max = 4000))]
    pub purpose: String,
    /// Испрашиваемый срок в месяцах
    #[garde(inner(range(min = 1, max = 240)))]
    pub requested_months: Option<i32>,
    /// Объем инвестиций - обязателен для инвестиционной категории (п. 97)
    #[garde(skip)]
    #[schema(value_type = Option<String>, example = "30000000.00")]
    pub investment_amount: Option<Decimal>,
}

/// Подача заявки особого порядка (FR-1201, Прил. 3): категория - только из
/// закрытого перечня п. 87, ее проверяет FK (INV-087).
#[utoipa::path(
    post,
    path = "/api/v1/special-requests",
    tag = "special",
    request_body = SubmitSpecialRequest,
    responses(
        (status = 201, description = "Заявка подана", body = SpecialRequestDto),
        (status = 409, description = "Категория вне перечня п. 87 (INV-087)", body = crate::error::Problem),
        (status = 422, description = "Данные не прошли проверку", body = crate::error::Problem),
    )
)]
pub async fn submit_special_request(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<SubmitSpecialRequest>,
) -> Result<(StatusCode, Json<SpecialRequestDto>), ApiError> {
    user.require(Action::ApplicationSubmit)?;
    body.validate()
        .map_err(|r| ApiError::Validation(r.to_string()))?;

    // Категория закрыта enum'ом домена: значение вне перечня п. 87 до базы
    // не доходит, а FK ловит рассинхрон каталога и домена
    let category: SpecialCategory = body
        .category
        .parse()
        .map_err(|_| ApiError::Validation("категория вне перечня п. 87 (FR-1201)".to_owned()))?;

    let details = serde_json::to_value(&body.applicant_details).map_err(ApiError::internal)?;
    let record = special::submit(
        &state.db,
        user.id(),
        special::NewRequest {
            category: category.as_str(),
            applicant_kind: body.applicant_kind.as_db(),
            applicant_details: &details,
            object_id: body.object_id,
            purpose: body.purpose.trim(),
            requested_months: body.requested_months,
            investment_amount: body.investment_amount,
        },
    )
    .await
    .map_err(special_error)?;

    Ok((
        StatusCode::CREATED,
        Json(SpecialRequestDto::from_record(record, Vec::new())?),
    ))
}

/// Мои заявки особого порядка (кабинет заявителя).
#[utoipa::path(
    get,
    path = "/api/v1/special-requests/my",
    tag = "special",
    responses((status = 200, description = "Заявки заявителя", body = [SpecialRequestDto]))
)]
pub async fn my_special_requests(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<SpecialRequestDto>>, ApiError> {
    user.require(Action::ApplicationReadOwn)?;

    let records = special::list_own(&state.db, user.id()).await?;
    let ids: Vec<Uuid> = records.iter().map(|r| r.id).collect();
    let mut by_request: HashMap<Uuid, Vec<FileRecord>> = HashMap::new();
    for file in special::files_for(&state.db, &ids).await? {
        by_request
            .entry(file.special_request_id)
            .or_default()
            .push(file);
    }

    records
        .into_iter()
        .map(|record| {
            let files = by_request.remove(&record.id).unwrap_or_default();
            SpecialRequestDto::from_record(record, files)
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
}

/// Заявка особого порядка: своя - заявителю, чужие - Правлению (п. 90).
#[utoipa::path(
    get,
    path = "/api/v1/special-requests/{id}",
    tag = "special",
    params(("id" = Uuid, Path, description = "Заявка особого порядка")),
    responses(
        (status = 200, description = "Заявка", body = SpecialRequestDto),
        (status = 404, description = "Заявка не найдена", body = crate::error::Problem),
    )
)]
pub async fn get_special_request(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<SpecialRequestDto>, ApiError> {
    let record = load_visible(&state, &user, id).await?;
    let files = special::list_files(&state.db, record.id).await?;
    Ok(Json(SpecialRequestDto::from_record(record, files)?))
}

/// Отзыв заявки заявителем, пока решение не принято (п. 88–90).
#[utoipa::path(
    post,
    path = "/api/v1/special-requests/{id}/withdraw",
    tag = "special",
    params(("id" = Uuid, Path, description = "Заявка особого порядка")),
    responses(
        (status = 200, description = "Заявка отозвана", body = SpecialRequestDto),
        (status = 404, description = "Заявка не найдена или решение уже принято", body = crate::error::Problem),
    )
)]
pub async fn withdraw_special_request(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<SpecialRequestDto>, ApiError> {
    user.require(Action::ApplicationWithdraw)?;

    let record = special::withdraw(&state.db, user.id(), id)
        .await
        .map_err(special_error)?;
    let files = special::list_files(&state.db, record.id).await?;
    Ok(Json(SpecialRequestDto::from_record(record, files)?))
}

/// Вложение к своей заявке (п. 88): позиция перечня категории закрывается
/// файлом; принадлежность позиции категории проверяет триггер БД.
#[utoipa::path(
    post,
    path = "/api/v1/special-requests/{id}/files",
    tag = "special",
    params(("id" = Uuid, Path, description = "Заявка особого порядка"), DocumentQuery),
    request_body(content = Vec<u8>, content_type = "multipart/form-data",
        description = "Часть `file` с документом"),
    responses(
        (status = 201, description = "Документ сохранен", body = SpecialRequestFileDto),
        (status = 409, description = "Заявка не принимает документы", body = crate::error::Problem),
        (status = 422, description = "Часть `file` отсутствует", body = crate::error::Problem),
    )
)]
pub async fn upload_special_file(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<DocumentQuery>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<SpecialRequestFileDto>), ApiError> {
    user.require(Action::ApplicationSubmit)?;

    let mut part = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::Validation(e.to_string()))?
    {
        if field.name() == Some("file") {
            let filename = field
                .file_name()
                .unwrap_or("document")
                .chars()
                .take(255)
                .collect::<String>();
            let content_type = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_owned();
            let bytes = field
                .bytes()
                .await
                .map_err(|e| ApiError::Validation(e.to_string()))?;
            part = Some((filename, content_type, bytes));
            break;
        }
    }
    let (filename, content_type, bytes) =
        part.ok_or_else(|| ApiError::Validation("часть 'file' отсутствует".into()))?;

    let file_key = format!("special-requests/{id}/{}", Uuid::now_v7());
    let object_path = ObjectPath::from(file_key.as_str());
    let size_bytes = i64::try_from(bytes.len()).map_err(ApiError::internal)?;

    state
        .storage
        .put(&object_path, PutPayload::from_bytes(bytes))
        .await
        .map_err(ApiError::internal)?;

    let inserted = special::add_file(
        &state.db,
        user.id(),
        special::NewFile {
            request_id: id,
            document_code: query.document_code.as_deref(),
            file_key: &file_key,
            filename: &filename,
            content_type: &content_type,
            size_bytes,
        },
    )
    .await
    .map_err(special_error);

    match inserted {
        Ok(Some(record)) => Ok((StatusCode::CREATED, Json(record.into()))),
        other => {
            // Метаданные не записаны - объект остается в бакете сиротой:
            // удалять из досье приложение не вправе (INV-042, Object Lock),
            // а без строки метаданных объект недостижим (см. applications.rs)
            tracing::warn!(%file_key, "документ загружен, метаданные отклонены");
            match other {
                Err(err) => Err(err),
                _ => Err(ApiError::RuleViolation(
                    "документ прикладывается к своей заявке до решения (п. 88)".into(),
                )),
            }
        }
    }
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct DocumentQuery {
    /// Позиция перечня документов категории (п. 88)
    pub document_code: Option<String>,
}

/// Скачивание документа заявки: заявителю - свои, Правлению - по любой заявке.
#[utoipa::path(
    get,
    path = "/api/v1/special-requests/{id}/files/{file_id}",
    tag = "special",
    params(
        ("id" = Uuid, Path, description = "Заявка особого порядка"),
        ("file_id" = Uuid, Path, description = "Документ"),
    ),
    responses(
        (status = 200, description = "Содержимое документа", content_type = "application/octet-stream", body = Vec<u8>),
        (status = 404, description = "Нет документа или нет доступа", body = crate::error::Problem),
    )
)]
pub async fn download_special_file(
    user: CurrentUser,
    State(state): State<AppState>,
    Path((id, file_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, ApiError> {
    load_visible(&state, &user, id).await?;
    let file = special::get_file(&state.db, file_id)
        .await?
        .filter(|f| f.special_request_id == id)
        .ok_or(ApiError::NotFound)?;

    let object = state
        .storage
        .get(&ObjectPath::from(file.file_key.as_str()))
        .await
        .map_err(ApiError::internal)?;
    let bytes = object.bytes().await.map_err(ApiError::internal)?;

    Ok((
        [
            (header::CONTENT_TYPE, file.content_type.clone()),
            (
                header::CONTENT_DISPOSITION,
                format!(
                    "attachment; filename=\"{}\"",
                    sanitize_filename(&file.filename)
                ),
            ),
        ],
        bytes.to_vec(),
    )
        .into_response())
}

/// Печатная форма заявки (Прил. 3): та же заявка на бумаге для подписи.
#[utoipa::path(
    get,
    path = "/api/v1/special-requests/{id}/application.pdf",
    tag = "special",
    params(("id" = Uuid, Path, description = "Заявка особого порядка")),
    responses(
        (status = 200, description = "PDF заявки по форме Прил. 3",
         content_type = "application/pdf", body = Vec<u8>),
        (status = 404, description = "Заявка не найдена", body = crate::error::Problem),
    )
)]
pub async fn special_request_pdf(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let record = load_visible(&state, &user, id).await?;
    let files = special::list_files(&state.db, record.id).await?;
    let categories = special::list_categories(&state.db).await?;
    let review_term = categories
        .iter()
        .find(|category| category.code == record.category)
        .and_then(|category| {
            u32::try_from(category.review_days)
                .ok()
                .and_then(|days| Term::from_parts(days, &category.review_term))
        });

    let data = request_data(&record, &files, review_term);
    let pdf_bytes = tokio::task::spawn_blocking(move || pdf::render(TEMPLATE, &data))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/pdf".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("inline; filename=\"special-request-{id}.pdf\""),
            ),
        ],
        pdf_bytes,
    )
        .into_response())
}

/// Заключение уполномоченного подразделения (FR-1202, п. 89).
#[derive(Debug, Serialize, ToSchema)]
pub struct SpecialReviewDto {
    pub id: Uuid,
    pub special_request_id: Uuid,
    pub reviewer_name: Option<String>,
    pub conclusion: String,
    /// Вывод подразделения: `grant` | `refuse` | `redirect`
    pub recommendation: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}

/// Решение Правления по заявке особого порядка (FR-1202, п. 90).
#[derive(Debug, Serialize, ToSchema)]
pub struct SpecialDecisionDto {
    pub id: Uuid,
    pub special_request_id: Uuid,
    /// `grant` | `refuse` | `redirect` (закрытый перечень п. 90)
    pub decision: String,
    /// Обоснование - публикуется вместе с результатом (п. 97, FR-1403)
    pub rationale: String,
    pub decided_by_name: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub decided_at: OffsetDateTime,
    /// Протокол решения сформирован (печатная форма доступна)
    pub has_pdf: bool,
}

/// Ход рассмотрения заявки: заключение подразделения и решение Правления.
#[derive(Debug, Serialize, ToSchema)]
pub struct SpecialProgressDto {
    pub review: Option<SpecialReviewDto>,
    pub decision: Option<SpecialDecisionDto>,
}

fn review_dto(record: tou_db::special::ReviewRecord) -> SpecialReviewDto {
    SpecialReviewDto {
        id: record.id,
        special_request_id: record.special_request_id,
        reviewer_name: record.reviewer_name,
        conclusion: record.conclusion,
        recommendation: record.recommendation,
        created_at: record.created_at,
    }
}

fn decision_dto(record: tou_db::special::DecisionRecord) -> SpecialDecisionDto {
    SpecialDecisionDto {
        id: record.id,
        special_request_id: record.special_request_id,
        decision: record.decision,
        rationale: record.rationale,
        decided_by_name: record.decided_by_name,
        decided_at: record.decided_at,
        has_pdf: record.pdf_key.is_some(),
    }
}

/// Конкуренция вокруг заявки (FR-1203, п. 86, 97).
#[derive(Debug, Serialize, ToSchema)]
pub struct CompetitionDto {
    /// Правило категории: `none` | `redirect` | `highest_amount`
    pub rule: String,
    /// Активные конкурирующие заявки на тот же объект
    pub rivals: usize,
    #[schema(value_type = Option<String>, example = "30000000.00")]
    pub best_rival_amount: Option<Decimal>,
    #[schema(value_type = Option<String>, example = "30000000.00")]
    pub own_amount: Option<Decimal>,
    /// Суммы конкурентов сопоставимы - выбор за Правлением (п. 97)
    pub amounts_comparable: bool,
    /// Решения, доступные Правлению при этой конкуренции (INV-086)
    pub permitted_decisions: Vec<String>,
}

fn competition_dto(competition: Competition) -> CompetitionDto {
    CompetitionDto {
        rule: competition.rule.as_str().to_owned(),
        rivals: competition.rivals,
        best_rival_amount: competition.best_rival_amount,
        own_amount: competition.own_amount,
        amounts_comparable: competition.amounts_comparable(),
        permitted_decisions: SpecialDecision::ALL
            .into_iter()
            .filter(|decision| competition.blocks(*decision).is_none())
            .map(|decision| decision.as_str().to_owned())
            .collect(),
    }
}

/// Конкуренция заявок (FR-1203, п. 86, 97): сколько заявок спорит за объект
/// и какие решения из-за этого доступны Правлению.
#[utoipa::path(
    get,
    path = "/api/v1/special-requests/{id}/competition",
    tag = "special",
    params(("id" = Uuid, Path, description = "Заявка особого порядка")),
    responses(
        (status = 200, description = "Конкуренция вокруг заявки", body = CompetitionDto),
        (status = 404, description = "Заявка не найдена", body = crate::error::Problem),
    )
)]
pub async fn special_competition(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<CompetitionDto>, ApiError> {
    load_visible(&state, &user, id).await?;

    let (competition, _) = special::competition(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(competition_dto(competition)))
}

/// Ход рассмотрения заявки (FR-1202): заключение и решение, если они есть.
#[utoipa::path(
    get,
    path = "/api/v1/special-requests/{id}/progress",
    tag = "special",
    params(("id" = Uuid, Path, description = "Заявка особого порядка")),
    responses(
        (status = 200, description = "Заключение и решение", body = SpecialProgressDto),
        (status = 404, description = "Заявка не найдена", body = crate::error::Problem),
    )
)]
pub async fn special_progress(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<SpecialProgressDto>, ApiError> {
    load_visible(&state, &user, id).await?;

    Ok(Json(SpecialProgressDto {
        review: special::review_of(&state.db, id).await?.map(review_dto),
        decision: special::decision_of(&state.db, id).await?.map(decision_dto),
    }))
}

#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct SpecialReviewRequest {
    /// Заключение подразделения - текст (п. 89)
    #[garde(length(chars, min = 1, max = 8000))]
    pub conclusion: String,
    /// Вывод из того же перечня, что и решение Правления (п. 90)
    #[garde(skip)]
    pub recommendation: String,
}

/// Заключение уполномоченного подразделения (FR-1202, п. 89): выносит заявку
/// на рассмотрение Правления, закрывает срок проверки и ставит срок решения
/// (10 рабочих дней, п. 90).
#[utoipa::path(
    post,
    path = "/api/v1/special-requests/{id}/review",
    tag = "special",
    params(("id" = Uuid, Path, description = "Заявка особого порядка")),
    request_body = SpecialReviewRequest,
    responses(
        (status = 201, description = "Заключение вынесено", body = SpecialReviewDto),
        (status = 409, description = "Заявка не в состоянии «подана» либо заключение уже есть", body = crate::error::Problem),
        (status = 422, description = "Данные не прошли проверку", body = crate::error::Problem),
    )
)]
pub async fn review_special_request(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<SpecialReviewRequest>,
) -> Result<(StatusCode, Json<SpecialReviewDto>), ApiError> {
    // Проверку ведет уполномоченное подразделение - в модели ролей
    // организатор (юридическая служба, п. 2.4; A-068)
    user.require(Action::TenderManage)?;
    body.validate()
        .map_err(|r| ApiError::Validation(r.to_string()))?;

    let recommendation: SpecialDecision = body.recommendation.parse().map_err(|_| {
        ApiError::Validation("вывод заключения вне перечня п. 90 (FR-1202)".to_owned())
    })?;

    let record = special::review(
        &state.db,
        user.id(),
        id,
        body.conclusion.trim(),
        recommendation.as_str(),
    )
    .await
    .map_err(special_error)?;

    Ok((StatusCode::CREATED, Json(review_dto(record))))
}

#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct SpecialDecideRequest {
    /// Решение из закрытого перечня п. 90
    #[garde(skip)]
    pub decision: String,
    /// Обоснование решения - публикуется (п. 97)
    #[garde(length(chars, min = 1, max = 8000))]
    pub rationale: String,
}

/// Решение Правления (FR-1202, п. 90): INV-090 - без заключения
/// подразделения решения не существует. Заявитель извещается (FR-1301),
/// протокол решения сохраняется печатной формой.
#[utoipa::path(
    post,
    path = "/api/v1/special-requests/{id}/decision",
    tag = "special",
    params(("id" = Uuid, Path, description = "Заявка особого порядка")),
    request_body = SpecialDecideRequest,
    responses(
        (status = 201, description = "Решение принято", body = SpecialDecisionDto),
        (status = 409, description = "Нет заключения (INV-090) либо решение уже принято", body = crate::error::Problem),
        (status = 422, description = "Данные не прошли проверку", body = crate::error::Problem),
    )
)]
pub async fn decide_special_request(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<SpecialDecideRequest>,
) -> Result<(StatusCode, Json<SpecialDecisionDto>), ApiError> {
    user.require(Action::BoardDecide)?;
    body.validate()
        .map_err(|r| ApiError::Validation(r.to_string()))?;

    let choice: SpecialDecision = body
        .decision
        .parse()
        .map_err(|_| ApiError::Validation("решение вне перечня п. 90 (FR-1202)".to_owned()))?;

    // INV-090 в домене: без заключения решение не построить. Тот же отказ
    // выдаст триггер БД - здесь он превращается в понятный problem+json.
    let request = special::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let status: SpecialRequestStatus = request
        .status
        .parse()
        .map_err(|_| ApiError::internal(std::io::Error::other("состояние заявки")))?;
    let conclusion = special::review_of(&state.db, id)
        .await?
        .map(|review| {
            review
                .recommendation
                .parse::<SpecialDecision>()
                .map(|recommendation| Conclusion {
                    recommendation,
                    request_status: status,
                })
        })
        .transpose()
        .map_err(|_| ApiError::internal(std::io::Error::other("вывод заключения")))?;
    // INV-086: конкуренция заявок закрывает «предоставить» (п. 86, 97) -
    // тот же отказ выдаст триггер БД
    let (competition, _) = special::competition(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    BoardDecision::take(conclusion, status, competition, choice)
        .map_err(|err| ApiError::RuleViolation(err.to_string()))?;

    let record = special::decide(
        &state.db,
        user.id(),
        id,
        choice.as_str(),
        body.rationale.trim(),
    )
    .await
    .map_err(special_error)?;

    // Перевод в общий порядок (FR-1203, п. 86): вопрос один - тендер один.
    // Черновик создается сразу, конкурирующие заявки уходят в него вместе
    // и получают то же решение с тем же обоснованием.
    if choice == SpecialDecision::Redirect {
        let request = special::get(&state.db, id)
            .await?
            .ok_or(ApiError::NotFound)?;
        let title = format!(
            "Особый порядок → общий порядок: {} ({})",
            request
                .object_name
                .as_deref()
                .unwrap_or(request.category_label.as_str()),
            request.category_rule_ref
        );
        special::redirect_to_tender(&state.db, user.id(), id, &title)
            .await
            .map_err(special_error)?;
        special::redirect_competitors(&state.db, user.id(), id, body.rationale.trim())
            .await
            .map_err(special_error)?;
    }

    // Протокол решения (п. 90) - снимок на момент решения
    let request = special::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let review = special::review_of(&state.db, id).await?;
    let data = decision_data(&request, review.as_ref(), &record);
    let pdf_bytes = tokio::task::spawn_blocking(move || pdf::render(DECISION_TEMPLATE, &data))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;

    let pdf_key = format!("special-requests/{id}/decision.pdf");
    state
        .storage
        .put(
            &ObjectPath::from(pdf_key.as_str()),
            PutPayload::from(pdf_bytes),
        )
        .await
        .map_err(ApiError::internal)?;
    special::attach_decision_pdf(&state.db, user.id(), record.id, &pdf_key)
        .await
        .map_err(special_error)?;

    // Извещение заявителя (FR-1301, п. 90): решение и его обоснование
    tou_db::notifications::insert(
        &state.db,
        user.id(),
        NotificationKind::SpecialDecided.as_str(),
        &[tou_db::notifications::NewNotification {
            user_id: request.applicant_id,
            payload: json!({
                "special_request_id": id,
                "category": request.category_label,
                "decision": choice.as_str(),
                "rationale": record.rationale,
                "rule_ref": "п. 90",
            }),
        }],
    )
    .await?;

    let record = special::decision_of(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok((StatusCode::CREATED, Json(decision_dto(record))))
}

/// Печатная форма протокола решения Правления (п. 90) из RustFS.
#[utoipa::path(
    get,
    path = "/api/v1/special-requests/{id}/decision.pdf",
    tag = "special",
    params(("id" = Uuid, Path, description = "Заявка особого порядка")),
    responses(
        (status = 200, description = "PDF протокола решения", content_type = "application/pdf"),
        (status = 404, description = "Решение не принято", body = crate::error::Problem),
    )
)]
pub async fn special_decision_pdf(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    load_visible(&state, &user, id).await?;

    let decision = special::decision_of(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let pdf_key = decision.pdf_key.ok_or(ApiError::NotFound)?;

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
                format!("inline; filename=\"special-request-{id}-decision.pdf\""),
            ),
        ],
        bytes,
    )
        .into_response())
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct SpecialListParams {
    /// Состояние заявок; по умолчанию - те, что в рассмотрении (п. 89–90)
    pub status: Option<String>,
}

/// Рабочий список заявок (FR-1202): по умолчанию ожидающие заключения и
/// решения, с фильтром - например удовлетворенные, по которым заключается
/// инвестиционный договор (FR-1204).
#[utoipa::path(
    get,
    path = "/api/v1/special-requests",
    tag = "special",
    params(SpecialListParams),
    responses(
        (status = 200, description = "Заявки", body = [SpecialRequestDto]),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
    )
)]
pub async fn pending_special_requests(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(params): Query<SpecialListParams>,
) -> Result<Json<Vec<SpecialRequestDto>>, ApiError> {
    // Список видят те, кто ведет рассмотрение: подразделение и Правление
    if user.require(Action::BoardDecide).is_err() {
        user.require(Action::TenderManage)?;
    }

    let statuses: Vec<&str> = match params.status.as_deref() {
        None | Some("") => vec![
            SpecialRequestStatus::Submitted.as_str(),
            SpecialRequestStatus::UnderReview.as_str(),
        ],
        Some(raw) => {
            let status: SpecialRequestStatus = raw.parse().map_err(|_| {
                ApiError::bad_request(format!("неизвестное состояние заявки: {raw}"))
            })?;
            vec![status.as_str()]
        }
    };

    let records = special::list_by_status(&state.db, &statuses).await?;
    records
        .into_iter()
        .map(|record| SpecialRequestDto::from_record(record, Vec::new()))
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
}

/// Заявка видна заявителю и Правлению (п. 90): чужую заявку посторонний
/// не отличает от несуществующей.
async fn load_visible(
    state: &AppState,
    user: &CurrentUser,
    id: Uuid,
) -> Result<RequestRecord, ApiError> {
    let record = special::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let handles_review =
        user.require(Action::BoardDecide).is_ok() || user.require(Action::TenderManage).is_ok();
    if record.applicant_id != user.id() && !handles_review {
        return Err(ApiError::NotFound);
    }
    Ok(record)
}

/// Данные протокола решения (п. 90): значения предформатированы (ru, NFR-01).
fn decision_data(
    request: &RequestRecord,
    review: Option<&tou_db::special::ReviewRecord>,
    decision: &tou_db::special::DecisionRecord,
) -> serde_json::Value {
    let applicant = request
        .applicant_details
        .expose()
        .get("name")
        .and_then(|value| value.as_str())
        .map(str::to_owned)
        .or_else(|| request.applicant_name.clone())
        .unwrap_or_else(|| "-".to_owned());
    let outcome = decision
        .decision
        .parse::<SpecialDecision>()
        .map(decision_ru)
        .unwrap_or("решение не распознано");

    json!({
        "number": request.id.simple().to_string()[..8].to_uppercase(),
        "decided_at": crate::admission::format_almaty(Some(decision.decided_at)),
        "submitted_at": crate::admission::format_almaty(Some(request.submitted_at)),
        "category": format!("{} ({})", request.category_label, request.category_rule_ref),
        "applicant": applicant,
        "object": request.object_name.clone().unwrap_or_else(|| "не указан".to_owned()),
        "purpose": request.purpose,
        "conclusion": review.map(|r| r.conclusion.clone())
            .unwrap_or_else(|| "заключение отсутствует".to_owned()),
        "recommendation": review
            .and_then(|r| r.recommendation.parse::<SpecialDecision>().ok())
            .map(decision_ru)
            .unwrap_or("-"),
        "decision": outcome,
        "rationale": decision.rationale,
        "decided_by": decision.decided_by_name.clone().unwrap_or_else(|| "-".to_owned()),
    })
}

fn decision_ru(decision: SpecialDecision) -> &'static str {
    match decision {
        SpecialDecision::Grant => "предоставить имущество в особом порядке",
        SpecialDecision::Refuse => "отказать в предоставлении",
        SpecialDecision::Redirect => "направить в общий порядок",
    }
}

/// Данные печатной формы: значения предформатированы сервером (ru, NFR-01).
fn request_data(
    record: &RequestRecord,
    files: &[FileRecord],
    review_term: Option<Term>,
) -> serde_json::Value {
    let details = |field: &str| {
        record
            .applicant_details
            .expose()
            .get(field)
            .and_then(|value| value.as_str())
            .unwrap_or("-")
            .to_owned()
    };
    let status = record
        .status
        .parse::<SpecialRequestStatus>()
        .map(status_ru)
        .unwrap_or("состояние неизвестно");

    json!({
        "number": record.id.simple().to_string()[..8].to_uppercase(),
        "submitted_at": crate::admission::format_almaty(Some(record.submitted_at)),
        "category": format!("{} ({})", record.category_label, record.category_rule_ref),
        "status": status,
        "applicant_name": details("name"),
        "applicant_id_number": details("id_number"),
        "applicant_address": details("address"),
        "applicant_phone": details("phone"),
        "applicant_email": details("email"),
        "object": record.object_name.clone().unwrap_or_else(|| "не указан".to_owned()),
        "purpose": record.purpose,
        "requested_months": record.requested_months
            .map(|months| months.to_string())
            .unwrap_or_else(|| "не указан".to_owned()),
        "review_term": review_term.map(term_ru).unwrap_or_else(|| "срок уточняется".to_owned()),
        "documents": files.iter().map(|file| json!({
            "filename": file.filename,
            "document_code": file.document_code.clone().unwrap_or_else(|| "-".to_owned()),
        })).collect::<Vec<_>>(),
    })
}

fn status_ru(status: SpecialRequestStatus) -> &'static str {
    match status {
        SpecialRequestStatus::Submitted => "подана",
        SpecialRequestStatus::UnderReview => "на рассмотрении Правления",
        SpecialRequestStatus::Granted => "предоставлено решением Правления",
        SpecialRequestStatus::Refused => "отказано решением Правления",
        SpecialRequestStatus::Redirected => "направлено в общий порядок",
        SpecialRequestStatus::Withdrawn => "отозвана заявителем",
    }
}

fn term_ru(term: Term) -> String {
    match term {
        Term::BusinessDays(days) => format!("{days} рабочих дней"),
        Term::CalendarDays(days) => format!("{days} календарных дней"),
    }
}

/// Заголовок не должен ломаться кавычками/переводами строк в имени файла.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c == '"' || c.is_control() { '_' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Печатная форма Прил. 3 (FR-1201) компилируется на снимке заявки;
    /// спецсимволы Typst приходят данными и разметку не ломают.
    #[test]
    fn special_request_renders_pdf() {
        let data = json!({
            "number": "0TEST",
            "submitted_at": "07.08.2026 10:15",
            "category": "Категория № 4 (п. 87.4)",
            "status": "подана",
            "applicant_name": "ТОО «Тест» *#_$@",
            "applicant_id_number": "123456789012",
            "applicant_address": "г. Павлодар, ул. Ломова, 64",
            "applicant_phone": "+7 700 000 00 00",
            "applicant_email": "test@tou.test",
            "object": "Корпус А, каб. 101",
            "purpose": "размещение оборудования, используемого в образовательном процессе",
            "requested_months": "12",
            "review_term": "15 календарных дней",
            "documents": [{"filename": "смета.pdf", "document_code": "documents_pending"}],
        });

        let bytes = pdf::render(TEMPLATE, &data).expect("печатная форма Прил. 3");
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 1_000);
    }

    /// Пустой перечень вложений печатается без таблицы (заявка без документов).
    #[test]
    fn special_request_without_documents_renders_pdf() {
        let data = json!({
            "number": "0TEST",
            "submitted_at": "07.08.2026 10:15",
            "category": "Категория № 1 (п. 87.1)",
            "status": "отозвана заявителем",
            "applicant_name": "Иванов И.И.",
            "applicant_id_number": "-",
            "applicant_address": "-",
            "applicant_phone": "-",
            "applicant_email": "-",
            "object": "не указан",
            "purpose": "проведение мероприятия",
            "requested_months": "не указан",
            "review_term": "срок уточняется",
            "documents": [],
        });

        let bytes = pdf::render(TEMPLATE, &data).expect("печатная форма без вложений");
        assert!(bytes.starts_with(b"%PDF"));
    }

    /// Протокол решения Правления (п. 90) компилируется на снимке решения.
    #[test]
    fn special_decision_renders_pdf() {
        let data = json!({
            "number": "0TEST",
            "decided_at": "21.08.2026 15:00",
            "submitted_at": "07.08.2026 10:15",
            "category": "Категория № 4 (п. 87.4)",
            "applicant": "ТОО «Тест» *#_$@",
            "object": "Корпус А, каб. 101",
            "purpose": "размещение оборудования",
            "conclusion": "Заявка соответствует требованиям категории.",
            "recommendation": "предоставить имущество в особом порядке",
            "decision": "направить в общий порядок",
            "rationale": "По категории поданы две заявки (п. 86).",
            "decided_by": "Председатель Правления",
        });

        let bytes = pdf::render(DECISION_TEMPLATE, &data).expect("протокол решения");
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 1_000);
    }

    /// Решения печатаются по-русски - без «решение не распознано».
    #[test]
    fn every_decision_has_a_russian_label() {
        for decision in SpecialDecision::ALL {
            assert!(
                !decision_ru(decision).is_empty(),
                "{decision:?} без подписи"
            );
        }
    }

    /// Состояния заявки печатаются по-русски - без «состояние неизвестно».
    #[test]
    fn every_status_has_a_russian_label() {
        for status in SpecialRequestStatus::ALL {
            assert!(!status_ru(status).is_empty(), "{status:?} без подписи");
        }
    }
}
