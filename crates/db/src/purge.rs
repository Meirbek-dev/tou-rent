//! Очистка данных стенда администратором (М15, FR-1503; след - FR-1601).
//!
//! Стенд, наполненный `api seed` под демонстрацию, стал рабочим: демо-объекты,
//! тендеры во всех статусах, заявки и протоколы должны исчезнуть до прихода
//! настоящих процедур. Штатных средств для этого в системе нет намеренно:
//! объявленный тендер отменяется, а не удаляется (FR-305), протоколы и досье
//! append-only (FR-702, INV-042), у роли приложения нет права DELETE на них.
//!
//! Поэтому само удаление живет в БД - `core.purge_data()` под владельцем
//! схемы (миграция `20260902110000_admin_data_purge_scopes.sql`): сторожа
//! append-only снимаются на одну транзакцию, аудит-триггеры остаются, и
//! сверху ложится сводное событие `core.data_purge`. Здесь - только вызов
//! и обзор того, что уйдет. Разрешение на вызов дает http-слой
//! (`ALLOW_DATA_PURGE`).

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{Db, MAX_ROWS, Page};

/// Домен демо-учеток `api seed` (Прил. Б): один пароль на всех, и на рабочем
/// стенде такие записи должны быть отключены до первого настоящего входа.
pub const DEMO_EMAIL_SUFFIX: &str = "@tou.demo";

/// Область очистки: с какого вида данных начинается удаление. Все, что
/// держится на удаляемых записях, уходит с ними по внешним ключам.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurgeScope {
    /// Все процедуры и объекты стенда
    Everything,
    /// Тендеры со всем, что на них висит; объекты остаются
    Tenders,
    /// Объекты вместе с тендерами, где они выставлены лотом, участками
    /// и заявками особого порядка по ним
    Objects,
    /// Заявки особого порядка с заключениями, решениями, досье и
    /// инвестиционными договорами
    SpecialRequests,
    /// Земельные участки с заявками, решениями и договорами по ним
    LandPlots,
    /// Уведомления - все, чьи бы ни были
    Notifications,
}

impl PurgeScope {
    /// Имя области для БД - совпадает с проверкой внутри `core.purge_data`.
    pub fn as_str(self) -> &'static str {
        match self {
            PurgeScope::Everything => "everything",
            PurgeScope::Tenders => "tenders",
            PurgeScope::Objects => "objects",
            PurgeScope::SpecialRequests => "special_requests",
            PurgeScope::LandPlots => "land_plots",
            PurgeScope::Notifications => "notifications",
        }
    }
}

/// Сколько строк каждого вида лежит на стенде - что именно уйдет при очистке.
#[derive(Debug, Clone, Copy, Default)]
pub struct DataCounts {
    pub objects: i64,
    pub tenders: i64,
    pub lots: i64,
    pub applications: i64,
    pub protocols: i64,
    pub auctions: i64,
    pub contracts: i64,
    pub acts: i64,
    pub ledger_entries: i64,
    pub special_requests: i64,
    pub land_plots: i64,
    pub investment_contracts: i64,
    pub dossier_items: i64,
    pub public_records: i64,
    pub obligations: i64,
    pub notifications: i64,
    /// Действующие демо-учетки `*@tou.demo`; очистка их не удаляет,
    /// а отключает - см. [`deactivate_demo_accounts`]
    pub demo_accounts: i64,
}

/// Обзор данных стенда для вкладки очистки.
pub async fn counts(db: &Db) -> Result<DataCounts, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT
             (SELECT count(*) FROM core.objects)              AS "objects!",
             (SELECT count(*) FROM core.tenders)              AS "tenders!",
             (SELECT count(*) FROM core.lots)                 AS "lots!",
             (SELECT count(*) FROM core.applications)         AS "applications!",
             (SELECT count(*) FROM core.protocols)            AS "protocols!",
             (SELECT count(*) FROM core.auctions)             AS "auctions!",
             (SELECT count(*) FROM core.contracts)            AS "contracts!",
             (SELECT count(*) FROM core.acts)                 AS "acts!",
             (SELECT count(*) FROM core.ledger_entries)       AS "ledger_entries!",
             (SELECT count(*) FROM core.special_requests)     AS "special_requests!",
             (SELECT count(*) FROM core.land_plots)           AS "land_plots!",
             (SELECT count(*) FROM core.investment_contracts) AS "investment_contracts!",
             (SELECT count(*) FROM core.dossier_items)        AS "dossier_items!",
             (SELECT count(*) FROM core.public_records)       AS "public_records!",
             (SELECT count(*) FROM core.obligations)          AS "obligations!",
             (SELECT count(*) FROM core.notifications)        AS "notifications!",
             (SELECT count(*) FROM core.users
               WHERE is_active AND email::text LIKE '%' || $1) AS "demo_accounts!""#,
        DEMO_EMAIL_SUFFIX
    )
    .fetch_one(db)
    .await?;

    Ok(DataCounts {
        objects: row.objects,
        tenders: row.tenders,
        lots: row.lots,
        applications: row.applications,
        protocols: row.protocols,
        auctions: row.auctions,
        contracts: row.contracts,
        acts: row.acts,
        ledger_entries: row.ledger_entries,
        special_requests: row.special_requests,
        land_plots: row.land_plots,
        investment_contracts: row.investment_contracts,
        dossier_items: row.dossier_items,
        public_records: row.public_records,
        obligations: row.obligations,
        notifications: row.notifications,
        demo_accounts: row.demo_accounts,
    })
}

/// Тендер в перечне на удаление: заголовок, статус и сколько заявок уйдет
/// вместе с ним.
#[derive(Debug, Clone)]
pub struct AdminTenderRecord {
    pub id: Uuid,
    pub title: String,
    pub title_kk: String,
    pub status: String,
    pub created_at: OffsetDateTime,
    pub applications: i64,
}

/// Все тендеры стенда, свежие сверху - в любом статусе, включая черновики
/// чужих организаторов: очистка смотрит на стенд целиком.
pub async fn list_tenders(db: &Db) -> Result<Page<AdminTenderRecord>, sqlx::Error> {
    let rows = sqlx::query_as!(
        AdminTenderRecord,
        r#"SELECT t.id, t.title, t.title_kk, t.status::text AS "status!", t.created_at,
                  (SELECT count(*) FROM core.applications a WHERE a.tender_id = t.id)
                    AS "applications!"
           FROM core.tenders t
           ORDER BY t.id DESC
           LIMIT $1"#,
        crate::probe_limit(MAX_ROWS)
    )
    .fetch_all(db)
    .await?;

    let page = Page::probe(rows, MAX_ROWS);
    crate::warn_if_truncated(page.truncated, "purge::list_tenders");
    Ok(page)
}

/// Объект в перечне на удаление: имя, вид, площадь и сколько тендеров
/// уйдет вместе с ним (те, где он выставлен лотом).
#[derive(Debug, Clone)]
pub struct AdminObjectRecord {
    pub id: Uuid,
    pub name: String,
    pub name_kk: String,
    pub kind: String,
    pub area_m2: Decimal,
    pub created_at: OffsetDateTime,
    pub tenders: i64,
}

/// Все объекты стенда, свежие сверху.
pub async fn list_objects(db: &Db) -> Result<Page<AdminObjectRecord>, sqlx::Error> {
    let rows = sqlx::query_as!(
        AdminObjectRecord,
        r#"SELECT o.id, o.name, o.name_kk, o.kind::text AS "kind!", o.area_m2, o.created_at,
                  (SELECT count(DISTINCT lo.tender_id) FROM core.lots lo WHERE lo.object_id = o.id)
                    AS "tenders!"
           FROM core.objects o
           ORDER BY o.id DESC
           LIMIT $1"#,
        crate::probe_limit(MAX_ROWS)
    )
    .fetch_all(db)
    .await?;

    let page = Page::probe(rows, MAX_ROWS);
    crate::warn_if_truncated(page.truncated, "purge::list_objects");
    Ok(page)
}

pub async fn tender_exists(db: &Db, id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM core.tenders WHERE id = $1) AS "exists!""#,
        id
    )
    .fetch_one(db)
    .await
}

pub async fn object_exists(db: &Db, id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM core.objects WHERE id = $1) AS "exists!""#,
        id
    )
    .fetch_one(db)
    .await
}

/// Сколько строк удалено, по таблицам схемы `core`; таблицы без удалений
/// в перечень не попадают.
pub type Deleted = BTreeMap<String, i64>;

/// Очистка области: `ids = None` - все записи области, `Some(ids)` - только
/// перечисленные (для [`PurgeScope::Everything`] и
/// [`PurgeScope::Notifications`] перечень не нужен).
///
/// Одна транзакция под актором: построчные события удалений и сводное
/// `core.data_purge` уходят в аудит с тем же `actor_id`. Отказ функции
/// (например, вызов без актора) откатывает и снятие сторожей.
pub async fn purge(
    db: &Db,
    actor: Uuid,
    scope: PurgeScope,
    ids: Option<&[Uuid]>,
) -> Result<Deleted, sqlx::Error> {
    let deleted = crate::with_actor(db, actor, async |tx| {
        sqlx::query_scalar!(
            r#"SELECT core.purge_data($1, $2::uuid[]) AS "deleted!""#,
            scope.as_str(),
            ids
        )
        .fetch_one(&mut *tx)
        .await
    })
    .await?;

    serde_json::from_value(deleted).map_err(|e| sqlx::Error::Decode(Box::new(e)))
}

/// Отключение демо-учеток (`*@tou.demo`) кроме самого актора.
///
/// Не удаление: к записям привязаны сессии, роли и прошлые действия в
/// аудите, а удаления пользователя в системе нет и не будет (W-07). Актор
/// остается действующим по той же причине, что и при обычном отключении:
/// вернуть записи было бы некому.
pub async fn deactivate_demo_accounts(db: &Db, actor: Uuid) -> Result<u64, sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        let done = sqlx::query!(
            "UPDATE core.users SET is_active = false
             WHERE is_active AND id <> $1 AND email::text LIKE '%' || $2",
            actor,
            DEMO_EMAIL_SUFFIX
        )
        .execute(&mut *tx)
        .await?;
        Ok(done.rows_affected())
    })
    .await
}
