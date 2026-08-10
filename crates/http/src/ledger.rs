//! Взносы и депозитная книга (М10, FR-405, FR-1001–1004).
//!
//! Банк-интеграции нет: поступление подтверждает оператор финблока вручную,
//! указывая сумму и дату (ответ заказчика № 11). Правила п. 23, 25, 26 и
//! INV-DB-05 проверяет слой данных и БД - здесь доступ и контракт.

use axum::extract::State;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tou_db::RowCursor;
use tou_db::ledger::{self, LedgerError};
use tou_domain::ledger::{AccountKind, RefundReason};
use tou_domain::policy::Action;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::dto::cursor;
use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::request::{Json, Path, Query};
use crate::state::AppState;

fn ledger_error(err: LedgerError) -> ApiError {
    match err {
        LedgerError::NotFound => ApiError::NotFound,
        LedgerError::Rejected(reason) => ApiError::RuleViolation(reason),
        LedgerError::Db(db) => db.into(),
    }
}

/// Счет депозитной книги с текущим остатком (FR-1001).
#[derive(Debug, Serialize, ToSchema)]
pub struct LedgerAccountDto {
    pub id: Uuid,
    /// `participant_fee` - взнос участника, `contract_deposit` - депозит договора
    pub kind: String,
    pub application_id: Option<Uuid>,
    pub contract_id: Option<Uuid>,
    pub owner_name: String,
    pub tender_id: Option<Uuid>,
    pub tender_title: Option<String>,
    pub lot_seq: Option<i32>,
    /// Остаток: сумма кредитов минус дебеты (INV-DB-05 не дает уйти в минус)
    #[schema(value_type = String, example = "36000.00")]
    pub balance: Decimal,
    /// Сколько должно быть на счете: у депозита - месячная плата договора
    /// (FR-1003, п. 132); у счета взноса размер задает лот (FR-206)
    #[schema(value_type = Option<String>, example = "60500.00")]
    pub required_amount: Option<Decimal>,
}

fn account_dto(row: ledger::AccountRow) -> LedgerAccountDto {
    LedgerAccountDto {
        id: row.id,
        kind: row.kind,
        application_id: row.application_id,
        contract_id: row.contract_id,
        required_amount: row.required_amount,
        owner_name: row.owner_name,
        tender_id: row.tender_id,
        tender_title: row.tender_title,
        lot_seq: row.lot_seq,
        balance: row.balance,
    }
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct AccountsParams {
    /// Фильтр по типу счета
    pub kind: Option<String>,
}

/// Депозитная книга (FR-1001): счета и остатки. Читает департамент финансов.
#[utoipa::path(
    get,
    path = "/api/v1/ledger/accounts",
    tag = "ledger",
    params(AccountsParams),
    responses((status = 200, description = "Счета книги", body = [LedgerAccountDto]))
)]
pub async fn list_accounts(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(params): Query<AccountsParams>,
) -> Result<Json<Vec<LedgerAccountDto>>, ApiError> {
    user.require(Action::LedgerRead)?;

    let kind = match params.kind.as_deref() {
        None | Some("") => None,
        Some("participant_fee") => Some(AccountKind::ParticipantFee),
        Some("contract_deposit") => Some(AccountKind::ContractDeposit),
        Some(other) => {
            return Err(ApiError::bad_request(format!(
                "неизвестный тип счета: {other}"
            )));
        }
    };

    let rows = ledger::accounts(&state.db, kind).await?;
    Ok(Json(rows.into_iter().map(account_dto).collect()))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LedgerEntryDto {
    pub id: Uuid,
    /// Операция книги (`receipt_confirmed`, `hold`, `offset`, …)
    pub op: String,
    #[schema(value_type = String, example = "0.00")]
    pub debit: Decimal,
    #[schema(value_type = String, example = "36000.00")]
    pub credit: Decimal,
    /// Пункт Правил - основание проводки
    pub rule_ref: Option<String>,
    /// Код основания возврата (только у `refund`, п. 26)
    pub refund_reason: Option<String>,
    /// Дата фактического поступления денег (только у `receipt_confirmed`)
    #[serde(with = "crate::dto::iso_date::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub paid_at: Option<time::Date>,
    pub note: Option<String>,
    pub recorded_by_name: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub occurred_at: OffsetDateTime,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct EntriesPageParams {
    /// Курсор следующей страницы - значение `next_after` предыдущей
    pub after: Option<String>,
    pub limit: Option<i64>,
}

/// Страница выписки по счету (ТЗ § 7).
#[derive(Debug, Serialize, ToSchema)]
pub struct LedgerEntryPage {
    pub items: Vec<LedgerEntryDto>,
    /// Курсор продолжения; `null` - журнал показан до конца
    pub next_after: Option<String>,
    /// Показана не вся выписка
    pub truncated: bool,
}

/// Выписка по счету (FR-1001): проводки двойной записи по порядку.
#[utoipa::path(
    get,
    path = "/api/v1/ledger/accounts/{id}/entries",
    tag = "ledger",
    params(("id" = Uuid, Path, description = "Счет книги"), EntriesPageParams),
    responses((status = 200, description = "Страница проводок счета", body = LedgerEntryPage))
)]
pub async fn account_entries(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<EntriesPageParams>,
) -> Result<Json<LedgerEntryPage>, ApiError> {
    user.require(Action::LedgerRead)?;

    let after = params.after.as_deref().map(cursor::parse).transpose()?;
    let limit = crate::page_limit(params.limit);

    let page = ledger::entries(&state.db, id, after, limit).await?;
    let truncated = page.truncated;
    let next_after = cursor::next(
        truncated,
        page.last()
            .map(|row| RowCursor::new(row.occurred_at, row.id)),
    );

    Ok(Json(LedgerEntryPage {
        items: page
            .into_iter()
            .map(|row| LedgerEntryDto {
                id: row.id,
                op: row.op,
                debit: row.debit,
                credit: row.credit,
                rule_ref: row.rule_ref,
                refund_reason: row.refund_reason,
                paid_at: row.paid_at,
                note: row.note,
                recorded_by_name: row.recorded_by_name,
                occurred_at: row.occurred_at,
            })
            .collect(),
        next_after,
        truncated,
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ConfirmFeeRequest {
    /// Поступившая сумма; должна совпадать с взносом лота (FR-206, п. 25)
    #[schema(value_type = String, example = "36000.00")]
    pub amount: Decimal,
    /// Дата поступления денег по выписке банка (вводит оператор)
    #[serde(with = "crate::dto::iso_date")]
    #[schema(value_type = String, format = Date)]
    pub paid_at: time::Date,
}

/// Подтверждение поступления гарантийного взноса (FR-405, п. 23).
/// Заявка получает статус «взнос подтвержден», проводка идет в книгу.
#[utoipa::path(
    post,
    path = "/api/v1/applications/{id}/fee",
    tag = "ledger",
    params(("id" = Uuid, Path, description = "Заявка")),
    request_body = ConfirmFeeRequest,
    responses(
        (status = 200, description = "Поступление подтверждено", body = LedgerAccountDto),
        (status = 409, description = "Сумма, срок или статус заявки не позволяют", body = crate::error::Problem),
    )
)]
pub async fn confirm_fee(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ConfirmFeeRequest>,
) -> Result<Json<LedgerAccountDto>, ApiError> {
    user.require(Action::FeeConfirm)?;

    let account = ledger::confirm_fee(&state.db, user.id(), id, body.amount, body.paid_at)
        .await
        .map_err(ledger_error)?;
    Ok(Json(account_dto(account)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RefundRequest {
    /// Код основания из закрытого перечня п. 26 (FR-1002)
    pub reason: String,
    pub note: Option<String>,
}

/// Возврат гарантийного взноса (FR-1002, п. 26): возвращается весь остаток
/// счета, основание - из закрытого перечня.
#[utoipa::path(
    post,
    path = "/api/v1/applications/{id}/fee/refund",
    tag = "ledger",
    params(("id" = Uuid, Path, description = "Заявка")),
    request_body = RefundRequest,
    responses(
        (status = 200, description = "Возврат оформлен", body = LedgerAccountDto),
        (status = 409, description = "Возвращать нечего", body = crate::error::Problem),
        (status = 422, description = "Основание вне перечня п. 26", body = crate::error::Problem),
    )
)]
pub async fn refund_fee(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<RefundRequest>,
) -> Result<Json<LedgerAccountDto>, ApiError> {
    user.require(Action::FeeConfirm)?;

    let reason: RefundReason = body.reason.parse().map_err(|_| {
        ApiError::Validation(format!(
            "основание «{}» вне перечня п. 26 (FR-1002)",
            body.reason
        ))
    })?;

    let account = ledger::refund_fee(
        &state.db,
        user.id(),
        id,
        reason,
        body.note.as_deref().filter(|note| !note.is_empty()),
    )
    .await
    .map_err(ledger_error)?;
    Ok(Json(account_dto(account)))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RefundReasonDto {
    pub code: String,
    pub label_ru: String,
    pub label_kk: Option<String>,
    pub label_en: Option<String>,
    /// Подпункт п. 26
    pub rule_ref: String,
}

/// Закрытый перечень оснований возврата (FR-1002, п. 26).
#[utoipa::path(
    get,
    path = "/api/v1/refdata/refund-reasons",
    tag = "ledger",
    responses((status = 200, description = "Основания п. 26", body = [RefundReasonDto]))
)]
pub async fn refund_reasons(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<RefundReasonDto>>, ApiError> {
    // Закрытый перечень из Правил: по нему участник понимает судьбу взноса
    user.require(Action::RefdataRead)?;

    let rows = ledger::refund_reasons(&state.db).await?;
    Ok(Json(
        rows.into_iter()
            .map(|row| RefundReasonDto {
                code: row.code,
                label_ru: row.label_ru,
                label_kk: row.label_kk,
                label_en: row.label_en,
                rule_ref: row.rule_ref,
            })
            .collect(),
    ))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DepositRequest {
    /// Сумма платежа: обязана равняться недостающей части депозита (п. 132)
    #[schema(value_type = String, example = "60500.00")]
    pub amount: Decimal,
    /// Дата поступления денег по выписке банка (вводит оператор)
    #[serde(with = "crate::dto::iso_date")]
    #[schema(value_type = String, format = Date)]
    pub paid_at: time::Date,
    pub note: Option<String>,
}

/// Внесение депозита по договору (FR-1003, п. 132).
///
/// Депозит равен месячной плате; сумма проверяется системой, а не
/// доверяется оператору. Платеж закрывает срок внесения (п. 132).
#[utoipa::path(
    post,
    path = "/api/v1/contracts/{id}/deposit",
    tag = "ledger",
    params(("id" = Uuid, Path, description = "Договор")),
    request_body = DepositRequest,
    responses(
        (status = 200, description = "Депозит внесен", body = LedgerAccountDto),
        (status = 404, description = "Договор не найден", body = crate::error::Problem),
        (status = 409, description = "Договор не заключен либо сумма не равна депозиту", body = crate::error::Problem),
    )
)]
pub async fn pay_deposit(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<DepositRequest>,
) -> Result<Json<LedgerAccountDto>, ApiError> {
    user.require(Action::FeeConfirm)?;

    let account = ledger::pay_deposit(
        &state.db,
        user.id(),
        id,
        body.amount,
        body.paid_at,
        body.note.as_deref().filter(|note| !note.trim().is_empty()),
    )
    .await
    .map_err(ledger_error)?;
    Ok(Json(account_dto(account)))
}

/// Возврат депозита после возврата объекта (FR-1003, п. 136).
#[utoipa::path(
    post,
    path = "/api/v1/contracts/{id}/deposit/refund",
    tag = "ledger",
    params(("id" = Uuid, Path, description = "Договор")),
    request_body = RefundNote,
    responses(
        (status = 200, description = "Депозит возвращен", body = LedgerAccountDto),
        (status = 404, description = "Депозитный счет не найден", body = crate::error::Problem),
        (status = 409, description = "Объект не возвращен либо остатка нет", body = crate::error::Problem),
    )
)]
pub async fn refund_deposit(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<RefundNote>,
) -> Result<Json<LedgerAccountDto>, ApiError> {
    user.require(Action::FeeConfirm)?;

    let account = ledger::refund_deposit(
        &state.db,
        user.id(),
        id,
        body.note.as_deref().filter(|note| !note.trim().is_empty()),
    )
    .await
    .map_err(ledger_error)?;
    Ok(Json(account_dto(account)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RefundNote {
    pub note: Option<String>,
}

/// Депозитный счет договора (FR-1003): наниматель видит свой, финблок -
/// любой, ведущие процесс - в карточке договора.
#[utoipa::path(
    get,
    path = "/api/v1/contracts/{id}/deposit",
    tag = "ledger",
    params(("id" = Uuid, Path, description = "Договор")),
    responses(
        (status = 200, description = "Депозитный счет", body = LedgerAccountDto),
        (status = 404, description = "Счет не открыт", body = crate::error::Problem),
    )
)]
pub async fn contract_deposit(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<LedgerAccountDto>, ApiError> {
    let account = ledger::account_of_contract(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;

    // Свой депозит видит наниматель; чужой - финблок и ведущие процесс
    if account.owner_user_id != user.id() && user.require(Action::LedgerRead).is_err() {
        user.require(Action::ContractRead)?;
    }

    Ok(Json(account_dto(account)))
}

/// Счет взноса по заявке: участник видит свой, финблок - любой.
#[utoipa::path(
    get,
    path = "/api/v1/applications/{id}/fee",
    tag = "ledger",
    params(("id" = Uuid, Path, description = "Заявка")),
    responses(
        (status = 200, description = "Счет взноса", body = LedgerAccountDto),
        (status = 404, description = "Взнос не подтверждался", body = crate::error::Problem),
    )
)]
pub async fn application_account(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<LedgerAccountDto>, ApiError> {
    let account = ledger::account_of_application(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;

    // Свой счет видит владелец; чужие - только департамент финансов
    if account.owner_user_id != user.id() {
        user.require(Action::LedgerRead)?;
    }

    Ok(Json(account_dto(account)))
}
