//! Публикации особого порядка (М12, М14: FR-1403, FR-1202, п. 90, 92, 97).
//!
//! Публикуются три материала раздела 12: результат рассмотрения заявки
//! с обоснованием, обоснование ставки договора и акт приемки инвестиций.
//! Условия публикации (публикуемость категории, сформированная печатная
//! форма, срок публичного доступа) проверяет и считает БД - здесь запись,
//! чтение и снятие по истечении срока для фонового воркера.

use serde_json::Value;
use time::OffsetDateTime;
use tou_domain::obligation::ObligationAction;
use tou_domain::publication::{PublicRecordKind, PublicationFacts};
use uuid::Uuid;

use crate::Db;
use crate::obligations::Subject;

#[derive(Debug, thiserror::Error)]
pub enum PublicationError {
    #[error("материал не найден")]
    NotFound,
    /// Правило п. 92, 97 (домен) либо отказ БД (FR-1403, INV-076)
    #[error("{0}")]
    Rejected(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

fn map_rule(err: sqlx::Error) -> PublicationError {
    if let sqlx::Error::Database(db_err) = &err
        && matches!(
            db_err.code().as_deref(),
            Some("P0001") | Some("23514") | Some("23503") | Some("23505")
        )
    {
        return PublicationError::Rejected(db_err.message().to_owned());
    }
    PublicationError::Db(err)
}

pub struct PublicRecord {
    pub id: Uuid,
    pub kind: PublicRecordKind,
    pub special_request_id: Option<Uuid>,
    pub contract_id: Option<Uuid>,
    pub acceptance_id: Option<Uuid>,
    pub title: String,
    pub file_key: Option<String>,
    /// Обоснование ставки - снимок расчета Прил. 4 (FR-201)
    pub payload: Value,
    pub published_at: OffsetDateTime,
    /// Момент автоматического снятия: публикация + 6 месяцев (INV-076)
    pub unpublish_at: OffsetDateTime,
    pub unpublished_at: Option<OffsetDateTime>,
}

/// Строка выборки: то же, что [`PublicRecord`], но `kind` - еще текст из БД.
///
/// Отдельный тип, потому что `query_as!` кладет столбцы в поля как есть,
/// а `PublicRecordKind` - доменный тип: чтобы макрос разбирал его сам,
/// домену пришлось бы зависеть от sqlx (арх. § 5). Разбор остается здесь.
struct PublicRecordRow {
    id: Uuid,
    kind: String,
    special_request_id: Option<Uuid>,
    contract_id: Option<Uuid>,
    acceptance_id: Option<Uuid>,
    title: String,
    file_key: Option<String>,
    payload: Value,
    published_at: OffsetDateTime,
    unpublish_at: OffsetDateTime,
    unpublished_at: Option<OffsetDateTime>,
}

impl TryFrom<PublicRecordRow> for PublicRecord {
    type Error = sqlx::Error;

    fn try_from(row: PublicRecordRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            kind: row
                .kind
                .parse()
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            special_request_id: row.special_request_id,
            contract_id: row.contract_id,
            acceptance_id: row.acceptance_id,
            title: row.title,
            file_key: row.file_key,
            payload: row.payload,
            published_at: row.published_at,
            unpublish_at: row.unpublish_at,
            unpublished_at: row.unpublished_at,
        })
    }
}

impl PublicRecord {
    /// Состояние публикации в терминах домена (INV-076): материал публичен
    /// от публикации до снятия.
    pub fn facts(&self) -> PublicationFacts {
        PublicationFacts {
            has_pdf: self.file_key.is_some(),
            published: true,
            unpublished: self.unpublished_at.is_some(),
        }
    }
}

/// Выборка публикации: общий список столбцов + хвост запроса (см. `acts.rs`).
///
/// `!` у `kind` - это `::text`, который планировщик считает потенциально
/// NULL, хотя столбец NOT NULL.
macro_rules! record_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            PublicRecordRow,
            r#"SELECT id, kind::text AS "kind!", special_request_id, contract_id,
                      acceptance_id, title, file_key, payload, published_at,
                      unpublish_at, unpublished_at
               FROM core.public_records"# + $tail
            $(, $arg)*
        )
    };
}

pub struct NewPublicRecord<'a> {
    pub kind: PublicRecordKind,
    pub special_request_id: Option<Uuid>,
    pub contract_id: Option<Uuid>,
    pub acceptance_id: Option<Uuid>,
    pub title: &'a str,
    pub file_key: Option<&'a str>,
    pub payload: Value,
}

/// Публикация материала на портале (FR-1403, п. 97). Условия и срок
/// публичного доступа проверяет БД; срок публикации результата (5 рабочих
/// дней, п. 97) закрывается самим фактом публикации (FR-1702).
pub async fn publish(
    db: &Db,
    actor: Uuid,
    new: NewPublicRecord<'_>,
) -> Result<PublicRecord, PublicationError> {
    crate::with_actor(db, actor, async |tx| {
        // `$1::text::core.public_record_kind`: значение приходит строкой
        // доменного типа, а приведение к перечислению делает БД
        let id = sqlx::query_scalar!(
            "INSERT INTO core.public_records
               (kind, special_request_id, contract_id, acceptance_id,
                title, file_key, payload, published_by)
             VALUES ($1::text::core.public_record_kind, $2, $3, $4, $5, $6, $7, $8)
             RETURNING id",
            new.kind.as_str(),
            new.special_request_id,
            new.contract_id,
            new.acceptance_id,
            new.title,
            new.file_key,
            new.payload,
            actor
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_rule)?;

        if let Some(request_id) = new.special_request_id {
            crate::obligations::complete(
                &mut *tx,
                ObligationAction::SpecialPublish,
                Subject::special_request(request_id),
            )
            .await?;
        }

        let row = record_query!(" WHERE id = $1", id)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_rule)?;
        PublicRecord::try_from(row).map_err(map_rule)
    })
    .await
}

/// Реестр портала (FR-1403): материалы, доступные публично сейчас.
pub async fn list_public(db: &Db) -> Result<Vec<PublicRecord>, sqlx::Error> {
    let rows = record_query!(
        " WHERE unpublished_at IS NULL ORDER BY published_at DESC LIMIT $1",
        crate::MAX_ROWS
    )
    .fetch_all(db)
    .await?;
    crate::warn_if_capped(rows.len(), "public_records::list_public");
    rows.into_iter().map(PublicRecord::try_from).collect()
}

pub async fn get(db: &Db, id: Uuid) -> Result<Option<PublicRecord>, sqlx::Error> {
    record_query!(" WHERE id = $1", id)
        .fetch_optional(db)
        .await?
        .map(PublicRecord::try_from)
        .transpose()
}

/// Публикации по заявке особого порядка: результат, обоснование ставки
/// ее договора и акты приемки - все, что относится к одному решению.
pub async fn list_for_request(db: &Db, request_id: Uuid) -> Result<Vec<PublicRecord>, sqlx::Error> {
    record_query!(
        " p
         WHERE p.special_request_id = $1
            OR p.contract_id IN (SELECT i.contract_id FROM core.investment_contracts i
                                 WHERE i.special_request_id = $1)
            OR p.acceptance_id IN (SELECT a.id FROM core.investment_acceptances a
                                   JOIN core.investment_contracts i ON i.contract_id = a.contract_id
                                   WHERE i.special_request_id = $1)
         ORDER BY p.published_at DESC",
        request_id
    )
    .fetch_all(db)
    .await?
    .into_iter()
    .map(PublicRecord::try_from)
    .collect()
}

/// Материал, который Правила велят опубликовать, а публикации еще нет
/// (FR-1403): рабочий список уполномоченного подразделения.
pub struct PendingPublication {
    pub kind: PublicRecordKind,
    /// Заявка, договор или акт - предмет будущей публикации
    pub source_id: Uuid,
    pub title: String,
    /// Событие, породившее обязанность публикации (решение, договор, акт)
    pub occurred_at: OffsetDateTime,
    /// Материал готов к публикации: печатная форма или расчет есть
    pub ready: bool,
}

/// Строка выборки: `kind` - еще текст из БД (см. [`PublicRecordRow`]).
struct PendingRow {
    kind: String,
    source_id: Uuid,
    title: String,
    occurred_at: OffsetDateTime,
    ready: bool,
}

impl TryFrom<PendingRow> for PendingPublication {
    type Error = sqlx::Error;

    fn try_from(row: PendingRow) -> Result<Self, Self::Error> {
        Ok(Self {
            kind: row
                .kind
                .parse()
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            source_id: row.source_id,
            title: row.title,
            occurred_at: row.occurred_at,
            ready: row.ready,
        })
    }
}

/// Что ждет публикации: решения по публикуемым категориям (п. 87, 97),
/// обоснования ставок договоров особого порядка и акты приемки (п. 92).
pub async fn pending(db: &Db) -> Result<Vec<PendingPublication>, sqlx::Error> {
    // Столбцы под UNION планировщик считает потенциально NULL; сортировка
    // идет по номеру, потому что переименование меняет имя столбца
    let rows = sqlx::query_as!(
        PendingRow,
        r#"SELECT 'decision'::text AS "kind!", r.id AS "source_id!",
                'Результат по заявке: ' || c.label_ru || ' (' || c.rule_ref || ')' AS "title!",
                d.decided_at AS "occurred_at!", d.pdf_key IS NOT NULL AS "ready!"
         FROM core.special_board_decisions d
         JOIN core.special_requests r ON r.id = d.special_request_id
         JOIN refdata.special_categories c ON c.code = r.category
         WHERE c.publishable
           AND NOT EXISTS (SELECT 1 FROM core.public_records p
                           WHERE p.special_request_id = r.id)
         UNION ALL
         SELECT 'rate', i.contract_id,
                'Обоснование ставки договора особого порядка', i.created_at,
                i.rate_calculation IS NOT NULL
         FROM core.investment_contracts i
         WHERE NOT EXISTS (SELECT 1 FROM core.public_records p
                           WHERE p.contract_id = i.contract_id)
         UNION ALL
         SELECT 'investment_act', a.id,
                'Акт приемки инвестиций от ' || to_char(a.act_date, 'DD.MM.YYYY'),
                a.created_at, a.pdf_key IS NOT NULL
         FROM core.investment_acceptances a
         WHERE NOT EXISTS (SELECT 1 FROM core.public_records p
                           WHERE p.acceptance_id = a.id)
         ORDER BY 4 LIMIT $1"#,
        crate::MAX_ROWS
    )
    .fetch_all(db)
    .await?;
    crate::warn_if_capped(rows.len(), "public_records::pending");
    rows.into_iter().map(PendingPublication::try_from).collect()
}

/// Снятие материалов с публичного доступа по истечении шести месяцев
/// (INV-076, п. 76) - работа фонового воркера. Материал остается в досье
/// решения: запись туда делает триггер БД.
pub async fn take_expired(db: &Db) -> Result<Vec<PublicRecord>, sqlx::Error> {
    let mut tx = db.begin().await?;

    let ids = sqlx::query_scalar!(
        // Пачкой - см. `publications::take_expired`
        "UPDATE core.public_records
         SET unpublished_at = core.now()
         WHERE id IN (
           SELECT id FROM core.public_records
           WHERE unpublished_at IS NULL AND unpublish_at <= core.now()
           ORDER BY unpublish_at
           LIMIT $1
           FOR UPDATE SKIP LOCKED
         )
         RETURNING id",
        crate::BATCH_ROWS
    )
    .fetch_all(&mut *tx)
    .await?;

    // Один запрос на весь пакет - см. `publications::take_expired`
    let taken = record_query!(" WHERE id = ANY($1)", &ids)
        .fetch_all(&mut *tx)
        .await?;

    tx.commit().await?;
    taken.into_iter().map(PublicRecord::try_from).collect()
}
