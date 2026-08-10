//! Сроки процесса (М17, FR-1701–1702): дашборд «мои сроки» и справочник
//! производственного календаря.
//!
//! Обязательства ставит и закрывает сам процесс (db-слой на событиях), здесь
//! только чтение и правка календаря админом.

use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tou_db::obligations;
use tou_domain::policy::Action;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::request::{Json, Path};
use crate::state::AppState;

#[derive(Debug, Serialize, ToSchema)]
pub struct ObligationDto {
    pub id: Uuid,
    /// Машинный код действия (`admission_protocol`, `notify_admitted`, …)
    pub action: String,
    /// Пункт Правил, из которого взят срок
    pub rule_ref: String,
    /// Роль-исполнитель (snake_case, enum `Role`)
    pub assignee_role: String,
    pub tender_id: Option<Uuid>,
    pub tender_title: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub due_at: OffsetDateTime,
    /// `pending` | `overdue` (исполненные в дашборд не попадают)
    pub status: String,
}

/// «Мои сроки» (FR-1702): открытые обязательства ролей пользователя.
#[utoipa::path(
    get,
    path = "/api/v1/obligations/my",
    tag = "obligations",
    responses((status = 200, description = "Открытые сроки", body = [ObligationDto]))
)]
pub async fn my_obligations(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<ObligationDto>>, ApiError> {
    let records = obligations::for_roles(&state.db, &user.roles).await?;

    Ok(Json(
        records
            .into_iter()
            .map(|record| ObligationDto {
                id: record.id,
                action: record.action,
                rule_ref: record.rule_ref,
                assignee_role: record.assignee_role.as_str().to_owned(),
                tender_id: record.tender_id,
                tender_title: record.tender_title,
                due_at: record.due_at,
                status: record.status,
            })
            .collect(),
    ))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct HolidayDto {
    #[serde(with = "crate::dto::iso_date")]
    #[schema(value_type = String, format = Date)]
    pub day: time::Date,
    pub label_ru: String,
}

/// Производственный календарь (FR-1701): его читают расчеты сроков.
#[utoipa::path(
    get,
    path = "/api/v1/refdata/holidays",
    tag = "obligations",
    responses((status = 200, description = "Праздничные дни", body = [HolidayDto]))
)]
pub async fn list_holidays(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<HolidayDto>>, ApiError> {
    let rows = obligations::holidays(&state.db).await?;
    Ok(Json(
        rows.into_iter()
            .map(|(day, label_ru)| HolidayDto { day, label_ru })
            .collect(),
    ))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct HolidayRequest {
    #[serde(with = "crate::dto::iso_date")]
    #[schema(value_type = String, format = Date)]
    pub day: time::Date,
    pub label_ru: String,
}

/// Правка календаря админом (FR-1701): от него зависят все «рабочие дни»
/// Правил, поэтому изменение - действие роли admin и пишется в аудит.
#[utoipa::path(
    post,
    path = "/api/v1/refdata/holidays",
    tag = "obligations",
    request_body = HolidayRequest,
    responses(
        (status = 204, description = "День добавлен"),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
        (status = 422, description = "Пустое наименование", body = crate::error::Problem),
    )
)]
pub async fn add_holiday(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<HolidayRequest>,
) -> Result<StatusCode, ApiError> {
    user.require(Action::RefdataManage)?;

    let label = body.label_ru.trim();
    if label.is_empty() {
        return Err(ApiError::Validation(
            "у праздничного дня должно быть наименование".to_owned(),
        ));
    }

    obligations::add_holiday(&state.db, user.id(), body.day, label).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Удаление дня из календаря (FR-1701).
#[utoipa::path(
    delete,
    path = "/api/v1/refdata/holidays/{day}",
    tag = "obligations",
    params(("day" = String, Path, description = "Дата в формате YYYY-MM-DD")),
    responses(
        (status = 204, description = "День удален"),
        (status = 404, description = "Такого дня в календаре нет", body = crate::error::Problem),
    )
)]
pub async fn remove_holiday(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(day): Path<String>,
) -> Result<StatusCode, ApiError> {
    user.require(Action::RefdataManage)?;

    let day = time::Date::parse(
        &day,
        &time::macros::format_description!("[year]-[month]-[day]"),
    )
    .map_err(|_| ApiError::bad_request("дата ожидается в формате YYYY-MM-DD"))?;

    if obligations::remove_holiday(&state.db, user.id(), day).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}
