//! Offline commission documents. Visibility never changes application outcomes.
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use crate::Db;

pub struct Document {
    pub id: Uuid,
    pub tender_id: Uuid,
    pub application_id: Option<Uuid>,
    pub title: String,
    pub number: String,
    pub document_date: Date,
    pub filename: String,
    pub file_key: String,
    pub size_bytes: i64,
    pub uploaded_by: Uuid,
    pub uploaded_at: OffsetDateTime,
    pub shared_at: Option<OffsetDateTime>,
}

macro_rules! documents {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(Document,
          "SELECT id, tender_id, application_id, title, number, document_date, filename,
                  file_key, size_bytes, uploaded_by, uploaded_at, shared_at
           FROM core.commission_documents d " + $tail $(, $arg)*)
    };
}

/// Authorize at query time, including direct downloads. No anonymous access.
pub async fn list(
    db: &Db,
    actor: Uuid,
    manager: bool,
    tender: Option<Uuid>,
) -> Result<crate::Page<Document>, sqlx::Error> {
    let rows = documents!(
        "WHERE ($1::uuid IS NULL OR d.tender_id = $1)
         AND ($2 OR (d.shared_at IS NOT NULL AND EXISTS (
           SELECT 1 FROM core.applications a WHERE a.participant_id = $3
             AND a.tender_id = d.tender_id
             AND (d.application_id IS NULL OR d.application_id = a.id))))
         ORDER BY d.uploaded_at DESC, d.id DESC LIMIT $4",
        tender,
        manager,
        actor,
        crate::probe_limit(crate::MAX_ROWS)
    )
    .fetch_all(db)
    .await?;
    Ok(crate::Page::probe(rows, crate::MAX_ROWS))
}

pub async fn visible(
    db: &Db,
    id: Uuid,
    actor: Uuid,
    manager: bool,
) -> Result<Option<Document>, sqlx::Error> {
    documents!(
        "WHERE d.id = $1 AND ($2 OR (d.shared_at IS NOT NULL AND EXISTS (
      SELECT 1 FROM core.applications a WHERE a.participant_id = $3
        AND a.tender_id = d.tender_id AND (d.application_id IS NULL OR d.application_id = a.id))))",
        id,
        manager,
        actor
    )
    .fetch_optional(db)
    .await
}

pub async fn valid_target(
    db: &Db,
    tender: Uuid,
    application: Option<Uuid>,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT EXISTS (SELECT 1 FROM core.tenders t
      WHERE t.id = $1 AND t.opened_at IS NOT NULL AND ($2::uuid IS NULL OR EXISTS (
        SELECT 1 FROM core.applications a WHERE a.id = $2 AND a.tender_id = t.id))) AS \"valid!\"",
        tender,
        application
    )
    .fetch_one(db)
    .await
}

pub struct NewDocument<'a> {
    pub id: Uuid,
    pub tender_id: Uuid,
    pub application_id: Option<Uuid>,
    pub title: &'a str,
    pub number: &'a str,
    pub document_date: Date,
    pub filename: &'a str,
    pub file_key: &'a str,
    pub size_bytes: i64,
}

pub async fn insert(db: &Db, actor: Uuid, doc: NewDocument<'_>) -> Result<(), sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query!("INSERT INTO core.commission_documents
          (id, tender_id, application_id, title, number, document_date, filename, file_key, size_bytes, uploaded_by)
          VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)", doc.id, doc.tender_id, doc.application_id,
          doc.title, doc.number, doc.document_date, doc.filename, doc.file_key, doc.size_bytes, actor)
          .execute(&mut *tx).await?;
        Ok(())
    }).await
}

pub async fn share(db: &Db, actor: Uuid, id: Uuid, shared: bool) -> Result<bool, sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        let result = sqlx::query!(
            "UPDATE core.commission_documents
          SET shared_at = CASE WHEN $2 THEN coalesce(shared_at, core.now()) ELSE NULL END
          WHERE id = $1",
            id,
            shared
        )
        .execute(&mut *tx)
        .await?;
        Ok(result.rows_affected() == 1)
    })
    .await
}
