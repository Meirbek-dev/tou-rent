//! Администрирование пользователей и ролей (FR-1503, М15).
//! Каждое изменение ролей фиксирует audit-триггер `core.role_grants` (INV-AUDIT).

use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use tou_domain::policy::Action;
use tou_domain::role::Role;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::auth::UserDto;
use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::request::{Json, Path, Query};
use crate::state::AppState;

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListUsersParams {
    /// Cursor: id последнего элемента предыдущей страницы (uuid v7 упорядочен по времени)
    pub after: Option<Uuid>,
    /// 1..=100, по умолчанию 50
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserPage {
    pub items: Vec<UserDto>,
    /// Cursor для следующей страницы; None - данных больше нет
    pub next_after: Option<Uuid>,
}

/// Список пользователей (cursor-пагинация, ТЗ § 7).
#[utoipa::path(
    get,
    path = "/api/v1/admin/users",
    tag = "admin",
    params(ListUsersParams),
    responses(
        (status = 200, description = "Страница пользователей", body = UserPage),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
    )
)]
pub async fn list_users(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(params): Query<ListUsersParams>,
) -> Result<Json<UserPage>, ApiError> {
    user.require(Action::UserManage)?;

    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    let records = tou_db::users::list_users(&state.db, params.after, limit).await?;

    let next_after = (records.len() as i64 == limit)
        .then(|| records.last().map(|u| u.id))
        .flatten();

    // Роли всей страницы одним запросом (без N+1)
    let ids: Vec<Uuid> = records.iter().map(|u| u.id).collect();
    let mut roles_by_user = tou_db::users::roles_for(&state.db, &ids).await?;

    let items = records
        .into_iter()
        .map(|record| {
            let roles = roles_by_user.remove(&record.id).unwrap_or_default();
            UserDto::new(record, roles)
        })
        .collect();

    Ok(Json(UserPage { items, next_after }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GrantRoleRequest {
    /// Роль из enum домена; `guest` не хранится и не назначается
    #[schema(value_type = String, example = "organizer")]
    pub role: String,
}

/// Назначить роль пользователю (FR-1503).
#[utoipa::path(
    post,
    path = "/api/v1/admin/users/{user_id}/roles",
    tag = "admin",
    params(("user_id" = Uuid, Path, description = "Пользователь")),
    request_body = GrantRoleRequest,
    responses(
        (status = 204, description = "Роль назначена (или уже была)"),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
        (status = 404, description = "Пользователь не найден", body = crate::error::Problem),
        (status = 422, description = "Неизвестная роль", body = crate::error::Problem),
    )
)]
pub async fn grant_role(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Json(body): Json<GrantRoleRequest>,
) -> Result<StatusCode, ApiError> {
    user.require(Action::RoleGrant)?;
    let role = parse_grantable_role(&body.role)?;

    tou_db::users::find_by_id(&state.db, user_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    tou_db::users::grant_role(&state.db, user.id(), user_id, role).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Отозвать роль (FR-1503).
#[utoipa::path(
    delete,
    path = "/api/v1/admin/users/{user_id}/roles/{role}",
    tag = "admin",
    params(
        ("user_id" = Uuid, Path, description = "Пользователь"),
        ("role" = String, Path, description = "Роль (snake_case)"),
    ),
    responses(
        (status = 204, description = "Роль отозвана (или ее не было)"),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
        (status = 422, description = "Неизвестная роль", body = crate::error::Problem),
    )
)]
pub async fn revoke_role(
    user: CurrentUser,
    State(state): State<AppState>,
    Path((user_id, role)): Path<(Uuid, String)>,
) -> Result<StatusCode, ApiError> {
    user.require(Action::RoleGrant)?;
    let role = parse_grantable_role(&role)?;
    tou_db::users::revoke_role(&state.db, user.id(), user_id, role).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_grantable_role(raw: &str) -> Result<Role, ApiError> {
    let role: Role = raw
        .parse()
        .map_err(|_| ApiError::Validation(format!("неизвестная роль: {raw}")))?;
    if role == Role::Guest {
        return Err(ApiError::Validation(
            "роль guest не назначается - это аноним".into(),
        ));
    }
    Ok(role)
}
