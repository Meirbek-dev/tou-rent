//! Договорный конвейер (М9, FR-901–902, FR-905, INV-115).
//!
//! Договор составляется из итогов торгов: существенные условия - снимок,
//! менять их нельзя (FR-901). Шаги п. 110–115 идут по порядку, подпись
//! наймодателя блокируется без завершенной сверки (INV-115), регистрация
//! в журнале дает дату заключения (п. 126).

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use object_store::ObjectStoreExt as _;
use object_store::PutPayload;
use object_store::path::Path as ObjectPath;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;
use tou_db::contracts::{self, ContractError, ContractRecord};
use tou_db::{objects, tenders};
use tou_domain::contract::Stage;
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

const TEMPLATE: &str = include_str!("templates/contract.typ");

fn contract_error(err: ContractError) -> ApiError {
    match err {
        ContractError::NotFound => ApiError::NotFound,
        ContractError::Rejected(reason) => ApiError::RuleViolation(reason),
        ContractError::Db(db) => db.into(),
    }
}

/// Договор и пройденные шаги конвейера (FR-902).
#[derive(Debug, Serialize, ToSchema)]
pub struct ContractDto {
    pub id: Uuid,
    pub tender_id: Option<Uuid>,
    pub lot_id: Option<Uuid>,
    pub lot_seq: Option<i32>,
    pub object_name: String,
    pub tenant_name: String,
    /// `draft` | `signing` | `active` | … (`core.contract_status`)
    pub status: String,
    /// Сторона договора: `winner` либо `runner_up` после уклонения (FR-903)
    pub place: String,
    /// Уклонение по договору зафиксировано (п. 116): договор прекращен
    pub evaded: bool,
    /// Существенные условия: ставка из торгов и срок найма (FR-901)
    #[schema(value_type = String, example = "79750.00")]
    pub monthly_rate: Decimal,
    pub lease_months: Option<i32>,
    /// Последний пройденный шаг п. 110–115; `null` - только составлен
    pub stage: Option<String>,
    pub stage_rule_ref: Option<String>,
    /// Следующий шаг, который можно выполнить
    pub next_stage: Option<String>,
    pub checklist_complete: bool,
    pub reg_number: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub registered_at: Option<OffsetDateTime>,
    pub has_pdf: bool,
    pub has_scan: bool,
    /// Способ подписания (ТЗ § 2): `unsigned` | `paper` | `electronic`.
    /// ЭЦП вне периметра, поэтому подписанным считается загруженный скан
    pub signature_status: String,
}

fn contract_dto(record: ContractRecord) -> ContractDto {
    let progress = record.progress();
    ContractDto {
        id: record.id,
        tender_id: record.tender_id,
        lot_id: record.lot_id,
        lot_seq: record.lot_seq,
        object_name: record.object_name,
        tenant_name: record.tenant_name,
        status: record.status,
        place: record.place,
        evaded: record.evaded,
        monthly_rate: record.monthly_rate,
        lease_months: record.lease_months,
        stage: progress.current.map(|stage| stage.as_str().to_owned()),
        stage_rule_ref: progress.current.map(|stage| stage.rule_ref().to_owned()),
        next_stage: progress
            .current
            .map_or(Some(Stage::Drafted), Stage::next)
            .map(|stage| stage.as_str().to_owned()),
        checklist_complete: progress.checklist_complete,
        reg_number: record.reg_number,
        registered_at: record.registered_at,
        has_pdf: record.pdf_key.is_some(),
        has_scan: record.signed_scan_key.is_some(),
        signature_status: record.signature_status,
    }
}

/// Договоры тендера (FR-901): по одному на лот с победителем.
#[utoipa::path(
    get,
    path = "/api/v1/tenders/{id}/contracts",
    tag = "contracts",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses((status = 200, description = "Договоры тендера", body = [ContractDto]))
)]
pub async fn tender_contracts(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<ContractDto>>, ApiError> {
    // Не `TenderRead`: тендер публичен, договорный конвейер - нет.
    // Свой договор наниматель читает через `GET /contracts/my`
    user.require(Action::ContractRead)?;

    let records = contracts::list_for_tender(&state.db, id).await?;
    Ok(Json(records.into_iter().map(contract_dto).collect()))
}

/// Договоры кабинета нанимателя (ТЗ § 7).
///
/// `next_after` всегда `null`: договоров у нанимателя единицы, продолжения
/// у выборки нет. Поле остается ради единообразия усекаемых реестров.
#[derive(Debug, Serialize, ToSchema)]
pub struct MyContractPage {
    pub items: Vec<ContractDto>,
    pub next_after: Option<String>,
    /// Показана не вся выборка
    pub truncated: bool,
}

/// Мои договоры (кабинет нанимателя).
#[utoipa::path(
    get,
    path = "/api/v1/contracts/my",
    tag = "contracts",
    responses((status = 200, description = "Договоры нанимателя", body = MyContractPage))
)]
pub async fn my_contracts(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<MyContractPage>, ApiError> {
    let page = contracts::list_for_tenant(&state.db, user.id()).await?;
    let truncated = page.truncated;
    Ok(Json(MyContractPage {
        items: page.into_iter().map(contract_dto).collect(),
        next_after: None,
        truncated,
    }))
}

/// Доступ к конкретному договору: ведущие процесс - по праву, наниматель -
/// по участию в нем.
///
/// Право `ContractRead` закрывает договорный конвейер от посторонних
/// (R-13), но своя сторона договора - не посторонний: без этой проверки
/// наниматель не видел бы ни собственных сроков п. 110–115, ни перечня
/// документов, которые сам обязан представить.
pub(crate) async fn ensure_contract_party(
    user: &CurrentUser,
    state: &AppState,
    contract_id: Uuid,
) -> Result<ContractRecord, ApiError> {
    let record = contracts::get(&state.db, contract_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    if record.tenant_id != user.id() {
        user.require(Action::ContractRead)?;
    }
    Ok(record)
}

/// Составление договора по итогам торгов лота (FR-901, п. 108, 110).
/// Условия берутся из победившей ставки - задать их извне нельзя.
#[utoipa::path(
    post,
    path = "/api/v1/lots/{id}/contract",
    tag = "contracts",
    params(("id" = Uuid, Path, description = "Лот")),
    responses(
        (status = 201, description = "Договор составлен", body = ContractDto),
        (status = 409, description = "Торги не завершены либо нет победителя", body = crate::error::Problem),
    )
)]
pub async fn draft_contract(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(lot_id): Path<Uuid>,
) -> Result<(StatusCode, Json<ContractDto>), ApiError> {
    user.require(Action::ContractManage)?;

    let record = contracts::draft_from_auction(&state.db, user.id(), lot_id)
        .await
        .map_err(contract_error)?;

    // Печатная форма Прил. 5 готовится сразу: договор передается победителю
    // именно ею (п. 110)
    let contract_id = record.id;
    render_and_store(&state, &user, record).await?;

    let record = contracts::get(&state.db, contract_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok((StatusCode::CREATED, Json(contract_dto(record))))
}

/// Печатная форма договора (Прил. 5) в RustFS.
async fn render_and_store(
    state: &AppState,
    user: &CurrentUser,
    record: ContractRecord,
) -> Result<(), ApiError> {
    let object = objects::get(&state.db, record_object_id(&record, state).await?)
        .await?
        .ok_or(ApiError::NotFound)?;
    let tender = match record.tender_id {
        Some(id) => tenders::get(&state.db, id).await?,
        None => None,
    };
    let lots = match record.tender_id {
        Some(id) => tenders::lots_of(&state.db, id).await?,
        None => Vec::new(),
    };
    let purpose = lots
        .iter()
        .find(|lot| Some(lot.id) == record.lot_id)
        .map(|lot| lot.purpose.clone())
        .unwrap_or_else(|| "-".to_owned());

    // Отметка «сформировано» - по часам сервера (`core.now()`, ADR-0005)
    let generated_at = tou_db::refdata::now(&state.db).await?;

    let data = json!({
        "number": record.reg_number.clone().unwrap_or_else(|| "б/н (до регистрации)".to_owned()),
        "contract_date": format_almaty(record.registered_at.or(record.drafted_at)),
        "tenant_name": record.tenant_name,
        "tender_title": tender.as_ref().map_or("-".to_owned(), |t| t.title.clone()),
        "protocol_number": "по протоколу итогов", // TODO-ENGINEER: номер протокола в форме
        "object_name": object.name,
        "object_address": object.address,
        "object_area": format_decimal_ru(object.area_m2),
        "purpose": purpose,
        "monthly_rate": format_decimal_ru(record.monthly_rate),
        "lease_months": record.lease_months.unwrap_or(0),
        "deposit": format_decimal_ru(record.monthly_rate),
        "generated_at": format_almaty(Some(generated_at)),
    });

    let pdf_bytes = tokio::task::spawn_blocking(move || pdf::render(TEMPLATE, &data))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;

    let pdf_key = format!("contracts/{}/contract.pdf", record.id);
    state
        .storage
        .put(
            &ObjectPath::from(pdf_key.as_str()),
            PutPayload::from(pdf_bytes),
        )
        .await
        .map_err(ApiError::internal)?;

    contracts::attach_pdf(&state.db, user.id(), record.id, &pdf_key)
        .await
        .map_err(contract_error)
}

/// Объект договора: в записи он уже есть, но выбирается вместе с названием -
/// для печатной формы нужны адрес и площадь.
async fn record_object_id(record: &ContractRecord, state: &AppState) -> Result<Uuid, ApiError> {
    let id = sqlx::query_scalar!(
        "SELECT object_id FROM core.contracts WHERE id = $1",
        record.id
    )
    .fetch_one(&state.db)
    .await?;
    Ok(id)
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ChecklistItemDto {
    pub item_code: String,
    pub label_ru: String,
    /// Подпункт п. 113
    pub rule_ref: String,
    pub checked: bool,
    pub checked_by_name: Option<String>,
}

/// Чек-лист сверки документов (п. 113, INV-115).
#[utoipa::path(
    get,
    path = "/api/v1/contracts/{id}/checklist",
    tag = "contracts",
    params(("id" = Uuid, Path, description = "Договор")),
    responses((status = 200, description = "Перечень сверки", body = [ChecklistItemDto]))
)]
pub async fn contract_checklist(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<ChecklistItemDto>>, ApiError> {
    // Перечень сверки п. 113 - документы конкретного нанимателя: он их и
    // представляет (п. 112), значит должен видеть, чего не хватает
    ensure_contract_party(&user, &state, id).await?;

    let rows = contracts::checklist(&state.db, id).await?;
    Ok(Json(rows.into_iter().map(checklist_dto).collect()))
}

fn checklist_dto(row: contracts::ChecklistRow) -> ChecklistItemDto {
    ChecklistItemDto {
        item_code: row.item_code,
        label_ru: row.label_ru,
        rule_ref: row.rule_ref,
        checked: row.checked_at.is_some(),
        checked_by_name: row.checked_by_name,
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CheckItemRequest {
    pub item_code: String,
    /// `false` снимает отметку, пока договор не подписан наймодателем
    pub checked: bool,
}

/// Отметка позиции сверки (п. 113): без полного перечня договор не
/// подписывается (INV-115).
#[utoipa::path(
    post,
    path = "/api/v1/contracts/{id}/checklist",
    tag = "contracts",
    params(("id" = Uuid, Path, description = "Договор")),
    request_body = CheckItemRequest,
    responses(
        (status = 200, description = "Перечень сверки", body = [ChecklistItemDto]),
        (status = 409, description = "Сверка закрыта либо позиция неизвестна", body = crate::error::Problem),
    )
)]
pub async fn check_checklist_item(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<CheckItemRequest>,
) -> Result<Json<Vec<ChecklistItemDto>>, ApiError> {
    user.require(Action::ContractManage)?;

    let rows = contracts::check_item(&state.db, user.id(), id, &body.item_code, body.checked)
        .await
        .map_err(contract_error)?;
    Ok(Json(rows.into_iter().map(checklist_dto).collect()))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AdvanceRequest {
    /// Шаг конвейера: `handed_to_tenant`, `tenant_signed`, … (п. 110–115)
    pub stage: String,
}

/// Шаг конвейера (FR-902, п. 110–115). Порядок и INV-115 проверяются
/// доменом и БД: пропустить сверку перед подписанием нельзя.
#[utoipa::path(
    post,
    path = "/api/v1/contracts/{id}/advance",
    tag = "contracts",
    params(("id" = Uuid, Path, description = "Договор")),
    request_body = AdvanceRequest,
    responses(
        (status = 200, description = "Шаг зафиксирован", body = ContractDto),
        (status = 409, description = "Шаг вне очереди либо сверка не завершена", body = crate::error::Problem),
        (status = 422, description = "Неизвестный шаг", body = crate::error::Problem),
    )
)]
pub async fn advance_contract(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AdvanceRequest>,
) -> Result<Json<ContractDto>, ApiError> {
    user.require(Action::ContractManage)?;

    let stage: Stage = body
        .stage
        .parse()
        .map_err(|_| ApiError::Validation(format!("неизвестный шаг конвейера: {}", body.stage)))?;

    let record = contracts::advance(&state.db, user.id(), id, stage)
        .await
        .map_err(contract_error)?;
    Ok(Json(contract_dto(record)))
}

/// Имя типа уникально в контракте: `ToSchema` берет имя компонента OpenAPI
/// из имени структуры, поэтому одноименный тип в другом модуле молча
/// затер бы схему (в `auth` есть свой `RegisterRequest`).
#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterContractRequest {
    /// Номер в Журнале регистрации договоров (п. 126)
    pub reg_number: String,
}

/// Регистрация договора (FR-905, п. 126): дата регистрации - дата
/// заключения, с нее считается период найма (INV-DB-02).
#[utoipa::path(
    post,
    path = "/api/v1/contracts/{id}/register",
    tag = "contracts",
    params(("id" = Uuid, Path, description = "Договор")),
    request_body = RegisterContractRequest,
    responses(
        (status = 200, description = "Договор зарегистрирован", body = ContractDto),
        (status = 409, description = "Договор не подписан обеими сторонами либо номер занят",
         body = crate::error::Problem),
    )
)]
pub async fn register_contract(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<RegisterContractRequest>,
) -> Result<Json<ContractDto>, ApiError> {
    user.require(Action::ContractManage)?;

    let record = contracts::register(&state.db, user.id(), id, &body.reg_number)
        .await
        .map_err(contract_error)?;
    Ok(Json(contract_dto(record)))
}

/// Скан подписанного экземпляра (FR-905, без ЭЦП).
#[utoipa::path(
    post,
    path = "/api/v1/contracts/{id}/scan",
    tag = "contracts",
    params(("id" = Uuid, Path, description = "Договор")),
    request_body(content = String, content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "Скан загружен", body = ContractDto),
        (status = 422, description = "Файл не приложен", body = crate::error::Problem),
    )
)]
pub async fn upload_scan(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<ContractDto>, ApiError> {
    // Подписанный экземпляр возвращает наниматель (п. 111) - до сих пор
    // это делал за него организатор, и следа действия самой стороны
    // договора не оставалось. Право проверяется до записи в бакет: досье
    // под Object Lock (INV-042), сироту оттуда не убрать
    ensure_contract_party(&user, &state, id).await?;

    // Прежний разбор клал в бакет КАЖДУЮ часть формы, а в БД записывал ключ
    // только последней: все прочие оставались несносимыми сиротами. Плюс ключ
    // строился из имени файла как есть - с путем и кавычками внутри
    let file =
        upload::take_file(&mut multipart, "file", "scan.pdf", upload::MAX_FILE_BYTES).await?;

    let key = format!("contracts/{id}/signed-{}", file.filename);
    state
        .storage
        .put(
            &ObjectPath::from(key.as_str()),
            PutPayload::from_bytes(file.bytes),
        )
        .await
        .map_err(ApiError::internal)?;

    let record = contracts::attach_scan(&state.db, user.id(), id, &key)
        .await
        .map_err(contract_error)?;
    Ok(Json(contract_dto(record)))
}

/// PDF договора (печатная форма Прил. 5) из RustFS.
#[utoipa::path(
    get,
    path = "/api/v1/contracts/{id}/pdf",
    tag = "contracts",
    params(("id" = Uuid, Path, description = "Договор")),
    responses(
        (status = 200, description = "PDF договора", content_type = "application/pdf"),
        (status = 404, description = "Печатная форма не сформирована", body = crate::error::Problem),
    )
)]
pub async fn contract_pdf(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let record = contracts::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;

    // Свой договор видит наниматель; чужие - организатор и секретарь
    if record.tenant_id != user.id() {
        user.require(Action::ContractManage)?;
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
                format!("inline; filename=\"contract-{id}.pdf\""),
            ),
        ],
        bytes,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Печатная форма компилируется в PDF на снимке существенных условий.
    #[test]
    fn contract_renders_pdf() {
        let data = json!({
            "number": "Д-0TEST",
            "contract_date": "20.08.2026 10:00",
            "tenant_name": "ТОО «Тест» - спецсимволы *#_$@«»",
            "tender_title": "Тестовый тендер",
            "protocol_number": "И-0TEST",
            "object_name": "Помещение 42 м²",
            "object_address": "Павлодар, Ломова 64",
            "object_area": "42,00",
            "purpose": "офис",
            "monthly_rate": "79 750,00",
            "lease_months": 12,
            "deposit": "79 750,00",
            "generated_at": "20.08.2026 10:05",
        });

        let bytes = pdf::render(TEMPLATE, &data).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 1_000);
    }
}
