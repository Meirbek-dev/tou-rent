//! Правка сроков тендера из кабинета админа (М15, FR-1503; след - FR-1601).
//!
//! Штатный путь переноса сроков - редакция документации (FR-304, п. 27):
//! она только продлевает прием заявок, не позже чем за два дня до его
//! окончания и не меньше чем на десять, и извещает участников. Дату
//! публикации она не трогает вовсе - ту ставит сервер в момент объявления
//! (NFR-03). Когда объявление на сайте университета вышло раньше, чем
//! тендер завели на стенде, и сроки в объявлении и на стенде разошлись,
//! этим путем расхождение не исправить: стенд обязан показывать то, что
//! опубликовано, и остается правка самой записи.
//!
//! Правка записи - не ход процедуры: участники не извещаются, назначенные
//! торги и сроки обязательств не переносятся, редакция документации не
//! публикуется. Возможность переставить юридически значимую отметку закрыта
//! теми же тремя рубежами, что очистка данных и сдвиг часов (ADR-0005):
//! право `DataPurge` одного admin, намерение `ALLOW_DATA_PURGE` в окружении
//! api, след в аудите (триггер `core.tenders`). Порядок отметок стережет
//! домен (`tender::Schedule`), согласие с фактом вскрытия - он же и CHECK
//! таблицы.

use axum::extract::State;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tou_db::tenders::{ScheduleFields, TenderRecord};
use tou_domain::amendment::{Instant, instant};
use tou_domain::policy::Action;
use tou_domain::rule::RuleViolation;
use tou_domain::tender::{Schedule, ScheduleFacts, TenderStatus};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::admin_data::require_purge_enabled;
use crate::dto::TenderStatusDto;
use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::request::{Json, Path};
use crate::state::AppState;
use crate::tenders::transition_error;

/// Тендер со сроками глазами админа: заголовок, статус и все отметки,
/// включая факт вскрытия, который правке не подлежит.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminTenderScheduleDto {
    pub id: Uuid,
    pub title: String,
    pub title_kk: String,
    pub status: TenderStatusDto,
    /// Публикация объявления (п. 5–6)
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub announced_at: Option<OffsetDateTime>,
    /// Окончание приема заявок (п. 36–39)
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub submission_deadline: Option<OffsetDateTime>,
    /// Назначенное вскрытие конвертов (п. 50)
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub opening_at: Option<OffsetDateTime>,
    /// Факт вскрытия секретарем (FR-403) - правке не подлежит
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub opened_at: Option<OffsetDateTime>,
    /// Торги (п. 59, 62)
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub trading_at: Option<OffsetDateTime>,
}

impl AdminTenderScheduleDto {
    fn from_record(r: TenderRecord) -> Result<Self, ApiError> {
        Ok(Self {
            id: r.id,
            title: r.title,
            title_kk: r.title_kk,
            status: TenderStatusDto::from_db(&r.status)?,
            announced_at: r.announced_at,
            submission_deadline: r.submission_deadline,
            opening_at: r.opening_at,
            opened_at: r.opened_at,
            trading_at: r.trading_at,
        })
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminTenderSchedulePageDto {
    pub items: Vec<AdminTenderScheduleDto>,
    /// Показаны не все тендеры: перечень обрезан потолком строк
    pub truncated: bool,
}

/// Новые сроки целиком. Поле без значения - «отметка не назначена», а не
/// «оставить как есть»: кабинет подставляет текущие значения, поэтому
/// стереть отметку можно только нарочно, а не забыв поле.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AdminTenderScheduleRequest {
    #[serde(default, with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub announced_at: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub submission_deadline: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub opening_at: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub trading_at: Option<OffsetDateTime>,
}

/// Момент домена из отметки БД: порядок сроков сверяется с точностью до
/// секунды, как и окно редакции документации.
fn to_instant(value: OffsetDateTime) -> Instant {
    instant(value.unix_timestamp())
}

/// Все тендеры стенда со сроками, свежие сверху, - что можно поправить.
#[utoipa::path(
    get,
    path = "/api/v1/admin/tenders/schedule",
    tag = "admin",
    responses(
        (status = 200, description = "Тендеры со сроками", body = AdminTenderSchedulePageDto),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
    )
)]
pub async fn list_schedules(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<AdminTenderSchedulePageDto>, ApiError> {
    user.require(Action::DataPurge)?;

    let page = tou_db::tenders::list_all(&state.db).await?;
    let truncated = page.truncated;
    let items = page
        .into_iter()
        .map(AdminTenderScheduleDto::from_record)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(AdminTenderSchedulePageDto { items, truncated }))
}

/// Правка сроков тендера в обход процедуры: все четыре отметки, включая дату
/// публикации, в любом статусе. Порядок отметок и обязательность первых трех
/// у опубликованного тендера проверяет домен (FR-303); участники об изменении
/// не узнают - это исправление записи, а не редакция документации (FR-304).
#[utoipa::path(
    put,
    path = "/api/v1/admin/tenders/{id}/schedule",
    tag = "admin",
    request_body = AdminTenderScheduleRequest,
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 200, description = "Сроки изменены", body = AdminTenderScheduleDto),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
        (status = 404, description = "Тендер не найден", body = crate::error::Problem),
        (status = 409, description = "Сроки не в порядке процедуры (FR-303)",
         body = crate::error::Problem),
        (status = 422, description = "Правка данных выключена (ALLOW_DATA_PURGE)",
         body = crate::error::Problem),
    )
)]
pub async fn set_schedule(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AdminTenderScheduleRequest>,
) -> Result<Json<AdminTenderScheduleDto>, ApiError> {
    user.require(Action::DataPurge)?;
    require_purge_enabled(&state)?;

    let current = tou_db::tenders::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let status = TenderStatus::parse(&current.status).ok_or_else(|| {
        ApiError::internal(std::io::Error::other(format!(
            "tender_status: неизвестное значение БД '{}' - рассинхрон enum'ов",
            current.status
        )))
    })?;

    let schedule = Schedule {
        announced_at: body.announced_at.map(to_instant),
        submission_deadline: body.submission_deadline.map(to_instant),
        opening_at: body.opening_at.map(to_instant),
        trading_at: body.trading_at.map(to_instant),
    };
    schedule
        .validate(ScheduleFacts {
            status,
            opened_at: current.opened_at.map(to_instant),
        })
        .map_err(|err| ApiError::rule(RuleViolation::TenderScheduleOrder, err.to_string()))?;

    let updated = tou_db::tenders::set_schedule(
        &state.db,
        user.id(),
        id,
        ScheduleFields {
            announced_at: body.announced_at,
            submission_deadline: body.submission_deadline,
            opening_at: body.opening_at,
            trading_at: body.trading_at,
        },
    )
    .await
    .map_err(transition_error)?
    .ok_or(ApiError::NotFound)?;

    tracing::warn!(
        actor = %user.id(),
        tender_id = %id,
        status = %updated.status,
        announced_at = ?updated.announced_at,
        submission_deadline = ?updated.submission_deadline,
        opening_at = ?updated.opening_at,
        trading_at = ?updated.trading_at,
        "сроки тендера изменены администратором в обход процедуры"
    );
    Ok(Json(AdminTenderScheduleDto::from_record(updated)?))
}

#[cfg(test)]
mod tests {
    /// Маршруты правки сроков зарегистрированы в контракте: путь в документе
    /// означает и работающий маршрут, и то, что кабинет соберет запрос
    /// после кодогена (G5).
    #[test]
    fn schedule_routes_are_registered_in_the_contract() {
        let json = crate::openapi().to_json().expect("сериализация контракта");
        for path in [
            "/api/v1/admin/tenders/schedule",
            "/api/v1/admin/tenders/{id}/schedule",
        ] {
            assert!(json.contains(path), "маршрут {path} не попал в контракт");
        }
        for schema in [
            "AdminTenderScheduleDto",
            "AdminTenderSchedulePageDto",
            "AdminTenderScheduleRequest",
        ] {
            assert!(json.contains(schema), "схема {schema} не попала в контракт");
        }
        assert!(
            json.contains("tender_schedule_order"),
            "причина отказа по порядку сроков не попала в перечень Rule"
        );
    }

    /// Поле без значения читается как «не назначено»: тело `{}` допустимо,
    /// и `null` - тоже. Так кабинет может стереть дату торгов, которую
    /// назначили по ошибке.
    #[test]
    fn request_fields_default_to_unset() {
        let empty: super::AdminTenderScheduleRequest =
            serde_json::from_str("{}").expect("пустое тело");
        assert!(empty.announced_at.is_none() && empty.trading_at.is_none());

        let explicit: super::AdminTenderScheduleRequest =
            serde_json::from_str(r#"{"announced_at":"2026-08-31T13:06:00Z","trading_at":null}"#)
                .expect("тело с null");
        assert!(explicit.announced_at.is_some());
        assert!(explicit.trading_at.is_none());
    }
}
