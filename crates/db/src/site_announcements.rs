//! Объявление на главной странице портала.

use time::OffsetDateTime;
use uuid::Uuid;

use crate::Db;

#[derive(Debug)]
pub struct SiteAnnouncementRecord {
    pub id: Uuid,
    pub title: String,
    pub body: String,
    pub is_published: bool,
    pub published_at: Option<OffsetDateTime>,
    pub updated_at: OffsetDateTime,
}

macro_rules! announcement_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            SiteAnnouncementRecord,
            r#"SELECT id, title, body, is_published, published_at, updated_at
               FROM core.site_announcements"# + $tail
            $(, $arg)*
        )
    };
}

/// Опубликованное объявление для открытой главной страницы.
pub async fn published(db: &Db) -> Result<Option<SiteAnnouncementRecord>, sqlx::Error> {
    announcement_query!(" WHERE placement = 'home' AND is_published")
        .fetch_optional(db)
        .await
}

/// Текущее объявление для формы администратора, включая скрытый черновик.
pub async fn current(db: &Db) -> Result<Option<SiteAnnouncementRecord>, sqlx::Error> {
    announcement_query!(" WHERE placement = 'home'")
        .fetch_optional(db)
        .await
}

/// Создает или заменяет объявление главной страницы. Триггер фиксирует в
/// аудите и первоначальную публикацию, и каждую последующую правку/скрытие.
pub async fn save(
    db: &Db,
    actor: Uuid,
    title: &str,
    body: &str,
    is_published: bool,
) -> Result<SiteAnnouncementRecord, sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query_as!(
            SiteAnnouncementRecord,
            r#"INSERT INTO core.site_announcements
                 (placement, title, body, is_published, published_at,
                  created_by, updated_by)
               VALUES ('home', $1, $2, $3,
                       CASE WHEN $3 THEN core.now() ELSE NULL END, $4, $4)
               ON CONFLICT (placement) DO UPDATE SET
                 title = EXCLUDED.title,
                 body = EXCLUDED.body,
                 is_published = EXCLUDED.is_published,
                 published_at = CASE
                   WHEN EXCLUDED.is_published THEN COALESCE(
                     core.site_announcements.published_at, core.now()
                   )
                   ELSE NULL
                 END,
                 updated_by = EXCLUDED.updated_by,
                 updated_at = core.now()
               RETURNING id, title, body, is_published, published_at, updated_at"#,
            title,
            body,
            is_published,
            actor
        )
        .fetch_one(&mut *tx)
        .await
    })
    .await
}
