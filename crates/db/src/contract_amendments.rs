//! Допсоглашения к договору (М9: FR-906, FR-901, п. 125).
//!
//! Допсоглашение записывается вместе со своими правками одной транзакцией:
//! соглашения без изменений не бывает. Что менять разрешено - закрытый
//! перечень справочника (FR-906), существенные условия в него не входят
//! и отдельно отклоняются триггером (FR-901).

use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::Db;

#[derive(Debug, thiserror::Error)]
pub enum AmendmentError {
    #[error("договор или допсоглашение не найдены")]
    NotFound,
    /// Правило п. 125 (домен) либо отказ БД (FR-901, FR-906)
    #[error("{0}")]
    Rejected(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

fn map_rule(err: sqlx::Error) -> AmendmentError {
    if let sqlx::Error::Database(db_err) = &err
        && matches!(
            db_err.code().as_deref(),
            Some("P0001") | Some("23514") | Some("23503") | Some("23505")
        )
    {
        return AmendmentError::Rejected(db_err.message().to_owned());
    }
    AmendmentError::Db(err)
}

/// Правка допсоглашения: поле, прежнее и новое значение (FR-906).
pub struct ChangeRecord {
    pub field_code: String,
    pub field_label: String,
    pub old_value: String,
    pub new_value: String,
}

pub struct AmendmentRecord {
    pub id: Uuid,
    pub contract_id: Uuid,
    pub seq: i32,
    pub ground: String,
    pub effective_on: Date,
    pub pdf_key: Option<String>,
    pub created_by_name: Option<String>,
    pub created_at: OffsetDateTime,
    pub changes: Vec<ChangeRecord>,
}

/// Строка выборки: то же, что [`AmendmentRecord`], но без правок - они
/// читаются вторым запросом. Отдельный тип, потому что `query_as!` кладет
/// в поля столбцы как есть, а `changes` столбцом не является.
struct AmendmentSqlRow {
    id: Uuid,
    contract_id: Uuid,
    seq: i32,
    ground: String,
    effective_on: Date,
    pdf_key: Option<String>,
    created_by_name: Option<String>,
    created_at: OffsetDateTime,
}

impl From<AmendmentSqlRow> for AmendmentRecord {
    fn from(row: AmendmentSqlRow) -> Self {
        Self {
            id: row.id,
            contract_id: row.contract_id,
            seq: row.seq,
            ground: row.ground,
            effective_on: row.effective_on,
            pdf_key: row.pdf_key,
            created_by_name: row.created_by_name,
            created_at: row.created_at,
            changes: Vec::new(),
        }
    }
}

/// Выборка допсоглашения: общий список столбцов + хвост (см. `acts.rs`).
///
/// `created_by_name` получает `?`: имя приходит `LEFT JOIN`'ом, а
/// `core.users.full_name` - NOT NULL, и без аннотации sqlx вывел бы non-null.
macro_rules! amendment_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            AmendmentSqlRow,
            r#"SELECT a.id, a.contract_id, a.seq, a.ground, a.effective_on,
                      a.pdf_key, u.full_name AS "created_by_name?", a.created_at
               FROM core.contract_amendments a
               LEFT JOIN core.users u ON u.id = a.created_by"# + $tail
            $(, $arg)*
        )
    };
}

/// Правки допсоглашения в порядке справочника.
macro_rules! change_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            ChangeRecord,
            r#"SELECT c.field_code, f.label_ru AS field_label,
                      c.old_value, c.new_value
               FROM core.contract_amendment_changes c
               JOIN refdata.amendable_fields f ON f.code = c.field_code"# + $tail
            $(, $arg)*
        )
    };
}

/// Допсоглашения договора с их правками, по порядку номеров (п. 125).
pub async fn list_for_contract(
    db: &Db,
    contract_id: Uuid,
) -> Result<Vec<AmendmentRecord>, sqlx::Error> {
    let mut records: Vec<AmendmentRecord> =
        amendment_query!(" WHERE a.contract_id = $1 ORDER BY a.seq", contract_id)
            .fetch_all(db)
            .await?
            .into_iter()
            .map(AmendmentRecord::from)
            .collect();

    for record in &mut records {
        record.changes = change_query!(" WHERE c.amendment_id = $1 ORDER BY f.ordinal", record.id)
            .fetch_all(db)
            .await?;
    }
    Ok(records)
}

pub async fn get(db: &Db, id: Uuid) -> Result<Option<AmendmentRecord>, sqlx::Error> {
    let Some(row) = amendment_query!(" WHERE a.id = $1", id)
        .fetch_optional(db)
        .await?
    else {
        return Ok(None);
    };
    let mut record = AmendmentRecord::from(row);

    record.changes = change_query!(" WHERE c.amendment_id = $1 ORDER BY f.ordinal", id)
        .fetch_all(db)
        .await?;
    Ok(Some(record))
}

pub struct NewChange<'a> {
    pub field_code: &'a str,
    pub old_value: &'a str,
    pub new_value: &'a str,
}

/// Допсоглашение с правками одной транзакцией (FR-906): номер в рамках
/// договора выдает БД, а пригодность полей проверяют FK и триггер.
pub async fn create(
    db: &Db,
    actor: Uuid,
    contract_id: Uuid,
    ground: &str,
    effective_on: Date,
    changes: &[NewChange<'_>],
) -> Result<AmendmentRecord, AmendmentError> {
    let id = crate::with_actor(db, actor, async |tx| {
        let id = sqlx::query_scalar!(
            "INSERT INTO core.contract_amendments
               (contract_id, seq, ground, effective_on, created_by)
             VALUES ($1,
                     (SELECT coalesce(max(seq), 0) + 1 FROM core.contract_amendments
                      WHERE contract_id = $1),
                     $2, $3, $4)
             RETURNING id",
            contract_id,
            ground,
            effective_on,
            actor
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_rule)?;

        for change in changes {
            sqlx::query!(
                "INSERT INTO core.contract_amendment_changes
                   (amendment_id, field_code, old_value, new_value)
                 VALUES ($1, $2, $3, $4)",
                id,
                change.field_code,
                change.old_value,
                change.new_value
            )
            .execute(&mut *tx)
            .await
            .map_err(map_rule)?;
        }

        Ok::<Uuid, AmendmentError>(id)
    })
    .await?;

    get(db, id).await?.ok_or(AmendmentError::NotFound)
}

/// Печатная форма допсоглашения (п. 125): ключ проставляется один раз,
/// само соглашение остается неизменяемым (триггер `freeze_contract_amendment`).
pub async fn attach_pdf(
    db: &Db,
    actor: Uuid,
    id: Uuid,
    pdf_key: &str,
) -> Result<(), AmendmentError> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query!(
            "UPDATE core.contract_amendments SET pdf_key = $2 WHERE id = $1",
            id,
            pdf_key
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;
        Ok(())
    })
    .await
}

/// Перечень изменяемых полей (FR-906, п. 125) - паритет с доменом
/// проверяет тест.
pub struct FieldRecord {
    pub code: String,
    pub label_ru: String,
    pub label_kk: Option<String>,
    pub label_en: Option<String>,
    pub rule_ref: String,
}

pub async fn list_fields(db: &Db) -> Result<Vec<FieldRecord>, sqlx::Error> {
    sqlx::query_as!(
        FieldRecord,
        "SELECT code, label_ru, label_kk, label_en, rule_ref
         FROM refdata.amendable_fields ORDER BY ordinal"
    )
    .fetch_all(db)
    .await
}
