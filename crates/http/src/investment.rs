//! Инвестиционные договоры особого порядка (М12, FR-1204, п. 91–94).
//!
//! Договор составляется по удовлетворенной заявке инвестиционной категории,
//! комплект приложений п. 91 закрывает позиции закрытого перечня (INV-091),
//! срок ограничен семью годами (INV-094), приемка инвестиций идет актами
//! (п. 92), а продление - только при полном исполнении (п. 93).
//!
//! Права: договор ведет организатор (юридическая служба, п. 2.4), продление
//! оформляет Правление - оно же принимало решение по заявке (A-072).

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
use time::{Date, OffsetDateTime};
use tou_db::investment::{self, AcceptanceRecord, InvestmentError, InvestmentRecord};
use tou_domain::investment::{Attachment, Extension, Extensions, Progress, Term};
use tou_domain::policy::Action;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::dto::iso_date;
use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::pdf;
use crate::request::{Json, Multipart, Path};
use crate::state::AppState;

const CONTRACT_TEMPLATE: &str = include_str!("templates/investment_contract.typ");
const ACT_TEMPLATE: &str = include_str!("templates/investment_act.typ");

fn investment_error(err: InvestmentError) -> ApiError {
    match err {
        InvestmentError::NotFound => ApiError::NotFound,
        InvestmentError::Rejected(reason) => ApiError::RuleViolation(reason),
        InvestmentError::Db(db) => db.into(),
    }
}

/// Обязательное приложение проекта (п. 91)
#[derive(Debug, Serialize, ToSchema)]
pub struct InvestmentAttachmentDto {
    pub code: String,
    pub ordinal: i32,
    pub label_ru: String,
    pub label_kk: Option<String>,
    pub label_en: Option<String>,
    pub rule_ref: String,
}

/// Закрытый перечень приложений инвестиционного проекта (FR-1204, п. 91).
#[utoipa::path(
    get,
    path = "/api/v1/refdata/investment-attachments",
    tag = "investment",
    responses((status = 200, description = "Приложения п. 91", body = [InvestmentAttachmentDto]))
)]
pub async fn investment_attachments(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<InvestmentAttachmentDto>>, ApiError> {
    let rows = investment::list_attachments(&state.db).await?;
    Ok(Json(
        rows.into_iter()
            .map(|row| InvestmentAttachmentDto {
                code: row.code,
                ordinal: row.ordinal,
                label_ru: row.label_ru,
                label_kk: row.label_kk,
                label_en: row.label_en,
                rule_ref: row.rule_ref,
            })
            .collect(),
    ))
}

/// Инвестиционный договор (FR-1204, п. 91–94).
#[derive(Debug, Serialize, ToSchema)]
pub struct InvestmentContractDto {
    pub id: Uuid,
    pub contract_id: Uuid,
    pub special_request_id: Uuid,
    #[schema(value_type = String, example = "30000000.00")]
    pub investment_amount: Decimal,
    /// Принято актами приемки (п. 92)
    #[schema(value_type = String, example = "30000000.00")]
    pub accepted_amount: Decimal,
    /// Обязательства исполнены полностью - продление возможно (п. 93)
    pub performance_complete: bool,
    pub term_months: i32,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub extended_at: Option<OffsetDateTime>,
    pub extension_months: Option<i32>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub prolonged_at: Option<OffsetDateTime>,
    pub prolongation_months: Option<i32>,
    pub contract_status: String,
    pub object_name: Option<String>,
    pub tenant_name: Option<String>,
    #[schema(value_type = String, example = "150000.00")]
    pub monthly_rate: Decimal,
    /// Снимок расчета ставки Прил. 4 - публикуемое обоснование (FR-1403, п. 97)
    #[schema(value_type = Option<Object>)]
    pub rate_calculation: Option<serde_json::Value>,
    /// Приложенные позиции перечня п. 91
    pub attachments: Vec<String>,
    /// Недостающие позиции: без них договор не подписывается (INV-091)
    pub missing_attachments: Vec<String>,
    /// Способы продления, доступные сейчас (п. 93)
    pub permitted_extensions: Vec<String>,
}

fn contract_dto(record: InvestmentRecord) -> InvestmentContractDto {
    let progress = Progress {
        promised: record.investment_amount,
        accepted: record.accepted_amount,
    };
    let extensions = Extensions {
        extended: record.extended_at.is_some(),
        prolonged: record.prolonged_at.is_some(),
    };
    let missing: Vec<String> = Attachment::ALL
        .into_iter()
        .filter(|attachment| {
            !record
                .attachments
                .iter()
                .any(|code| code == attachment.as_str())
        })
        .map(|attachment| attachment.as_str().to_owned())
        .collect();

    InvestmentContractDto {
        id: record.id,
        contract_id: record.contract_id,
        special_request_id: record.special_request_id,
        investment_amount: record.investment_amount,
        accepted_amount: record.accepted_amount,
        performance_complete: progress.is_complete(),
        term_months: record.term_months,
        extended_at: record.extended_at,
        extension_months: record.extension_months,
        prolonged_at: record.prolonged_at,
        prolongation_months: record.prolongation_months,
        contract_status: record.contract_status,
        object_name: record.object_name,
        tenant_name: record.tenant_name,
        monthly_rate: record.monthly_rate,
        rate_calculation: record.rate_calculation,
        attachments: record.attachments,
        missing_attachments: missing,
        permitted_extensions: Extension::ALL
            .into_iter()
            .filter(|extension| extensions.allow(*extension, progress).is_ok())
            .map(|extension| extension.as_str().to_owned())
            .collect(),
    }
}

/// Инвестиционные договоры (FR-1204): список ведет организатор, Правление
/// видит его же - продление оформляет оно.
#[utoipa::path(
    get,
    path = "/api/v1/investment-contracts",
    tag = "investment",
    responses(
        (status = 200, description = "Инвестиционные договоры", body = [InvestmentContractDto]),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
    )
)]
pub async fn list_investment_contracts(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<InvestmentContractDto>>, ApiError> {
    require_investment_access(&user)?;
    let rows = investment::list(&state.db).await?;
    Ok(Json(rows.into_iter().map(contract_dto).collect()))
}

#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct DraftInvestmentRequest {
    #[garde(skip)]
    pub special_request_id: Uuid,
    /// Опции коэффициентов Прил. 4: ставку и ее обоснование считает сервер
    /// (FR-201, FR-1403) - тем же снимком, что и у лота тендера
    #[garde(skip)]
    #[serde(default)]
    pub rate_options: crate::rates::RateOptionsDto,
    /// Срок договора в месяцах - не более семи лет (INV-094)
    #[garde(skip)]
    pub term_months: i32,
}

fn positive(value: &Decimal, _ctx: &()) -> garde::Result {
    if *value > Decimal::ZERO {
        Ok(())
    } else {
        Err(garde::Error::new("значение должно быть > 0"))
    }
}

/// Составление инвестиционного договора (FR-1204, п. 91): объект, наниматель
/// и объем инвестиций переносятся из удовлетворенной заявки, а ставку и ее
/// обоснование считает сервер по Прил. 4 - снимок публикуется (FR-1403, п. 97).
#[utoipa::path(
    post,
    path = "/api/v1/investment-contracts",
    tag = "investment",
    request_body = DraftInvestmentRequest,
    responses(
        (status = 201, description = "Договор составлен", body = InvestmentContractDto),
        (status = 409, description = "Заявка не удовлетворена либо срок больше семи лет (INV-094)", body = crate::error::Problem),
        (status = 422, description = "Данные не прошли проверку", body = crate::error::Problem),
    )
)]
pub async fn draft_investment_contract(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<DraftInvestmentRequest>,
) -> Result<(StatusCode, Json<InvestmentContractDto>), ApiError> {
    user.require(Action::TenderManage)?;
    body.validate()
        .map_err(|r| ApiError::Validation(r.to_string()))?;

    // INV-094 в домене: срок больше семи лет значением не станет,
    // тот же предел стережет CHECK в БД
    let term = Term::new(body.term_months).map_err(|err| ApiError::Validation(err.to_string()))?;

    // Ставка замораживается снимком расчета Прил. 4 - как у лота тендера
    // (FR-201, FR-202): числом с экрана она не задается, иначе публиковать
    // в качестве обоснования нечего (FR-1403, п. 97)
    let request = tou_db::special::get(&state.db, body.special_request_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let object_id = request.object_id.ok_or_else(|| {
        ApiError::RuleViolation(
            "FR-1204: инвестиционный договор заключается на конкретный объект (п. 91)".to_owned(),
        )
    })?;
    let object = tou_db::objects::get(&state.db, object_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let calculation =
        crate::rates::build_calculation(&state.db, object.area_m2, &body.rate_options).await?;
    let rate_calculation = serde_json::to_value(&calculation).map_err(ApiError::internal)?;

    let record = investment::create(
        &state.db,
        user.id(),
        investment::NewInvestmentContract {
            special_request_id: body.special_request_id,
            monthly_rate: calculation.monthly.amount(),
            term_months: term.months(),
            rate_calculation,
        },
    )
    .await
    .map_err(investment_error)?;

    Ok((StatusCode::CREATED, Json(contract_dto(record))))
}

/// Приложение к договору (п. 91): позиция закрытого перечня закрывается файлом.
#[utoipa::path(
    post,
    path = "/api/v1/investment-contracts/{id}/attachments/{code}",
    tag = "investment",
    params(
        ("id" = Uuid, Path, description = "Инвестиционный договор"),
        ("code" = String, Path, description = "Позиция перечня п. 91"),
    ),
    request_body(content = Vec<u8>, content_type = "multipart/form-data",
        description = "Часть `file` с документом"),
    responses(
        (status = 201, description = "Документ сохранен"),
        (status = 404, description = "Договор не найден", body = crate::error::Problem),
        (status = 422, description = "Позиция вне перечня п. 91 либо часть `file` отсутствует", body = crate::error::Problem),
    )
)]
pub async fn upload_attachment(
    user: CurrentUser,
    State(state): State<AppState>,
    Path((id, code)): Path<(Uuid, String)>,
    mut multipart: Multipart,
) -> Result<StatusCode, ApiError> {
    user.require(Action::TenderManage)?;

    // Перечень п. 91 закрыт: код вне него до базы не доходит
    let attachment: Attachment = code
        .parse()
        .map_err(|_| ApiError::Validation("документ вне перечня п. 91 (FR-1204)".to_owned()))?;

    let record = investment::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let mut part = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::Validation(e.to_string()))?
    {
        if field.name() == Some("file") {
            let filename = field
                .file_name()
                .unwrap_or("attachment")
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

    let file_key = format!(
        "investment-contracts/{}/{}",
        record.contract_id,
        attachment.as_str()
    );
    let size_bytes = i64::try_from(bytes.len()).map_err(ApiError::internal)?;
    state
        .storage
        .put(
            &ObjectPath::from(file_key.as_str()),
            PutPayload::from_bytes(bytes),
        )
        .await
        .map_err(ApiError::internal)?;

    investment::add_file(
        &state.db,
        user.id(),
        investment::NewAttachmentFile {
            contract_id: record.contract_id,
            code: attachment.as_str(),
            file_key: &file_key,
            filename: &filename,
            content_type: &content_type,
            size_bytes,
        },
    )
    .await
    .map_err(investment_error)?;

    Ok(StatusCode::CREATED)
}

/// Скачивание приложения договора (п. 91).
#[utoipa::path(
    get,
    path = "/api/v1/investment-contracts/{id}/attachments/{code}",
    tag = "investment",
    params(
        ("id" = Uuid, Path, description = "Инвестиционный договор"),
        ("code" = String, Path, description = "Позиция перечня п. 91"),
    ),
    responses(
        (status = 200, description = "Содержимое документа", content_type = "application/octet-stream", body = Vec<u8>),
        (status = 404, description = "Документ не приложен", body = crate::error::Problem),
    )
)]
pub async fn download_attachment(
    user: CurrentUser,
    State(state): State<AppState>,
    Path((id, code)): Path<(Uuid, String)>,
) -> Result<Response, ApiError> {
    require_investment_access(&user)?;

    let record = investment::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let file = investment::list_files(&state.db, record.contract_id)
        .await?
        .into_iter()
        .find(|file| file.code == code)
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
                    file.filename
                        .chars()
                        .map(|c| if c == '"' || c.is_control() { '_' } else { c })
                        .collect::<String>()
                ),
            ),
        ],
        bytes.to_vec(),
    )
        .into_response())
}

/// Акт приемки инвестиций (FR-1204, п. 92).
#[derive(Debug, Serialize, ToSchema)]
pub struct AcceptanceDto {
    pub id: Uuid,
    #[serde(with = "iso_date")]
    #[schema(value_type = String, format = Date)]
    pub act_date: Date,
    #[schema(value_type = String, example = "10000000.00")]
    pub accepted_amount: Decimal,
    pub note: Option<String>,
    pub accepted_by_name: Option<String>,
    pub has_pdf: bool,
}

fn acceptance_dto(record: AcceptanceRecord) -> AcceptanceDto {
    AcceptanceDto {
        id: record.id,
        act_date: record.act_date,
        accepted_amount: record.accepted_amount,
        note: record.note,
        accepted_by_name: record.accepted_by_name,
        has_pdf: record.pdf_key.is_some(),
    }
}

/// Акты приемки инвестиций по договору (п. 92).
#[utoipa::path(
    get,
    path = "/api/v1/investment-contracts/{id}/acceptances",
    tag = "investment",
    params(("id" = Uuid, Path, description = "Инвестиционный договор")),
    responses((status = 200, description = "Акты приемки", body = [AcceptanceDto]))
)]
pub async fn list_acceptances(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<AcceptanceDto>>, ApiError> {
    require_investment_access(&user)?;

    let record = investment::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let rows = investment::list_acceptances(&state.db, record.contract_id).await?;
    Ok(Json(rows.into_iter().map(acceptance_dto).collect()))
}

#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct AcceptInvestmentRequest {
    #[serde(with = "iso_date")]
    #[schema(value_type = String, format = Date)]
    #[garde(skip)]
    pub act_date: Date,
    #[garde(custom(positive))]
    #[schema(value_type = String, example = "10000000.00")]
    pub accepted_amount: Decimal,
    #[garde(inner(length(chars, max = 4000)))]
    pub note: Option<String>,
}

/// Приемка инвестиций комиссией (FR-1204, п. 92): акт фиксирует принятый
/// объем и служит основанием продления (п. 93).
#[utoipa::path(
    post,
    path = "/api/v1/investment-contracts/{id}/acceptances",
    tag = "investment",
    params(("id" = Uuid, Path, description = "Инвестиционный договор")),
    request_body = AcceptInvestmentRequest,
    responses(
        (status = 201, description = "Акт приемки оформлен", body = AcceptanceDto),
        (status = 404, description = "Договор не найден", body = crate::error::Problem),
        (status = 422, description = "Данные не прошли проверку", body = crate::error::Problem),
    )
)]
pub async fn accept_investment(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AcceptInvestmentRequest>,
) -> Result<(StatusCode, Json<AcceptanceDto>), ApiError> {
    // Приемку ведет комиссия; фиксирует ее секретарь - как и прочие
    // комиссионные документы (A-072)
    user.require(Action::ProtocolGenerate)?;
    body.validate()
        .map_err(|r| ApiError::Validation(r.to_string()))?;

    let record = investment::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let acceptance = investment::accept(
        &state.db,
        user.id(),
        record.contract_id,
        body.act_date,
        body.accepted_amount,
        body.note.as_deref().filter(|note| !note.trim().is_empty()),
    )
    .await
    .map_err(investment_error)?;

    // Печатная форма акта (п. 92) - снимок на момент приемки
    let accepted_total = record.accepted_amount + acceptance.accepted_amount;
    let data = json!({
        "number": acceptance.id.simple().to_string()[..8].to_uppercase(),
        "act_date": acceptance.act_date.to_string(),
        "object": record.object_name.clone().unwrap_or_else(|| "-".to_owned()),
        "tenant": record.tenant_name.clone().unwrap_or_else(|| "-".to_owned()),
        "promised": record.investment_amount.to_string(),
        "accepted": acceptance.accepted_amount.to_string(),
        "accepted_total": accepted_total.to_string(),
        "complete": accepted_total >= record.investment_amount,
        "note": acceptance.note.clone().unwrap_or_default(),
        "accepted_by": acceptance.accepted_by_name.clone().unwrap_or_else(|| "-".to_owned()),
    });
    let pdf_bytes = tokio::task::spawn_blocking(move || pdf::render(ACT_TEMPLATE, &data))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;

    let pdf_key = format!(
        "investment-contracts/{}/acceptance-{}.pdf",
        record.contract_id, acceptance.id
    );
    state
        .storage
        .put(
            &ObjectPath::from(pdf_key.as_str()),
            PutPayload::from(pdf_bytes),
        )
        .await
        .map_err(ApiError::internal)?;
    investment::attach_acceptance_pdf(&state.db, user.id(), acceptance.id, &pdf_key)
        .await
        .map_err(investment_error)?;

    Ok((StatusCode::CREATED, Json(acceptance_dto(acceptance))))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ExtendInvestmentRequest {
    /// `three_years` - продление на три года, `prolongation` - пролонгация
    pub extension: String,
    /// Период пролонгации в месяцах (для продления - всегда 36)
    pub months: Option<i32>,
}

/// Продление договора (FR-1204, п. 93): однократное продление на три года
/// при полном исполнении и объеме от 30 млн ₸; пролонгация - от 100 млн ₸
/// решением Правления.
#[utoipa::path(
    post,
    path = "/api/v1/investment-contracts/{id}/extend",
    tag = "investment",
    params(("id" = Uuid, Path, description = "Инвестиционный договор")),
    request_body = ExtendInvestmentRequest,
    responses(
        (status = 200, description = "Договор продлен", body = InvestmentContractDto),
        (status = 409, description = "Условия продления не наступили (п. 93)", body = crate::error::Problem),
        (status = 422, description = "Неизвестный способ продления", body = crate::error::Problem),
    )
)]
pub async fn extend_investment(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ExtendInvestmentRequest>,
) -> Result<Json<InvestmentContractDto>, ApiError> {
    user.require(Action::BoardDecide)?;

    let extension: Extension = body
        .extension
        .parse()
        .map_err(|_| ApiError::Validation("неизвестный способ продления (п. 93)".to_owned()))?;

    let record = investment::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;

    // Условия п. 93 в домене; тот же отказ выдаст триггер БД
    let progress = Progress {
        promised: record.investment_amount,
        accepted: record.accepted_amount,
    };
    let extensions = Extensions {
        extended: record.extended_at.is_some(),
        prolonged: record.prolonged_at.is_some(),
    };
    extensions
        .allow(extension, progress)
        .map_err(|err| ApiError::RuleViolation(err.to_string()))?;

    let months = match extension {
        Extension::ThreeYears => tou_domain::investment::EXTENSION_MONTHS,
        // «Аналогичный период» (п. 93): по умолчанию - срок самого договора
        Extension::Prolongation => body.months.unwrap_or(record.term_months),
    };

    let updated = investment::extend(
        &state.db,
        user.id(),
        id,
        extension == Extension::Prolongation,
        months,
    )
    .await
    .map_err(investment_error)?;

    Ok(Json(contract_dto(updated)))
}

/// Печатная форма договора по инвесторской форме (Прил. 6, FR-1204).
#[utoipa::path(
    get,
    path = "/api/v1/investment-contracts/{id}/contract.pdf",
    tag = "investment",
    params(("id" = Uuid, Path, description = "Инвестиционный договор")),
    responses(
        (status = 200, description = "PDF договора по форме Прил. 6",
         content_type = "application/pdf", body = Vec<u8>),
        (status = 404, description = "Договор не найден", body = crate::error::Problem),
    )
)]
pub async fn investment_contract_pdf(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    require_investment_access(&user)?;

    let record = investment::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let files = investment::list_files(&state.db, record.contract_id).await?;

    let data = json!({
        "number": record.contract_id.simple().to_string()[..8].to_uppercase(),
        "object": record.object_name.clone().unwrap_or_else(|| "-".to_owned()),
        "tenant": record.tenant_name.clone().unwrap_or_else(|| "-".to_owned()),
        "monthly_rate": crate::announcement::format_decimal_ru(record.monthly_rate),
        "investment_amount": crate::announcement::format_decimal_ru(record.investment_amount),
        "term_months": record.term_months,
        "attachments": Attachment::ALL.into_iter().map(|attachment| json!({
            "label": attachment_label_ru(attachment),
            "attached": files.iter().any(|file| file.code == attachment.as_str()),
        })).collect::<Vec<_>>(),
    });
    let pdf_bytes = tokio::task::spawn_blocking(move || pdf::render(CONTRACT_TEMPLATE, &data))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/pdf".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("inline; filename=\"investment-contract-{id}.pdf\""),
            ),
        ],
        pdf_bytes,
    )
        .into_response())
}

/// Подписи приложений п. 91 для печатной формы (ru, NFR-01).
fn attachment_label_ru(attachment: Attachment) -> &'static str {
    match attachment {
        Attachment::Estimate => "Смета инвестиционного проекта",
        Attachment::Schedule => "График выполнения работ",
        Attachment::Appraisal => "Заключение оценщика",
        Attachment::Guarantee => "Гарантия исполнения обязательств",
    }
}

/// Договор ведет организатор, продление оформляет Правление, приемку -
/// секретарь комиссии: читают все трое (A-072).
fn require_investment_access(user: &CurrentUser) -> Result<(), ApiError> {
    if user.require(Action::TenderManage).is_ok()
        || user.require(Action::BoardDecide).is_ok()
        || user.require(Action::ProtocolGenerate).is_ok()
    {
        return Ok(());
    }
    Err(ApiError::Forbidden)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Печатная форма Прил. 6 компилируется на снимке договора.
    #[test]
    fn investment_contract_renders_pdf() {
        let data = json!({
            "number": "0TEST",
            "object": "Корпус А, каб. 101",
            "tenant": "ТОО «Инвестор» *#_$@",
            "monthly_rate": "150 000,00",
            "investment_amount": "30 000 000,00",
            "term_months": 84,
            "attachments": [
                {"label": "Смета инвестиционного проекта", "attached": true},
                {"label": "График выполнения работ", "attached": false},
            ],
        });

        let bytes = pdf::render(CONTRACT_TEMPLATE, &data).expect("договор Прил. 6");
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 1_000);
    }

    /// Печатная форма акта приемки инвестиций (п. 92).
    #[test]
    fn investment_act_renders_pdf() {
        let data = json!({
            "number": "0TEST",
            "act_date": "2026-09-01",
            "object": "Корпус А, каб. 101",
            "tenant": "ТОО «Инвестор»",
            "promised": "30000000.00",
            "accepted": "10000000.00",
            "accepted_total": "10000000.00",
            "complete": false,
            "note": "принят первый этап работ",
            "accepted_by": "Секретарь комиссии",
        });

        let bytes = pdf::render(ACT_TEMPLATE, &data).expect("акт приемки");
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn every_attachment_has_a_russian_label() {
        for attachment in Attachment::ALL {
            assert!(!attachment_label_ru(attachment).is_empty());
        }
    }
}
