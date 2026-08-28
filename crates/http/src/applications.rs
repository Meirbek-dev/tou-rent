//! Заявки участников (М4, FR-401–404) и журнал регистрации (Прил. 12).
//!
//! Подача/отзыв - транзакции db-слоя с журнальной записью: дедлайн стережет
//! триггер INV-037 (отказы конвертируются в problem+json `rule_violation`),
//! цены запечатаны RLS INV-040. Файлы лежат в RustFS (бакет dossiers), доступ
//! к ним решает этот слой (FR-403): владелец - всегда, secretary/commission -
//! только после вскрытия.

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
use time::OffsetDateTime;
use tou_db::applications::{
    self, ApplicationRecord, FileRecord, JournalRecord, SubmitError, WithdrawError,
};
use tou_db::tenders;
use tou_domain::policy::Action;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::dto::{ApplicantKindDto, ApplicationStatusDto, JournalEntryKindDto};
use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::request::{Json, Multipart, Path, Query};
use crate::state::AppState;
use crate::storage;
use crate::upload;
use tou_domain::rule::RuleViolation;

#[derive(Debug, Serialize, ToSchema)]
pub struct ApplicationFileDto {
    pub id: Uuid,
    pub document_kind: ApplicationDocumentKindDto,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub uploaded_at: OffsetDateTime,
}

impl ApplicationFileDto {
    fn from_record(r: FileRecord) -> Result<Self, ApiError> {
        Ok(Self {
            id: r.id,
            document_kind: ApplicationDocumentKindDto::from_db(&r.document_kind)?,
            filename: r.filename,
            content_type: r.content_type,
            size_bytes: r.size_bytes,
            uploaded_at: r.uploaded_at,
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationDocumentKindDto {
    ApplicationForm,
    RegistrationCertificate,
    TaxClearance,
    GuaranteePayment,
    QualificationDocuments,
    Legacy,
}

impl ApplicationDocumentKindDto {
    fn as_db(self) -> &'static str {
        match self {
            Self::ApplicationForm => "application_form",
            Self::RegistrationCertificate => "registration_certificate",
            Self::TaxClearance => "tax_clearance",
            Self::GuaranteePayment => "guarantee_payment",
            Self::QualificationDocuments => "qualification_documents",
            Self::Legacy => "legacy",
        }
    }

    fn from_db(raw: &str) -> Result<Self, ApiError> {
        match raw {
            "application_form" => Ok(Self::ApplicationForm),
            "registration_certificate" => Ok(Self::RegistrationCertificate),
            "tax_clearance" => Ok(Self::TaxClearance),
            "guarantee_payment" => Ok(Self::GuaranteePayment),
            "qualification_documents" => Ok(Self::QualificationDocuments),
            "legacy" => Ok(Self::Legacy),
            other => Err(ApiError::internal(std::io::Error::other(format!(
                "unknown application_document_kind: {other}"
            )))),
        }
    }

    fn required(self) -> Result<&'static str, ApiError> {
        match self {
            Self::Legacy => Err(ApiError::Validation(
                "тип legacy зарезервирован для файлов, загруженных до введения обязательного пакета"
                    .to_owned(),
            )),
            _ => Ok(self.as_db()),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct UploadApplicationFileQuery {
    pub document_kind: ApplicationDocumentKindDto,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApplicationDto {
    pub id: Uuid,
    pub tender_id: Uuid,
    pub lot_id: Uuid,
    pub participant_id: Uuid,
    pub status: ApplicationStatusDto,
    pub applicant_kind: ApplicantKindDto,
    /// Сведения Прил. 2 (персональные данные - NFR-07: в логи не попадают)
    #[schema(value_type = Object)]
    pub applicant_details: serde_json::Value,
    /// Сведения о квалификации (Прил. 11)
    pub qualification: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub submitted_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub withdrawn_at: Option<OffsetDateTime>,
    /// Код основания отклонения (FR-502, закрытый перечень п. 52)
    pub rejection_reason: Option<String>,
    /// Ценовое предложение (Прил. 9). None - цена запечатана (INV-040):
    /// до вскрытия ее видит только сам участник.
    #[schema(value_type = Option<String>, example = "36000.00")]
    pub price_amount: Option<Decimal>,
    pub files: Vec<ApplicationFileDto>,
    /// Все пять обязательных PDF загружены в AES-256-конвертах.
    pub package_complete: bool,
}

impl ApplicationDto {
    pub(crate) fn from_record(
        r: ApplicationRecord,
        files: Vec<FileRecord>,
    ) -> Result<Self, ApiError> {
        Ok(Self {
            id: r.id,
            tender_id: r.tender_id,
            lot_id: r.lot_id,
            participant_id: r.participant_id,
            status: ApplicationStatusDto::from_db(&r.status)?,
            applicant_kind: ApplicantKindDto::from_db(&r.applicant_kind)?,
            // Граница слоя: DTO - ответ уполномоченному читателю (NFR-07)
            applicant_details: r.applicant_details.into_inner(),
            qualification: r
                .qualification
                .as_ref()
                .and_then(|q| q.expose().get("text"))
                .and_then(|t| t.as_str())
                .map(str::to_owned),
            submitted_at: r.submitted_at,
            withdrawn_at: r.withdrawn_at,
            rejection_reason: r.rejection_reason,
            price_amount: r.price_amount,
            files: files
                .into_iter()
                .map(ApplicationFileDto::from_record)
                .collect::<Result<Vec<_>, _>>()?,
            package_complete: r.package_complete,
        })
    }
}

/// Сведения о заявителе (Прил. 2; поля-приближение контура 1, A-020).
///
/// Тот же тип принимает сведения заявителя особого порядка (Прил. 3,
/// `special.rs`), поэтому предметная проверка реквизитов здесь - единственная
/// на оба маршрута.
#[derive(Debug, Serialize, Deserialize, garde::Validate, ToSchema)]
pub struct ApplicantDetailsDto {
    /// Наименование юрлица либо ФИО физлица
    #[garde(length(chars, min = 1, max = 300))]
    pub name: String,
    /// БИН/ИИН (персональные данные - NFR-07)
    #[garde(custom(valid_id_number))]
    pub id_number: String,
    #[garde(length(chars, min = 1, max = 500))]
    pub address: String,
    #[garde(custom(valid_phone))]
    pub phone: String,
    #[garde(inner(email, length(max = 254)))]
    pub email: Option<String>,
}

/// Предметная проверка ИИН/БИН живет в домене: по этому реквизиту сторона
/// опознается в договоре и в реестре уклонившихся, и та же проверка теми же
/// правилами стоит в форме (`apps/web/src/lib/validation.ts`). Прежняя
/// «строка от 1 до 32 символов» ловила только пустое поле.
///
/// Имя не совпадает с именем поля намеренно: derive `garde` вводит в область
/// видимости локальную переменную с именем поля, и функция `id_number`
/// оказалась бы ею закрыта.
fn valid_id_number(value: &str, _ctx: &()) -> garde::Result {
    tou_domain::identity::validate_id_number(value)
        .map_err(|err| garde::Error::new(err.to_string()))
}

fn valid_phone(value: &str, _ctx: &()) -> garde::Result {
    tou_domain::identity::validate_phone(value).map_err(|err| garde::Error::new(err.to_string()))
}

#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct SubmitApplicationRequest {
    #[garde(skip)]
    pub lot_id: Uuid,
    #[garde(skip)]
    pub applicant_kind: ApplicantKindDto,
    #[garde(dive)]
    pub applicant_details: ApplicantDetailsDto,
    /// Сведения о квалификации (Прил. 11)
    #[garde(inner(length(chars, max = 4000)))]
    pub qualification: Option<String>,
    /// Первоначальная цена (Прил. 9); запечатывается до вскрытия (INV-040)
    #[garde(custom(positive_amount))]
    #[schema(value_type = String, example = "36000.00")]
    pub price_amount: Decimal,
}

fn positive_amount(value: &Decimal, _ctx: &()) -> garde::Result {
    if *value > Decimal::ZERO {
        Ok(())
    } else {
        Err(garde::Error::new("цена должна быть > 0"))
    }
}

/// Подача заявки (FR-401): заявка + цена + запись журнала одной транзакцией.
/// После дедлайна БД отклоняет всё целиком (INV-037).
#[utoipa::path(
    post,
    path = "/api/v1/tenders/{id}/applications",
    tag = "applications",
    params(("id" = Uuid, Path, description = "Тендер")),
    request_body = SubmitApplicationRequest,
    responses(
        (status = 201, description = "Заявка подана и внесена в журнал", body = ApplicationDto),
        (status = 409, description = "Прием закрыт / дубль заявки (п. 22, INV-037)", body = crate::error::Problem),
        (status = 422, description = "Данные не прошли проверку", body = crate::error::Problem),
    )
)]
pub async fn submit_application(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(tender_id): Path<Uuid>,
    Json(body): Json<SubmitApplicationRequest>,
) -> Result<(StatusCode, Json<ApplicationDto>), ApiError> {
    user.require(Action::ApplicationSubmit)?;
    body.validate()
        .map_err(|r| ApiError::Validation(r.to_string()))?;

    let details = serde_json::to_value(&body.applicant_details).map_err(ApiError::internal)?;
    let qualification = body
        .qualification
        .as_deref()
        .filter(|q| !q.trim().is_empty())
        .map(|q| serde_json::json!({ "text": q }));

    let record = applications::submit(
        &state.db,
        user.id(),
        applications::NewApplication {
            tender_id,
            lot_id: body.lot_id,
            applicant_kind: body.applicant_kind.as_db(),
            applicant_details: &details,
            qualification: qualification.as_ref(),
            price_amount: body.price_amount,
        },
    )
    .await
    .map_err(|err| match err {
        SubmitError::NotAccepting => ApiError::rule(
            RuleViolation::ApplicationIntakeClosed,
            "прием заявок по тендеру не открыт (п. 36)",
        ),
        SubmitError::Duplicate => ApiError::rule(
            RuleViolation::ApplicationAlreadySubmitted,
            "заявка на этот лот уже подана (п. 22)",
        ),
        SubmitError::Rejected(reason) => ApiError::RuleViolation(reason),
        SubmitError::Db(db) => db.into(),
    })?;

    Ok((
        StatusCode::CREATED,
        Json(ApplicationDto::from_record(record, Vec::new())?),
    ))
}

/// Мои заявки (кабинет участника): свои цены видны всегда (RLS).
#[utoipa::path(
    get,
    path = "/api/v1/applications/my",
    tag = "applications",
    responses((status = 200, description = "Заявки участника", body = [ApplicationDto]))
)]
pub async fn my_applications(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<ApplicationDto>>, ApiError> {
    user.require(Action::ApplicationReadOwn)?;
    let records = applications::list_own(&state.db, user.id()).await?;
    Ok(Json(with_files(&state, records).await?))
}

/// Отзыв заявки до дедлайна (FR-404) с фиксацией в журнале.
#[utoipa::path(
    post,
    path = "/api/v1/applications/{id}/withdraw",
    tag = "applications",
    params(("id" = Uuid, Path, description = "Заявка")),
    responses(
        (status = 200, description = "Заявка отозвана", body = ApplicationDto),
        (status = 409, description = "Отзыв невозможен (статус/дедлайн, INV-037)", body = crate::error::Problem),
    )
)]
pub async fn withdraw_application(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApplicationDto>, ApiError> {
    user.require(Action::ApplicationWithdraw)?;

    let record = applications::withdraw(&state.db, user.id(), id)
        .await
        .map_err(|err| match err {
            WithdrawError::NotWithdrawable => ApiError::rule(
                RuleViolation::ApplicationNotPending,
                "заявка не найдена или уже не в статусе «подана» (FR-404)",
            ),
            WithdrawError::Rejected(reason) => ApiError::RuleViolation(reason),
            WithdrawError::Db(db) => db.into(),
        })?;

    let files = applications::list_files(&state.db, record.id).await?;
    Ok(Json(ApplicationDto::from_record(record, files)?))
}

/// Загрузка файла-вложения (FR-401) к своей поданной заявке до дедлайна.
#[utoipa::path(
    post,
    path = "/api/v1/applications/{id}/files",
    tag = "applications",
    params(("id" = Uuid, Path, description = "Заявка")),
    params(("document_kind" = ApplicationDocumentKindDto, Query,
        description = "Тип обязательного документа")),
    request_body(content = Vec<u8>, content_type = "multipart/form-data",
        description = "Часть `file` с вложением"),
    responses(
        (status = 201, description = "Файл сохранен", body = ApplicationFileDto),
        (status = 409, description = "Заявка не «подана» или прием закрыт", body = crate::error::Problem),
        (status = 422, description = "Часть `file` отсутствует", body = crate::error::Problem),
    )
)]
pub async fn upload_file(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<UploadApplicationFileQuery>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<ApplicationFileDto>), ApiError> {
    user.require(Action::ApplicationSubmit)?;

    // Право `ApplicationSubmit` есть у любого участника, поэтому одного его
    // мало: до этой проверки чужой (или несуществующий) id заявки доходил
    // до `put`, а отказ приходил уже после записи в бакет. Бакет досье -
    // Object Lock в режиме compliance на пять лет (INV-042), удалять из него
    // не вправе ни api, ни владелец хранилища, так что каждый такой отказ
    // занимал место навсегда. Атомарная проверка в `add_file` остается
    // последним рубежом от гонки с дедлайном, эта - закрывает дешевый способ
    // засорить WORM.
    ensure_open_own_application(&state, &user, id).await?;

    let document_kind = query.document_kind.required()?;
    let file =
        upload::take_file(&mut multipart, "file", "attachment", upload::MAX_FILE_BYTES).await?;
    if file.content_type != "application/pdf" {
        return Err(ApiError::UnsupportedMediaType(
            "пакет заявки принимается только в формате PDF".to_owned(),
        ));
    }
    let size_bytes = i64::try_from(file.bytes.len()).map_err(ApiError::internal)?;
    let encrypted = state
        .file_cipher
        .encrypt(id, &file.bytes)
        .map_err(ApiError::internal)?;

    // Ключ не зависит от id строки метаданных: объект кладется до вставки.
    // Компенсации при отказе БД нет - бакет досье под Object Lock, и права
    // на удаление у приложения нет (INV-042); см. ветку `None` ниже.
    let file_key = storage::application_file_key(id, Uuid::now_v7());
    let object_path = ObjectPath::from(file_key.as_str());

    state
        .storage
        .put(
            &object_path,
            PutPayload::from_bytes(axum::body::Bytes::from(encrypted)),
        )
        .await
        .map_err(ApiError::internal)?;

    let inserted = applications::add_file(
        &state.db,
        user.id(),
        applications::NewApplicationFile {
            application_id: id,
            file_key: &file_key,
            filename: &file.filename,
            document_kind,
            content_type: file.content_type,
            size_bytes,
        },
    )
    .await?;

    match inserted {
        Some(record) => Ok((
            StatusCode::CREATED,
            Json(ApplicationFileDto::from_record(record)?),
        )),
        None => {
            // Метаданные не записаны - объект остается в бакете сиротой.
            // Это безвредно: файлы выдаются только по строке метаданных,
            // без нее объект недостижим и мимо приложения. Ключ - в журнал:
            // разобрать бакет может только владелец хранилища (INV-042)
            tracing::warn!(%file_key, "объект загружен, метаданные отклонены");
            Err(ApiError::rule(
                RuleViolation::ApplicationDeadlinePassed,
                "файл можно приложить только к своей поданной заявке до дедлайна (п. 36–39)",
            ))
        }
    }
}

/// Предварительная проверка перед записью в бакет: заявка есть, она своя
/// и прием по тендеру еще открыт.
///
/// Дублирует условие `applications::add_file` намеренно. Та проверка атомарна
/// и остается решающей, но выполняется после `put`; эта - до, и потому решает
/// другую задачу: не пустить в WORM объект, который заведомо будет отвергнут.
/// Время берется у БД (`core.now()`, ADR-0005) - тем же источником, которым
/// его меряет триггер INV-037, иначе граница дедлайна у двух проверок разъедется.
async fn ensure_open_own_application(
    state: &AppState,
    user: &CurrentUser,
    id: Uuid,
) -> Result<(), ApiError> {
    let record = applications::get(&state.db, user.id(), id)
        .await?
        .ok_or(ApiError::NotFound)?;

    // Чужая заявка - 404, а не 403: иначе по коду ответа перебираются
    // идентификаторы чужих заявок (NFR-07)
    if record.participant_id != user.id() {
        return Err(ApiError::NotFound);
    }

    if record.status != "submitted" {
        return Err(ApiError::rule(
            RuleViolation::ApplicationDeadlinePassed,
            "файл можно приложить только к своей поданной заявке до дедлайна (п. 36–39)",
        ));
    }

    let tender = tenders::get(&state.db, record.tender_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if let Some(deadline) = tender.submission_deadline
        && tou_db::refdata::now(&state.db).await? > deadline
    {
        return Err(ApiError::rule(
            RuleViolation::ApplicationDeadlinePassed,
            "прием заявок по тендеру закрыт - состав заявки зафиксирован (п. 36–39)",
        ));
    }

    Ok(())
}

/// Скачивание вложения после вскрытия. До назначенного события AES-ключ не
/// применяется ни для владельца, ни для администратора или комиссии.
#[utoipa::path(
    get,
    path = "/api/v1/applications/{id}/files/{file_id}",
    tag = "applications",
    params(
        ("id" = Uuid, Path, description = "Заявка"),
        ("file_id" = Uuid, Path, description = "Файл"),
    ),
    responses(
        (status = 200, description = "Содержимое файла", content_type = "application/octet-stream", body = Vec<u8>),
        (status = 404, description = "Нет файла или нет доступа", body = crate::error::Problem),
    )
)]
pub async fn download_file(
    user: CurrentUser,
    State(state): State<AppState>,
    Path((id, file_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, ApiError> {
    let file = applications::get_file(&state.db, file_id)
        .await?
        .filter(|f| f.application_id == id)
        .ok_or(ApiError::NotFound)?;

    let record = applications::get(&state.db, user.id(), id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let tender = tenders::get(&state.db, record.tender_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if tender.opened_at.is_none() {
        return Err(ApiError::NotFound);
    }

    if record.participant_id != user.id() {
        user.require(Action::ApplicationReadAll)?;
    }

    let object = state
        .storage
        .get(&ObjectPath::from(file.file_key.as_str()))
        .await
        .map_err(ApiError::internal)?;
    let stored = object.bytes().await.map_err(ApiError::internal)?;
    let bytes = match file.encryption_version {
        0 => stored.to_vec(),
        1 => state
            .file_cipher
            .decrypt(id, &stored)
            .map_err(ApiError::internal)?,
        version => {
            return Err(ApiError::internal(std::io::Error::other(format!(
                "unsupported application file encryption version: {version}"
            ))));
        }
    };

    Ok((
        [
            (header::CONTENT_TYPE, file.content_type.clone()),
            (
                header::CONTENT_DISPOSITION,
                format!(
                    "attachment; filename=\"{}\"",
                    upload::sanitize_filename(&file.filename)
                ),
            ),
        ],
        bytes,
    )
        .into_response())
}

/// Заявки тендера (secretary/commission): цены до вскрытия скрыты RLS (INV-040).
#[utoipa::path(
    get,
    path = "/api/v1/tenders/{id}/applications",
    tag = "applications",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 200, description = "Заявки тендера", body = [ApplicationDto]),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
    )
)]
pub async fn tender_applications(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(tender_id): Path<Uuid>,
) -> Result<Json<Vec<ApplicationDto>>, ApiError> {
    user.require(Action::ApplicationReadAll)?;
    let records = applications::list_for_tender(&state.db, user.id(), tender_id).await?;
    Ok(Json(with_files(&state, records).await?))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct JournalEntryDto {
    pub seq: i32,
    pub entry_kind: JournalEntryKindDto,
    pub application_id: Option<Uuid>,
    pub actor_id: Option<Uuid>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub occurred_at: OffsetDateTime,
    pub note: Option<String>,
}

impl JournalEntryDto {
    fn from_record(r: JournalRecord) -> Result<Self, ApiError> {
        Ok(Self {
            seq: r.seq,
            entry_kind: JournalEntryKindDto::from_db(&r.entry_kind)?,
            application_id: r.application_id,
            actor_id: r.actor_id,
            occurred_at: r.occurred_at,
            note: r.note,
        })
    }
}

/// Журнал регистрации заявок (Прил. 12, FR-402) - секретарь комиссии.
#[utoipa::path(
    get,
    path = "/api/v1/tenders/{id}/journal",
    tag = "applications",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 200, description = "Журнал (append-only, seq монотонен)", body = [JournalEntryDto]),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
    )
)]
pub async fn tender_journal(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(tender_id): Path<Uuid>,
) -> Result<Json<Vec<JournalEntryDto>>, ApiError> {
    user.require(Action::JournalRead)?;
    let records = applications::journal_of(&state.db, tender_id).await?;
    records
        .into_iter()
        .map(JournalEntryDto::from_record)
        .collect::<Result<Vec<_>, _>>()
        .map(Json)
}

/// Файлы пачки заявок одним запросом + сборка DTO.
async fn with_files(
    state: &AppState,
    records: Vec<ApplicationRecord>,
) -> Result<Vec<ApplicationDto>, ApiError> {
    let ids: Vec<Uuid> = records.iter().map(|r| r.id).collect();
    let mut by_app: HashMap<Uuid, Vec<FileRecord>> = HashMap::new();
    for file in applications::files_for(&state.db, &ids).await? {
        by_app.entry(file.application_id).or_default().push(file);
    }
    records
        .into_iter()
        .map(|r| {
            let files = by_app.remove(&r.id).unwrap_or_default();
            ApplicationDto::from_record(r, files)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn details(id_number: &str, phone: &str) -> ApplicantDetailsDto {
        ApplicantDetailsDto {
            name: "ТОО «Тест»".to_owned(),
            id_number: id_number.to_owned(),
            address: "г. Павлодар, ул. Ломова, 64".to_owned(),
            phone: phone.to_owned(),
            email: None,
        }
    }

    /// Проверка домена действительно подключена к DTO: без нее сюда проходила
    /// любая строка от 1 до 32 символов (W-13). Перечни примеров и границы -
    /// в `tou_domain::identity`, здесь проверяется только проводка.
    #[test]
    fn applicant_details_run_the_domain_checks() {
        assert!(
            details("670124440127", "+7 701 123 45 67")
                .validate()
                .is_ok()
        );
        // Опечатка в контрольном разряде
        assert!(
            details("670124440128", "+7 701 123 45 67")
                .validate()
                .is_err()
        );
        assert!(details("670124440127", "не указан").validate().is_err());
    }
}
