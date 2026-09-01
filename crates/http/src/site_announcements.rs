//! Публичное объявление на главной странице и его администрирование.

use axum::extract::State;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tou_db::site_announcements::SiteAnnouncementRecord;
use tou_domain::policy::Action;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::request::Json;
use crate::state::AppState;

#[derive(Debug, Serialize, ToSchema)]
pub struct SiteAnnouncementDto {
    pub id: Uuid,
    pub title: String,
    pub title_kk: String,
    pub body: String,
    pub body_kk: String,
    pub is_published: bool,
    #[schema(value_type = Option<String>, format = DateTime)]
    pub published_at: Option<OffsetDateTime>,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
}

impl From<SiteAnnouncementRecord> for SiteAnnouncementDto {
    fn from(record: SiteAnnouncementRecord) -> Self {
        Self {
            id: record.id,
            title: record.title,
            title_kk: record.title_kk,
            body: record.body,
            body_kk: record.body_kk,
            is_published: record.is_published,
            published_at: record.published_at,
            updated_at: record.updated_at,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveSiteAnnouncementRequest {
    pub title: String,
    pub title_kk: String,
    pub body: String,
    pub body_kk: String,
    pub is_published: bool,
}

fn validated_text(value: String, name: &str, max: usize) -> Result<String, ApiError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > max {
        return Err(ApiError::Validation(format!(
            "{name}: требуется от 1 до {max} символов"
        )));
    }
    Ok(trimmed.to_owned())
}

/// Опубликованное объявление. Скрытый черновик для гостя не существует.
#[utoipa::path(
    get,
    path = "/api/v1/site-announcement",
    tag = "site-announcements",
    responses(
        (status = 200, description = "Опубликованное объявление", body = SiteAnnouncementDto),
        (status = 404, description = "Объявления нет или оно скрыто", body = crate::error::Problem),
    )
)]
pub async fn published(
    State(state): State<AppState>,
) -> Result<Json<SiteAnnouncementDto>, ApiError> {
    let record = tou_db::site_announcements::published(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(record.into()))
}

/// Текущее объявление для формы администратора, включая скрытое.
#[utoipa::path(
    get,
    path = "/api/v1/admin/site-announcement",
    tag = "admin",
    responses(
        (status = 200, description = "Объявление для редактирования", body = SiteAnnouncementDto),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
        (status = 404, description = "Объявление еще не создано", body = crate::error::Problem),
    )
)]
pub async fn current(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<SiteAnnouncementDto>, ApiError> {
    user.require(Action::SiteAnnouncementManage)?;
    let record = tou_db::site_announcements::current(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(record.into()))
}

/// Создание, изменение, публикация и скрытие объявления одной операцией.
#[utoipa::path(
    put,
    path = "/api/v1/admin/site-announcement",
    tag = "admin",
    request_body = SaveSiteAnnouncementRequest,
    responses(
        (status = 200, description = "Объявление сохранено", body = SiteAnnouncementDto),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
        (status = 422, description = "Некорректный заголовок или текст", body = crate::error::Problem),
    )
)]
pub async fn save(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<SaveSiteAnnouncementRequest>,
) -> Result<Json<SiteAnnouncementDto>, ApiError> {
    user.require(Action::SiteAnnouncementManage)?;
    let title = validated_text(body.title, "title", 200)?;
    let title_kk = validated_text(body.title_kk, "title_kk", 200)?;
    let announcement_body = validated_text(body.body, "body", 20_000)?;
    let announcement_body_kk = validated_text(body.body_kk, "body_kk", 20_000)?;
    let record = tou_db::site_announcements::save(
        &state.db,
        user.id(),
        &title,
        &title_kk,
        &announcement_body,
        &announcement_body_kk,
        body.is_published,
    )
    .await?;
    Ok(Json(record.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn announcement_text_is_trimmed_and_bounded() {
        assert_eq!(
            validated_text("  text  ".to_owned(), "body", 10).unwrap(),
            "text"
        );
        assert!(validated_text("   ".to_owned(), "body", 10).is_err());
        assert!(validated_text("12345678901".to_owned(), "body", 10).is_err());
    }
}
