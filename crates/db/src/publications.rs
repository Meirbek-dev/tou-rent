//! Публикация протоколов и досье (М7, М14, М16: FR-702, FR-703, FR-1206,
//! FR-1402, FR-1602, INV-042, INV-076).
//!
//! Публикация задает срок публичного доступа (шесть месяцев считает БД,
//! п. 76), снятие выполняет фоновый воркер, а досье собирается триггерами
//! в момент событий - здесь только чтение его состава и выдача материалов.
//! Досье два вида: тендера и решения особого порядка (FR-1206); срок
//! хранения каждого материала проставляет БД (INV-042).

use time::OffsetDateTime;
use tou_domain::obligation::ObligationAction;
use tou_domain::publication::{DossierKind, PublicationFacts};
use tou_domain::rule::{RuleRejection, RuleViolation};
use uuid::Uuid;

use crate::Db;
use crate::obligations::Subject;

#[derive(Debug, thiserror::Error)]
pub enum PublicationError {
    #[error("протокол не найден")]
    NotFound,
    /// Правило п. 75–76 (домен) либо отказ БД (INV-076)
    #[error("{0}")]
    Rejected(RuleRejection),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

impl From<tou_domain::publication::PublicationError> for PublicationError {
    fn from(err: tou_domain::publication::PublicationError) -> Self {
        // Истекший срок публичного доступа - INV-076, остальное - FR-702
        let rule = match &err {
            tou_domain::publication::PublicationError::AccessExpired => {
                RuleViolation::PublicationRetention
            }
            tou_domain::publication::PublicationError::NoDocument
            | tou_domain::publication::PublicationError::AlreadyPublished => {
                RuleViolation::ProtocolPublication
            }
        };
        PublicationError::Rejected(RuleRejection::new(rule, err.to_string()))
    }
}

fn map_rule(err: sqlx::Error) -> PublicationError {
    if let sqlx::Error::Database(db_err) = &err
        && matches!(
            db_err.code().as_deref(),
            Some("P0001") | Some("23514") | Some("23503") | Some("23505")
        )
    {
        return PublicationError::Rejected(crate::rule::rejection(db_err.as_ref()));
    }
    PublicationError::Db(err)
}

pub struct ProtocolRecord {
    pub id: Uuid,
    pub tender_id: Uuid,
    pub tender_title: String,
    /// `admission` | `results` | `failed` | `winner2`
    pub kind: String,
    pub number: Option<String>,
    pub pdf_key: Option<String>,
    pub generated_at: OffsetDateTime,
    pub published_at: Option<OffsetDateTime>,
    /// Момент автоматического снятия - публикация + 6 месяцев (INV-076)
    pub unpublish_at: Option<OffsetDateTime>,
    pub unpublished_at: Option<OffsetDateTime>,
}

impl ProtocolRecord {
    /// Состояние публикации в терминах домена (FR-702, INV-076).
    pub fn facts(&self) -> PublicationFacts {
        PublicationFacts {
            has_pdf: self.pdf_key.is_some(),
            published: self.published_at.is_some(),
            unpublished: self.unpublished_at.is_some(),
        }
    }
}

/// Выборка протокола: общий список столбцов + хвост запроса (см. `acts.rs`).
///
/// `!` у `kind` - это `::text`, который планировщик считает потенциально
/// NULL, хотя столбец NOT NULL.
macro_rules! protocol_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            ProtocolRecord,
            r#"SELECT p.id, p.tender_id, t.title AS tender_title,
                      p.kind::text AS "kind!", p.number, p.pdf_key, p.generated_at,
                      p.published_at, p.unpublish_at, p.unpublished_at
               FROM core.protocols p
               JOIN core.tenders t ON t.id = p.tender_id"# + $tail
            $(, $arg)*
        )
    };
}

pub async fn get(db: &Db, id: Uuid) -> Result<Option<ProtocolRecord>, sqlx::Error> {
    protocol_query!(" WHERE p.id = $1", id)
        .fetch_optional(db)
        .await
}

/// Протоколы тендера: все виды, свежие сверху.
pub async fn list_for_tender(db: &Db, tender_id: Uuid) -> Result<Vec<ProtocolRecord>, sqlx::Error> {
    protocol_query!(
        " WHERE p.tender_id = $1 ORDER BY p.generated_at DESC",
        tender_id
    )
    .fetch_all(db)
    .await
}

/// Протоколы тендеров, в которых участвовал пользователь (FR-703, п. 56):
/// копия доступна участнику независимо от публичного срока.
///
/// Курсора нет: выборку ограничивает участие одного человека - протоколов
/// у тендера единицы, а тендеров у участника десятки. Потолок здесь -
/// защита от невероятного, и признак усечения сообщает именно о нем.
pub async fn list_for_participant(
    db: &Db,
    participant_id: Uuid,
) -> Result<crate::Page<ProtocolRecord>, sqlx::Error> {
    let rows = protocol_query!(
        " WHERE EXISTS (SELECT 1 FROM core.applications a
                        WHERE a.tender_id = p.tender_id AND a.participant_id = $1)
          ORDER BY p.generated_at DESC LIMIT $2",
        participant_id,
        crate::probe_limit(crate::MAX_ROWS)
    )
    .fetch_all(db)
    .await?;
    let page = crate::Page::probe(rows, crate::MAX_ROWS);
    crate::warn_if_truncated(page.truncated, "publications::list_for_participant");
    Ok(page)
}

/// Публикация протокола (FR-702, п. 75). Срок публичного доступа считает БД
/// (INV-076), срок публикации закрывается фактом (FR-1702).
pub async fn publish(
    db: &Db,
    actor: Uuid,
    protocol_id: Uuid,
) -> Result<ProtocolRecord, PublicationError> {
    let record = get(db, protocol_id)
        .await?
        .ok_or(PublicationError::NotFound)?;
    record.facts().check_publish()?;

    crate::with_actor(db, actor, async |tx| {
        sqlx::query!(
            "UPDATE core.protocols SET published_at = core.now() WHERE id = $1",
            protocol_id
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;

        // Срок публикации итогов - два рабочих дня (п. 75, FR-1702)
        if record.kind == "results" {
            crate::obligations::complete(
                &mut *tx,
                ObligationAction::PublishResults,
                Subject::tender(record.tender_id),
            )
            .await?;
        }

        protocol_query!(" WHERE p.id = $1", protocol_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_rule)
    })
    .await
}

/// Снятие протоколов с публичного доступа по истечении шести месяцев
/// (INV-076, п. 76) - работа фонового воркера. Возвращает снятые протоколы:
/// в досье они уже записаны триггером.
pub async fn take_expired(db: &Db) -> Result<Vec<ProtocolRecord>, sqlx::Error> {
    let mut tx = db.begin().await?;

    // Пачкой: срок истекает у многих сразу, а следующий тик воркера
    // подберет остальное (см. crate::BATCH_ROWS)
    let ids = sqlx::query_scalar!(
        "UPDATE core.protocols
         SET unpublished_at = core.now()
         WHERE id IN (
           SELECT id FROM core.protocols
           WHERE published_at IS NOT NULL AND unpublished_at IS NULL
             AND unpublish_at IS NOT NULL AND unpublish_at <= core.now()
           ORDER BY unpublish_at
           LIMIT $1
           FOR UPDATE SKIP LOCKED
         )
         RETURNING id",
        crate::BATCH_ROWS
    )
    .fetch_all(&mut *tx)
    .await?;

    // Один запрос на весь пакет, а не на протокол: проход воркера идет
    // раз в минуту, и в день истечения срока снимается сразу пачка
    let taken = protocol_query!(" WHERE p.id = ANY($1)", &ids)
        .fetch_all(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(taken)
}

pub struct DossierItem {
    pub id: Uuid,
    pub kind: DossierKind,
    pub title: Option<String>,
    pub file_key: Option<String>,
    pub source_table: Option<String>,
    pub source_id: Option<Uuid>,
    pub occurred_at: OffsetDateTime,
    /// Срок хранения материала: считает БД по предмету досье (INV-042)
    pub retain_until: OffsetDateTime,
}

/// Строка выборки: то же, что [`DossierItem`], но `kind` - еще текст из БД
/// (см. `acts.rs`).
struct DossierRow {
    id: Uuid,
    kind: String,
    title: Option<String>,
    file_key: Option<String>,
    source_table: Option<String>,
    source_id: Option<Uuid>,
    occurred_at: OffsetDateTime,
    retain_until: OffsetDateTime,
}

impl TryFrom<DossierRow> for DossierItem {
    type Error = sqlx::Error;

    fn try_from(row: DossierRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            kind: row
                .kind
                .parse()
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            title: row.title,
            file_key: row.file_key,
            source_table: row.source_table,
            source_id: row.source_id,
            occurred_at: row.occurred_at,
            retain_until: row.retain_until,
        })
    }
}

/// Выборка материала досье: общий список столбцов + хвост (см. `acts.rs`).
macro_rules! dossier_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            DossierRow,
            "SELECT id, kind, title, file_key, source_table, source_id,
                    occurred_at, retain_until
             FROM core.dossier_items" + $tail
            $(, $arg)*
        )
    };
}

/// Состав досье тендера (FR-1602): материалы в хронологическом порядке.
pub async fn dossier(db: &Db, tender_id: Uuid) -> Result<Vec<DossierItem>, sqlx::Error> {
    dossier_query!(
        " WHERE tender_id = $1 ORDER BY occurred_at, kind",
        tender_id
    )
    .fetch_all(db)
    .await?
    .into_iter()
    .map(DossierItem::try_from)
    .collect()
}

/// Состав досье решения особого порядка (FR-1206, п. 97): заявка, ее
/// документы, заключение подразделения и решение Правления - тем же
/// механизмом и в том же порядке, что и досье тендера.
pub async fn special_dossier(db: &Db, request_id: Uuid) -> Result<Vec<DossierItem>, sqlx::Error> {
    dossier_query!(
        " WHERE special_request_id = $1 ORDER BY occurred_at, kind",
        request_id
    )
    .fetch_all(db)
    .await?
    .into_iter()
    .map(DossierItem::try_from)
    .collect()
}

/// Участники тендера (получатели копий протоколов, п. 56).
pub async fn participants_of(db: &Db, tender_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT DISTINCT participant_id FROM core.applications WHERE tender_id = $1",
        tender_id
    )
    .fetch_all(db)
    .await
}
