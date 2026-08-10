//! Экстрактор текущего пользователя и проверка политики (INV-POL-01).

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use tou_db::users::{self, UserRecord};
use tou_domain::policy::{Action, is_allowed};
use tou_domain::role::Role;
use tower_sessions::Session;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

/// Ключ user_id в сессии (tower-sessions, Redis).
pub const SESSION_USER_KEY: &str = "user_id";

/// Аутентифицированный пользователь: сессия → строка БД + роли.
/// Роли читаются на каждый запрос - отзыв роли действует немедленно (FR-1503).
pub struct CurrentUser {
    pub record: UserRecord,
    pub roles: Vec<Role>,
}

impl CurrentUser {
    pub fn id(&self) -> Uuid {
        self.record.id
    }

    /// Политика доступа: хотя бы одна роль пользователя разрешает действие.
    pub fn require(&self, action: Action) -> Result<(), ApiError> {
        if self.roles.iter().any(|role| is_allowed(*role, action)) {
            Ok(())
        } else {
            Err(ApiError::Forbidden)
        }
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|(_, msg)| ApiError::internal(std::io::Error::other(msg)))?;

        let user_id: Uuid = session
            .get(SESSION_USER_KEY)
            .await
            .map_err(ApiError::internal)?
            .ok_or(ApiError::Unauthorized)?;

        let record = users::find_by_id(&state.db, user_id)
            .await?
            .filter(|user| user.is_active)
            .ok_or(ApiError::Unauthorized)?;
        let roles = users::roles_of(&state.db, user_id).await?;

        Ok(CurrentUser { record, roles })
    }
}

/// `Option<CurrentUser>` в хендлерах публичных маршрутов: аноним → None,
/// реальные ошибки инфраструктуры пробрасываются.
impl axum::extract::OptionalFromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Option<Self>, Self::Rejection> {
        match <Self as FromRequestParts<AppState>>::from_request_parts(parts, state).await {
            Ok(user) => Ok(Some(user)),
            Err(ApiError::Unauthorized) => Ok(None),
            Err(other) => Err(other),
        }
    }
}
