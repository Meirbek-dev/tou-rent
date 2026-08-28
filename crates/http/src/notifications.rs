//! Центр уведомлений (М13, FR-1301–1302): история, счетчик непрочитанных,
//! отметка о прочтении, SSE-стрим.
//!
//! Запись в `core.notifications` - доказательная база (FR-1302, audit-триггер
//! INV-AUDIT); SSE доставляет событие открытым стримам получателя ≤1 с
//! (критерий Т10). Типы событий - enum `tou_domain::notification`.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::BroadcastStream;
use tou_db::notifications;
use tou_db::notifications::NotificationRecord;
use tou_domain::policy::Action;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::request::{Json, Query};
use crate::state::AppState;

#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationDto {
    pub id: Uuid,
    /// Тип события - имя варианта `NotificationKind` (напр. `auction_invitation`)
    pub kind: String,
    /// Данные события; состав определяет тип (FR-504: стартовая ставка,
    /// дата торгов и т.д.), локализацию выполняет клиент
    pub payload: serde_json::Value,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub read_at: Option<OffsetDateTime>,
}

impl NotificationDto {
    pub fn from_record(record: NotificationRecord) -> Self {
        Self {
            id: record.id,
            kind: record.kind,
            payload: record.payload,
            created_at: record.created_at,
            read_at: record.read_at,
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListNotificationsParams {
    pub after: Option<Uuid>,
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NotificationPage {
    pub items: Vec<NotificationDto>,
    pub next_after: Option<Uuid>,
}

/// История уведомлений получателя (FR-1301), новые сверху.
#[utoipa::path(
    get,
    path = "/api/v1/notifications",
    tag = "notifications",
    params(ListNotificationsParams),
    responses((status = 200, description = "Страница уведомлений", body = NotificationPage))
)]
pub async fn list_notifications(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(params): Query<ListNotificationsParams>,
) -> Result<Json<NotificationPage>, ApiError> {
    user.require(Action::NotificationReadOwn)?;

    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    let records = notifications::list_for_user(&state.db, user.id(), params.after, limit).await?;

    let next_after = (records.len() as i64 == limit)
        .then(|| records.last().map(|r| r.id))
        .flatten();

    Ok(Json(NotificationPage {
        items: records
            .into_iter()
            .map(NotificationDto::from_record)
            .collect(),
        next_after,
    }))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UnreadCountDto {
    pub count: i64,
}

/// Счетчик непрочитанных для колокольчика (FR-1301).
#[utoipa::path(
    get,
    path = "/api/v1/notifications/unread-count",
    tag = "notifications",
    responses((status = 200, description = "Число непрочитанных", body = UnreadCountDto))
)]
pub async fn unread_count(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<UnreadCountDto>, ApiError> {
    user.require(Action::NotificationReadOwn)?;
    let count = notifications::unread_count(&state.db, user.id()).await?;
    Ok(Json(UnreadCountDto { count }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MarkReadRequest {
    /// Конкретные уведомления; отсутствие поля - прочитать все
    pub ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MarkReadResponse {
    pub updated: u64,
}

/// Отметка о прочтении своих уведомлений (все или перечисленные).
#[utoipa::path(
    post,
    path = "/api/v1/notifications/read",
    tag = "notifications",
    request_body = MarkReadRequest,
    responses((status = 200, description = "Отмечено прочитанными", body = MarkReadResponse))
)]
pub async fn mark_read(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<MarkReadRequest>,
) -> Result<Json<MarkReadResponse>, ApiError> {
    user.require(Action::NotificationReadOwn)?;
    let updated = notifications::mark_read(&state.db, user.id(), body.ids.as_deref()).await?;
    Ok(Json(MarkReadResponse { updated }))
}

/// SSE-стрим событий получателя (FR-1301, доставка ≤1 с): каждое событие -
/// `event: notification` с JSON `NotificationDto` в `data`. Keep-alive
/// комментарии удерживают соединение через прокси.
#[utoipa::path(
    get,
    path = "/api/v1/notifications/stream",
    tag = "notifications",
    responses((status = 200, description = "text/event-stream событий получателя",
        content_type = "text/event-stream", body = String))
)]
pub async fn stream(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    user.require(Action::NotificationReadOwn)?;
    let user_id = user.id();

    let events = BroadcastStream::new(state.notifier.subscribe()).filter_map(move |message| {
        match message {
            Ok(event) if event.user_id == user_id => Some(Ok(Event::default()
                .event("notification")
                .data(event.json.as_ref()))),
            // Чужие события и Lagged (переполнение буфера): пропуск -
            // пропущенное клиент дотянет запросом истории
            _ => None,
        }
    });

    // Axum's default heartbeat is 15 s, while Bun closes an idle request after
    // 10 s in the local dev proxy. Keep the stream active before that limit so
    // EventSource is not continuously disconnected and reconnected.
    Ok(Sse::new(events).keep_alive(KeepAlive::new().interval(Duration::from_secs(5))))
}
