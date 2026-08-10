//! Реестр объектов имущества (М1): CRUD `core.objects`.
//! Статус вычисляется view `core.object_statuses` (FR-103), не хранится.

use rust_decimal::Decimal;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::Db;

#[derive(Debug, Clone)]
pub struct ObjectRecord {
    pub id: Uuid,
    pub kind: String,
    pub name: String,
    pub address: String,
    pub area_m2: Decimal,
    pub floor_part: Option<String>,
    pub premises_type_code: Option<String>,
    pub premises_kind_code: Option<String>,
    pub comfort_code: Option<String>,
    pub location_code: Option<String>,
    pub photo_keys: Vec<String>,
    /// Из view: free | in_tender | leased (FR-103)
    pub status: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

/// Выборка объекта: общий список столбцов + хвост запроса (см. `acts.rs`).
///
/// `!` у `kind` и `status`: первый - `::text` от перечисления, второй
/// приходит из view `core.object_statuses`, и планировщик считает оба
/// потенциально NULL, хотя ни один таким не бывает.
macro_rules! object_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            ObjectRecord,
            r#"SELECT o.id, o.kind::text AS "kind!", o.name, o.address, o.area_m2,
                      o.floor_part, o.premises_type_code, o.premises_kind_code,
                      o.comfort_code, o.location_code,
                      o.photo_keys, s.status AS "status!", o.created_at, o.updated_at
               FROM core.objects o
               JOIN core.object_statuses s ON s.object_id = o.id"# + $tail
            $(, $arg)*
        )
    };
}

/// Поля создания/полного обновления объекта (FR-101).
#[derive(Debug, Clone)]
pub struct ObjectFields<'a> {
    pub kind: &'a str,
    pub name: &'a str,
    pub address: &'a str,
    pub area_m2: Decimal,
    pub floor_part: Option<&'a str>,
    pub premises_type_code: Option<&'a str>,
    pub premises_kind_code: Option<&'a str>,
    pub comfort_code: Option<&'a str>,
    pub location_code: Option<&'a str>,
}

/// Фильтры витрины свободных площадей (FR-102). Пустое поле - «не фильтровать».
#[derive(Debug, Default, Clone)]
pub struct ObjectFilter<'a> {
    /// `free` | `in_tender` | `leased` (вычисляемый статус, FR-103)
    pub status: Option<&'a str>,
    /// `premises` | `land_plot`
    pub kind: Option<&'a str>,
    /// Подстрока названия или адреса, регистронезависимо
    pub query: Option<&'a str>,
    pub area_min: Option<Decimal>,
    pub area_max: Option<Decimal>,
}

pub async fn list(
    db: &Db,
    after: Option<Uuid>,
    limit: i64,
    filter: ObjectFilter<'_>,
) -> Result<Vec<ObjectRecord>, sqlx::Error> {
    // Свежие объекты сверху: реестр читают с последних поступлений,
    // а только что заведенный объект обязан быть виден сразу - иначе
    // за пределами первой страницы его не найти (находка приемки, T44)
    object_query!(
        "
         WHERE ($1::uuid IS NULL OR o.id < $1)
           AND ($3::text IS NULL OR s.status = $3)
           AND ($4::text IS NULL OR o.kind = $4::text::core.object_kind)
           AND ($5::text IS NULL OR o.name ILIKE '%' || $5 || '%'
                                 OR o.address ILIKE '%' || $5 || '%')
           AND ($6::numeric IS NULL OR o.area_m2 >= $6)
           AND ($7::numeric IS NULL OR o.area_m2 <= $7)
         ORDER BY o.id DESC LIMIT $2",
        after,
        limit,
        filter.status,
        filter.kind,
        filter.query,
        filter.area_min,
        filter.area_max
    )
    .fetch_all(db)
    .await
}

pub async fn get(db: &Db, id: Uuid) -> Result<Option<ObjectRecord>, sqlx::Error> {
    object_query!(" WHERE o.id = $1", id)
        .fetch_optional(db)
        .await
}

pub async fn insert(
    db: &Db,
    actor: Uuid,
    f: ObjectFields<'_>,
) -> Result<ObjectRecord, sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        let id = sqlx::query_scalar!(
            "INSERT INTO core.objects (kind, name, address, area_m2, floor_part,
                premises_type_code, premises_kind_code, comfort_code, location_code)
             VALUES ($1::text::core.object_kind, $2, $3, $4, $5, $6, $7, $8, $9)
             RETURNING id",
            f.kind,
            f.name,
            f.address,
            f.area_m2,
            f.floor_part,
            f.premises_type_code,
            f.premises_kind_code,
            f.comfort_code,
            f.location_code
        )
        .fetch_one(&mut *tx)
        .await?;

        // Статус берется из view - читаем в той же транзакции
        object_query!(" WHERE o.id = $1", id)
            .fetch_one(&mut *tx)
            .await
    })
    .await
}

pub async fn update(
    db: &Db,
    actor: Uuid,
    id: Uuid,
    f: ObjectFields<'_>,
) -> Result<Option<ObjectRecord>, sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        let updated = sqlx::query!(
            "UPDATE core.objects SET kind = $2::text::core.object_kind, name = $3, address = $4,
                area_m2 = $5, floor_part = $6, premises_type_code = $7, premises_kind_code = $8,
                comfort_code = $9, location_code = $10
             WHERE id = $1",
            id,
            f.kind,
            f.name,
            f.address,
            f.area_m2,
            f.floor_part,
            f.premises_type_code,
            f.premises_kind_code,
            f.comfort_code,
            f.location_code
        )
        .execute(&mut *tx)
        .await?;

        if updated.rows_affected() == 0 {
            return Ok(None);
        }
        object_query!(" WHERE o.id = $1", id)
            .fetch_optional(&mut *tx)
            .await
    })
    .await
}

#[derive(Debug, thiserror::Error)]
pub enum DeleteObjectError {
    #[error("объект используется лотами или договорами - удаление запрещено (FR-101)")]
    InUse,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Удаление; ссылающиеся лоты/договоры блокируют его FK-ограничениями (FR-101).
pub async fn delete(db: &Db, actor: Uuid, id: Uuid) -> Result<bool, DeleteObjectError> {
    crate::with_actor(db, actor, async |tx| {
        let result = sqlx::query!("DELETE FROM core.objects WHERE id = $1", id)
            .execute(&mut *tx)
            .await;

        match result {
            Ok(done) => Ok(done.rows_affected() > 0),
            Err(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("23503") => {
                Err(DeleteObjectError::InUse)
            }
            Err(other) => Err(other.into()),
        }
    })
    .await
}
