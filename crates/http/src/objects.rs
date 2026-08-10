//! Реестр объектов имущества (М1): публичное чтение (п. 6),
//! управление - organizer (INV-POL-01).

use axum::extract::State;
use axum::http::StatusCode;
use garde::Validate as _;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tou_db::objects::{self, DeleteObjectError, ObjectFields, ObjectRecord};
use tou_domain::policy::Action;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::dto::{ObjectKindDto, ObjectStatusDto};
use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::request::{Json, Path, Query};
use crate::state::AppState;
use tou_domain::rule::RuleViolation;

#[derive(Debug, Serialize, ToSchema)]
pub struct ObjectDto {
    pub id: Uuid,
    pub kind: ObjectKindDto,
    pub name: String,
    pub address: String,
    #[schema(value_type = String, example = "42.00")]
    pub area_m2: Decimal,
    pub floor_part: Option<String>,
    pub premises_type_code: Option<String>,
    pub premises_kind_code: Option<String>,
    pub comfort_code: Option<String>,
    pub location_code: Option<String>,
    pub photo_keys: Vec<String>,
    /// Вычисляется из тендеров и договоров (FR-103)
    pub status: ObjectStatusDto,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
}

impl ObjectDto {
    pub(crate) fn from_record(r: ObjectRecord) -> Result<Self, ApiError> {
        Ok(Self {
            id: r.id,
            kind: ObjectKindDto::from_db(&r.kind)?,
            status: ObjectStatusDto::from_db(&r.status)?,
            name: r.name,
            address: r.address,
            area_m2: r.area_m2,
            floor_part: r.floor_part,
            premises_type_code: r.premises_type_code,
            premises_kind_code: r.premises_kind_code,
            comfort_code: r.comfort_code,
            location_code: r.location_code,
            photo_keys: r.photo_keys,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
    }
}

/// Создание и полное обновление объекта (FR-101).
#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct ObjectRequest {
    #[garde(skip)]
    pub kind: ObjectKindDto,
    #[garde(length(chars, min = 1, max = 300))]
    pub name: String,
    #[garde(length(chars, min = 1, max = 500))]
    pub address: String,
    /// Площадь, м² (> 0)
    #[garde(custom(positive_area))]
    #[schema(value_type = String, example = "42.00")]
    pub area_m2: Decimal,
    #[garde(inner(length(chars, max = 100)))]
    pub floor_part: Option<String>,
    /// Коды опций Прил. 4 для коэффициентов (FR-101)
    #[garde(inner(length(chars, max = 50)))]
    pub premises_type_code: Option<String>,
    #[garde(inner(length(chars, max = 50)))]
    pub premises_kind_code: Option<String>,
    #[garde(inner(length(chars, max = 50)))]
    pub comfort_code: Option<String>,
    #[garde(inner(length(chars, max = 50)))]
    pub location_code: Option<String>,
}

fn positive_area(value: &Decimal, _ctx: &()) -> garde::Result {
    if *value > Decimal::ZERO {
        Ok(())
    } else {
        Err(garde::Error::new("площадь должна быть > 0"))
    }
}

impl ObjectRequest {
    fn as_fields(&self) -> ObjectFields<'_> {
        ObjectFields {
            kind: self.kind.as_db(),
            name: &self.name,
            address: &self.address,
            area_m2: self.area_m2,
            floor_part: self.floor_part.as_deref(),
            premises_type_code: self.premises_type_code.as_deref(),
            premises_kind_code: self.premises_kind_code.as_deref(),
            comfort_code: self.comfort_code.as_deref(),
            location_code: self.location_code.as_deref(),
        }
    }
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListObjectsParams {
    /// Cursor: id последнего элемента предыдущей страницы
    pub after: Option<Uuid>,
    /// 1..=100, по умолчанию 50
    pub limit: Option<i64>,
    /// Витрина FR-102: статус объекта (вычисляемый, FR-103)
    pub status: Option<ObjectStatusDto>,
    /// Витрина FR-102: вид имущества
    pub kind: Option<ObjectKindDto>,
    /// Поиск по названию и адресу
    pub q: Option<String>,
    /// Площадь от, м²
    #[param(value_type = Option<String>, example = "20")]
    pub area_min: Option<Decimal>,
    /// Площадь до, м²
    #[param(value_type = Option<String>, example = "200")]
    pub area_max: Option<Decimal>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ObjectPage {
    pub items: Vec<ObjectDto>,
    pub next_after: Option<Uuid>,
}

/// Публичный реестр объектов (п. 6; FR-102 витрина - фильтры в Т16).
#[utoipa::path(
    get,
    path = "/api/v1/objects",
    tag = "objects",
    params(ListObjectsParams),
    responses((status = 200, description = "Страница объектов", body = ObjectPage))
)]
pub async fn list_objects(
    State(state): State<AppState>,
    Query(params): Query<ListObjectsParams>,
) -> Result<Json<ObjectPage>, ApiError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    // Пустая строка из нативной GET-формы (NFR-04) - это «не фильтровать»
    let query = params
        .q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let records = objects::list(
        &state.db,
        params.after,
        limit,
        objects::ObjectFilter {
            status: params.status.map(ObjectStatusDto::as_db),
            kind: params.kind.map(ObjectKindDto::as_db),
            query,
            area_min: params.area_min,
            area_max: params.area_max,
        },
    )
    .await?;

    let next_after = (records.len() as i64 == limit)
        .then(|| records.last().map(|r| r.id))
        .flatten();
    let items = records
        .into_iter()
        .map(ObjectDto::from_record)
        .collect::<Result<_, _>>()?;
    Ok(Json(ObjectPage { items, next_after }))
}

/// Карточка объекта (публично).
#[utoipa::path(
    get,
    path = "/api/v1/objects/{id}",
    tag = "objects",
    params(("id" = Uuid, Path, description = "Объект")),
    responses(
        (status = 200, description = "Объект", body = ObjectDto),
        (status = 404, description = "Не найден", body = crate::error::Problem),
    )
)]
pub async fn get_object(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ObjectDto>, ApiError> {
    let record = objects::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(ObjectDto::from_record(record)?))
}

/// Создание объекта (FR-101; organizer).
#[utoipa::path(
    post,
    path = "/api/v1/objects",
    tag = "objects",
    request_body = ObjectRequest,
    responses(
        (status = 201, description = "Объект создан", body = ObjectDto),
        (status = 422, description = "Данные не прошли проверку", body = crate::error::Problem),
    )
)]
pub async fn create_object(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<ObjectRequest>,
) -> Result<(StatusCode, Json<ObjectDto>), ApiError> {
    user.require(Action::ObjectManage)?;
    body.validate()
        .map_err(|r| ApiError::Validation(r.to_string()))?;

    let record = objects::insert(&state.db, user.id(), body.as_fields()).await?;
    Ok((StatusCode::CREATED, Json(ObjectDto::from_record(record)?)))
}

/// Полное обновление объекта (FR-101; organizer).
#[utoipa::path(
    put,
    path = "/api/v1/objects/{id}",
    tag = "objects",
    params(("id" = Uuid, Path, description = "Объект")),
    request_body = ObjectRequest,
    responses(
        (status = 200, description = "Объект обновлен", body = ObjectDto),
        (status = 404, description = "Не найден", body = crate::error::Problem),
    )
)]
pub async fn update_object(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ObjectRequest>,
) -> Result<Json<ObjectDto>, ApiError> {
    user.require(Action::ObjectManage)?;
    body.validate()
        .map_err(|r| ApiError::Validation(r.to_string()))?;

    let record = objects::update(&state.db, user.id(), id, body.as_fields())
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(ObjectDto::from_record(record)?))
}

/// Удаление объекта; блокируется лотами и договорами (FR-101).
#[utoipa::path(
    delete,
    path = "/api/v1/objects/{id}",
    tag = "objects",
    params(("id" = Uuid, Path, description = "Объект")),
    responses(
        (status = 204, description = "Удален"),
        (status = 404, description = "Не найден", body = crate::error::Problem),
        (status = 409, description = "Используется лотами/договорами", body = crate::error::Problem),
    )
)]
pub async fn delete_object(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    user.require(Action::ObjectManage)?;

    match objects::delete(&state.db, user.id(), id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(ApiError::NotFound),
        Err(DeleteObjectError::InUse) => Err(ApiError::rule(
            RuleViolation::ObjectInUse,
            "объект используется лотами или договорами (FR-101)",
        )),
        Err(DeleteObjectError::Db(err)) => Err(err.into()),
    }
}
