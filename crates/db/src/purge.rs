//! Очистка данных стенда администратором (М15, FR-1503; след - FR-1601).
//!
//! Стенд, наполненный `api seed` под демонстрацию, стал рабочим: демо-объекты,
//! тендеры во всех статусах, заявки и протоколы должны исчезнуть до прихода
//! настоящих процедур. Штатных средств для этого в системе нет намеренно:
//! объявленный тендер отменяется, а не удаляется (FR-305), протоколы и досье
//! append-only (FR-702, INV-042), у роли приложения нет права DELETE на них.
//!
//! Поэтому само удаление живет в БД - `core.purge_data()` под владельцем
//! схемы (миграция `20260902120000_admin_data_purge_kinds.sql`): сторожа
//! append-only снимаются на одну транзакцию, аудит-триггеры остаются, и
//! сверху ложится сводное событие `core.data_purge`. Здесь - только вызов,
//! обзор того, что уйдет, и перечни записей для точечного удаления.
//! Разрешение на вызов дает http-слой (`ALLOW_DATA_PURGE`).

use std::collections::BTreeMap;

use time::OffsetDateTime;
use uuid::Uuid;

use crate::{Db, MAX_ROWS, Page};

/// Домен демо-учеток `api seed` (Прил. Б): один пароль на всех, и на рабочем
/// стенде такие записи должны быть отключены до первого настоящего входа.
pub const DEMO_EMAIL_SUFFIX: &str = "@tou.demo";

/// Область очистки: весь стенд либо вид данных, с которого начинается
/// удаление. Все, что держится на удаляемых записях, уходит с ними по
/// внешним ключам.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurgeScope {
    /// Все процедуры и объекты стенда
    Everything,
    /// Объекты вместе с тендерами, где они выставлены лотом, участками
    /// и заявками особого порядка по ним
    Objects,
    /// Тендеры со всем, что на них висит; объекты остаются
    Tenders,
    /// Лоты с заявками, торгами и договорами по ним
    Lots,
    /// Заявки с файлами, ценой, журналом, голосами, торгами, где заявка -
    /// победитель или очередной ход, договором, счетом и проводками
    Applications,
    /// Протоколы с договорами по ним
    Protocols,
    /// Торги со ставками и кругом участников
    Auctions,
    /// Договоры с актами, сверкой, допсоглашениями, льготой, счетом
    /// и проводками
    Contracts,
    Acts,
    LedgerEntries,
    /// Заявки особого порядка с заключениями, решениями, досье и
    /// инвестиционными договорами
    SpecialRequests,
    /// Земельные участки с заявками, решениями и договорами по ним
    LandPlots,
    InvestmentContracts,
    DossierItems,
    PublicRecords,
    Obligations,
    Notifications,
}

impl PurgeScope {
    /// Виды данных вкладки «Данные» - все области, кроме полной очистки.
    pub const KINDS: [PurgeScope; 16] = [
        PurgeScope::Objects,
        PurgeScope::Tenders,
        PurgeScope::Lots,
        PurgeScope::Applications,
        PurgeScope::Protocols,
        PurgeScope::Auctions,
        PurgeScope::Contracts,
        PurgeScope::Acts,
        PurgeScope::LedgerEntries,
        PurgeScope::SpecialRequests,
        PurgeScope::LandPlots,
        PurgeScope::InvestmentContracts,
        PurgeScope::DossierItems,
        PurgeScope::PublicRecords,
        PurgeScope::Obligations,
        PurgeScope::Notifications,
    ];

    /// Имя области для БД - совпадает с проверкой внутри `core.purge_data`
    /// и с именем таблицы схемы `core` у видов данных.
    pub fn as_str(self) -> &'static str {
        match self {
            PurgeScope::Everything => "everything",
            PurgeScope::Objects => "objects",
            PurgeScope::Tenders => "tenders",
            PurgeScope::Lots => "lots",
            PurgeScope::Applications => "applications",
            PurgeScope::Protocols => "protocols",
            PurgeScope::Auctions => "auctions",
            PurgeScope::Contracts => "contracts",
            PurgeScope::Acts => "acts",
            PurgeScope::LedgerEntries => "ledger_entries",
            PurgeScope::SpecialRequests => "special_requests",
            PurgeScope::LandPlots => "land_plots",
            PurgeScope::InvestmentContracts => "investment_contracts",
            PurgeScope::DossierItems => "dossier_items",
            PurgeScope::PublicRecords => "public_records",
            PurgeScope::Obligations => "obligations",
            PurgeScope::Notifications => "notifications",
        }
    }

    /// Таблица схемы `core`, которой принадлежат записи вида; у полной
    /// очистки ее нет.
    pub fn table(self) -> Option<&'static str> {
        match self {
            PurgeScope::Everything => None,
            kind => Some(kind.as_str()),
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

/// Запись вида данных в перечне на удаление: чем ее опознать. Строки
/// собраны из данных самой записи и ее родителей (заголовок тендера, имя
/// участника, номер договора); коды статусов - как в БД.
#[derive(Debug, Clone)]
pub struct AdminRecord {
    pub id: Uuid,
    pub title: String,
    pub title_kk: Option<String>,
    pub details: Option<String>,
    pub created_at: Option<OffsetDateTime>,
}

/// Одна выборка перечня: столбцы у всех видов одни, свежие сверху
/// (`id` - uuid v7, монотонен по времени создания).
macro_rules! records {
    ($db:expr, $sql:literal) => {
        sqlx::query_as!(AdminRecord, $sql, crate::probe_limit(MAX_ROWS))
            .fetch_all($db)
            .await
    };
}

/// Записи одного вида для точечного удаления; у полной очистки перечня нет.
pub async fn list_records(db: &Db, kind: PurgeScope) -> Result<Page<AdminRecord>, sqlx::Error> {
    let rows = match kind {
        PurgeScope::Everything => Vec::new(),
        PurgeScope::Objects => records!(
            db,
            r#"SELECT id, name AS "title!", name_kk AS "title_kk?",
                      kind::text || ' · ' || address AS "details?", created_at AS "created_at?"
               FROM core.objects ORDER BY id DESC LIMIT $1"#
        )?,
        PurgeScope::Tenders => records!(
            db,
            r#"SELECT id, title AS "title!", title_kk AS "title_kk?",
                      status::text AS "details?", created_at AS "created_at?"
               FROM core.tenders ORDER BY id DESC LIMIT $1"#
        )?,
        PurgeScope::Lots => records!(
            db,
            r#"SELECT l.id, l.purpose AS "title!", l.purpose_kk AS "title_kk?",
                      '№' || l.seq || ' · ' || t.title AS "details?", t.created_at AS "created_at?"
               FROM core.lots l JOIN core.tenders t ON t.id = l.tender_id
               ORDER BY l.id DESC LIMIT $1"#
        )?,
        PurgeScope::Applications => records!(
            db,
            r#"SELECT a.id, u.full_name AS "title!", NULL::text AS "title_kk?",
                      t.title || ' · ' || a.status::text AS "details?",
                      a.submitted_at AS "created_at?"
               FROM core.applications a
               JOIN core.users u ON u.id = a.participant_id
               JOIN core.tenders t ON t.id = a.tender_id
               ORDER BY a.id DESC LIMIT $1"#
        )?,
        PurgeScope::Protocols => records!(
            db,
            r#"SELECT p.id, p.kind::text || ' ' || coalesce(p.number, '') AS "title!",
                      NULL::text AS "title_kk?", t.title AS "details?",
                      p.generated_at AS "created_at?"
               FROM core.protocols p JOIN core.tenders t ON t.id = p.tender_id
               ORDER BY p.id DESC LIMIT $1"#
        )?,
        PurgeScope::Auctions => records!(
            db,
            r#"SELECT au.id, t.title AS "title!", t.title_kk AS "title_kk?",
                      '№' || l.seq || ' · ' || au.status::text || ' · ' || au.starting_bid::text
                        AS "details?",
                      coalesce(au.started_at, au.ends_at) AS "created_at?"
               FROM core.auctions au
               JOIN core.lots l ON l.id = au.lot_id
               JOIN core.tenders t ON t.id = l.tender_id
               ORDER BY au.id DESC LIMIT $1"#
        )?,
        PurgeScope::Contracts => records!(
            db,
            r#"SELECT c.id, coalesce(c.reg_number, c.id::text) AS "title!", NULL::text AS "title_kk?",
                      u.full_name || ' · ' || c.status::text || ' · ' || c.monthly_rate::text
                        AS "details?",
                      c.created_at AS "created_at?"
               FROM core.contracts c JOIN core.users u ON u.id = c.tenant_id
               ORDER BY c.id DESC LIMIT $1"#
        )?,
        PurgeScope::Acts => records!(
            db,
            r#"SELECT a.id, a.kind::text || ' ' || a.act_date::text AS "title!",
                      NULL::text AS "title_kk?", coalesce(c.reg_number, c.id::text) AS "details?",
                      a.created_at AS "created_at?"
               FROM core.acts a JOIN core.contracts c ON c.id = a.contract_id
               ORDER BY a.id DESC LIMIT $1"#
        )?,
        PurgeScope::LedgerEntries => records!(
            db,
            r#"SELECT e.id, e.op::text || ' ' || coalesce(e.debit, e.credit)::text AS "title!",
                      NULL::text AS "title_kk?",
                      acc.kind::text || ' · ' || coalesce(e.rule_ref, '') AS "details?",
                      e.occurred_at AS "created_at?"
               FROM core.ledger_entries e JOIN core.ledger_accounts acc ON acc.id = e.account_id
               ORDER BY e.id DESC LIMIT $1"#
        )?,
        PurgeScope::SpecialRequests => records!(
            db,
            r#"SELECT s.id, u.full_name AS "title!", NULL::text AS "title_kk?",
                      s.category || ' · ' || s.status::text AS "details?",
                      s.created_at AS "created_at?"
               FROM core.special_requests s JOIN core.users u ON u.id = s.applicant_id
               ORDER BY s.id DESC LIMIT $1"#
        )?,
        PurgeScope::LandPlots => records!(
            db,
            r#"SELECT lp.id, coalesce(lp.cadastral_number, o.name) AS "title!",
                      o.name_kk AS "title_kk?", lp.designation AS "details?",
                      lp.created_at AS "created_at?"
               FROM core.land_plots lp JOIN core.objects o ON o.id = lp.object_id
               ORDER BY lp.id DESC LIMIT $1"#
        )?,
        PurgeScope::InvestmentContracts => records!(
            db,
            r#"SELECT ic.id, coalesce(c.reg_number, c.id::text) AS "title!",
                      NULL::text AS "title_kk?",
                      ic.investment_amount::text || ' · ' || ic.term_months::text AS "details?",
                      ic.created_at AS "created_at?"
               FROM core.investment_contracts ic JOIN core.contracts c ON c.id = ic.contract_id
               ORDER BY ic.id DESC LIMIT $1"#
        )?,
        PurgeScope::DossierItems => records!(
            db,
            r#"SELECT d.id, coalesce(d.title, d.kind) AS "title!", NULL::text AS "title_kk?",
                      d.kind || ' · ' || coalesce(t.title, s.category, '') AS "details?",
                      d.occurred_at AS "created_at?"
               FROM core.dossier_items d
               LEFT JOIN core.tenders t ON t.id = d.tender_id
               LEFT JOIN core.special_requests s ON s.id = d.special_request_id
               ORDER BY d.id DESC LIMIT $1"#
        )?,
        PurgeScope::PublicRecords => records!(
            db,
            r#"SELECT id, coalesce(title, kind::text) AS "title!", NULL::text AS "title_kk?",
                      kind::text AS "details?", published_at AS "created_at?"
               FROM core.public_records ORDER BY id DESC LIMIT $1"#
        )?,
        PurgeScope::Obligations => records!(
            db,
            r#"SELECT id, action AS "title!", NULL::text AS "title_kk?",
                      rule_ref || ' · ' || status::text || ' · ' || assignee_role::text AS "details?",
                      due_at AS "created_at?"
               FROM core.obligations ORDER BY id DESC LIMIT $1"#
        )?,
        PurgeScope::Notifications => records!(
            db,
            r#"SELECT n.id, n.kind AS "title!", NULL::text AS "title_kk?",
                      u.email::text || coalesce(' · ' || (n.payload ->> 'tender_title'), '')
                        AS "details?",
                      n.created_at AS "created_at?"
               FROM core.notifications n JOIN core.users u ON u.id = n.user_id
               ORDER BY n.id DESC LIMIT $1"#
        )?,
    };

    let page = Page::probe(rows, MAX_ROWS);
    crate::warn_if_truncated(page.truncated, "purge::list_records");
    Ok(page)
}

/// Есть ли запись вида с таким идентификатором.
///
/// Имя таблицы подставляется в текст запроса: оно берется из закрытого
/// перечня [`PurgeScope::table`], а не с провода, поэтому подстановка
/// безопасна. Шестнадцать проверенных макросом запросов ради `EXISTS`
/// раздули бы слепок `.sqlx` без пользы.
pub async fn record_exists(db: &Db, kind: PurgeScope, id: Uuid) -> Result<bool, sqlx::Error> {
    let Some(table) = kind.table() else {
        return Ok(false);
    };
    // AssertSqlSafe - осознанно: динамическая часть - имя таблицы из
    // закрытого перечня выше, пользовательского ввода в строке нет
    sqlx::query_scalar::<_, bool>(sqlx::AssertSqlSafe(format!(
        "SELECT EXISTS (SELECT 1 FROM core.{table} WHERE id = $1)"
    )))
    .bind(id)
    .fetch_one(db)
    .await
}

/// Сколько строк удалено, по таблицам схемы `core`; таблицы без удалений
/// в перечень не попадают.
pub type Deleted = BTreeMap<String, i64>;

/// Очистка области: `ids = None` - все записи области, `Some(ids)` - только
/// перечисленные (для [`PurgeScope::Everything`] перечень не нужен).
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

#[cfg(test)]
mod tests {
    use super::*;

    /// У каждого вида данных есть таблица, и имя области - это ее имя:
    /// `record_exists` подставляет его в запрос, и расхождение с каталогом
    /// всплыло бы только на стенде.
    #[test]
    fn every_kind_names_its_table() {
        for kind in PurgeScope::KINDS {
            assert_eq!(kind.table(), Some(kind.as_str()));
            assert!(
                kind.as_str()
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'_'),
                "имя таблицы {} годится в текст запроса без кавычек",
                kind.as_str()
            );
        }
        assert_eq!(PurgeScope::Everything.table(), None);
    }
}
