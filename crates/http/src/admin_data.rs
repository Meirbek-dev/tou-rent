//! Очистка данных стенда из кабинета админа (М15, FR-1503; след - FR-1601).
//!
//! Стенд, наполненный `api seed` под демонстрацию, возвращается в пустое
//! состояние перед вводом в работу - целиком, по видам данных или по одной
//! записи любого вида. Удаление живет в БД (`core.purge_data`, см.
//! `tou_db::purge`); здесь - право роли, рубеж намерения (`ALLOW_DATA_PURGE`),
//! слово подтверждения для массовых операций и перечни записей.

use std::collections::BTreeMap;

use axum::extract::State;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tou_db::purge::PurgeScope;
use tou_domain::policy::Action;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::request::{Json, Path, Query};
use crate::state::AppState;

/// Сколько строк каждого вида лежит на стенде - что уйдет при очистке.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminDataCountsDto {
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
    /// Действующие демо-учетки `*@tou.demo` (Прил. Б)
    pub demo_accounts: i64,
}

impl From<tou_db::purge::DataCounts> for AdminDataCountsDto {
    fn from(c: tou_db::purge::DataCounts) -> Self {
        Self {
            objects: c.objects,
            tenders: c.tenders,
            lots: c.lots,
            applications: c.applications,
            protocols: c.protocols,
            auctions: c.auctions,
            contracts: c.contracts,
            acts: c.acts,
            ledger_entries: c.ledger_entries,
            special_requests: c.special_requests,
            land_plots: c.land_plots,
            investment_contracts: c.investment_contracts,
            dossier_items: c.dossier_items,
            public_records: c.public_records,
            obligations: c.obligations,
            notifications: c.notifications,
            demo_accounts: c.demo_accounts,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminDataOverviewDto {
    /// Очистка разрешена конфигурацией стенда (`ALLOW_DATA_PURGE`); без нее
    /// кнопки в кабинете не действуют, а маршруты отказывают
    pub purge_enabled: bool,
    pub counts: AdminDataCountsDto,
}

/// Слово, которым подтверждается массовая очистка. Одно на все локали:
/// его вводят с клавиатуры, и оно должно совпасть с тем, что просит экран.
pub const PURGE_CONFIRMATION: &str = "purge";

/// Область очистки: весь стенд либо вид данных вкладки «Данные». Удаляемая
/// запись уносит все, что на ней держится по внешним ключам.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdminPurgeScope {
    /// Все процедуры и объекты стенда
    #[default]
    Everything,
    Objects,
    Tenders,
    Lots,
    Applications,
    Protocols,
    Auctions,
    Contracts,
    Acts,
    LedgerEntries,
    SpecialRequests,
    LandPlots,
    InvestmentContracts,
    DossierItems,
    PublicRecords,
    Obligations,
    Notifications,
}

impl From<AdminPurgeScope> for PurgeScope {
    fn from(scope: AdminPurgeScope) -> Self {
        match scope {
            AdminPurgeScope::Everything => PurgeScope::Everything,
            AdminPurgeScope::Objects => PurgeScope::Objects,
            AdminPurgeScope::Tenders => PurgeScope::Tenders,
            AdminPurgeScope::Lots => PurgeScope::Lots,
            AdminPurgeScope::Applications => PurgeScope::Applications,
            AdminPurgeScope::Protocols => PurgeScope::Protocols,
            AdminPurgeScope::Auctions => PurgeScope::Auctions,
            AdminPurgeScope::Contracts => PurgeScope::Contracts,
            AdminPurgeScope::Acts => PurgeScope::Acts,
            AdminPurgeScope::LedgerEntries => PurgeScope::LedgerEntries,
            AdminPurgeScope::SpecialRequests => PurgeScope::SpecialRequests,
            AdminPurgeScope::LandPlots => PurgeScope::LandPlots,
            AdminPurgeScope::InvestmentContracts => PurgeScope::InvestmentContracts,
            AdminPurgeScope::DossierItems => PurgeScope::DossierItems,
            AdminPurgeScope::PublicRecords => PurgeScope::PublicRecords,
            AdminPurgeScope::Obligations => PurgeScope::Obligations,
            AdminPurgeScope::Notifications => PurgeScope::Notifications,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AdminPurgeRequest {
    /// Слово подтверждения - ровно [`PURGE_CONFIRMATION`]
    #[schema(example = "purge")]
    pub confirmation: String,
    /// Что стирать; без поля - весь стенд
    #[serde(default)]
    pub scope: AdminPurgeScope,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminPurgeResultDto {
    /// Удалено строк по таблицам схемы `core` (таблицы без удалений опущены)
    pub deleted: BTreeMap<String, i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminDemoAccountsDto {
    /// Сколько демо-учеток отключено
    pub deactivated: u64,
}

/// Запись вида данных в перечне на удаление: чем ее опознать.
#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRecordDto {
    pub id: Uuid,
    /// Главная строка: заголовок, имя, номер, вид
    pub title: String,
    /// Казахский вариант заголовка, если он хранится
    pub title_kk: Option<String>,
    /// Контекст: чей, по какому тендеру или договору, в каком статусе -
    /// коды статусов как в БД
    pub details: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub created_at: Option<OffsetDateTime>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminRecordPageDto {
    pub items: Vec<AdminRecordDto>,
    /// Перечень обрезан потолком строк
    pub truncated: bool,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct AdminRecordsParams {
    /// Вид данных; `everything` перечнем не является
    pub kind: AdminPurgeScope,
}

/// Обзор данных стенда: что и в каком количестве уйдет при очистке.
#[utoipa::path(
    get,
    path = "/api/v1/admin/data",
    tag = "admin",
    responses(
        (status = 200, description = "Обзор данных стенда", body = AdminDataOverviewDto),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
    )
)]
pub async fn data_overview(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<AdminDataOverviewDto>, ApiError> {
    user.require(Action::DataPurge)?;

    let counts = tou_db::purge::counts(&state.db).await?;
    Ok(Json(AdminDataOverviewDto {
        purge_enabled: state.data_purge_enabled,
        counts: counts.into(),
    }))
}

/// Записи одного вида, свежие сверху, - для точечного удаления.
#[utoipa::path(
    get,
    path = "/api/v1/admin/data/records",
    tag = "admin",
    params(AdminRecordsParams),
    responses(
        (status = 200, description = "Перечень записей вида", body = AdminRecordPageDto),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
        (status = 422, description = "`everything` перечнем не является", body = crate::error::Problem),
    )
)]
pub async fn list_records(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(params): Query<AdminRecordsParams>,
) -> Result<Json<AdminRecordPageDto>, ApiError> {
    user.require(Action::DataPurge)?;
    let kind = record_kind(params.kind)?;

    let page = tou_db::purge::list_records(&state.db, kind).await?;
    let truncated = page.truncated;
    let items = page
        .into_iter()
        .map(|r| AdminRecordDto {
            id: r.id,
            title: r.title,
            title_kk: r.title_kk,
            details: r.details,
            created_at: r.created_at,
        })
        .collect();
    Ok(Json(AdminRecordPageDto { items, truncated }))
}

/// Область, у которой есть записи: `everything` - это не вид данных.
fn record_kind(scope: AdminPurgeScope) -> Result<PurgeScope, ApiError> {
    if scope == AdminPurgeScope::Everything {
        return Err(ApiError::Validation(
            "область everything перечнем записей не является - укажите вид данных".to_owned(),
        ));
    }
    Ok(scope.into())
}

/// Рубеж намерения: без `ALLOW_DATA_PURGE` право роли до удаления не доводит.
/// Тот же рубеж - у правки сроков тендера ([`crate::admin_schedule`]).
pub(crate) fn require_purge_enabled(state: &AppState) -> Result<(), ApiError> {
    if state.data_purge_enabled {
        Ok(())
    } else {
        Err(ApiError::Validation(
            "очистка данных выключена: задайте ALLOW_DATA_PURGE=1 в окружении api \
             и перезапустите его"
                .to_owned(),
        ))
    }
}

/// Массовая очистка: весь стенд либо все записи одного вида со всем, что
/// на них держится, одной транзакцией.
///
/// Учетные записи, роли, состав комиссии, объявление на главной, справочники
/// и журнал аудита остаются. Файлы в `dossiers` тоже: бакет под Object Lock
/// (INV-042), а без строки метаданных объект недостижим (A-095).
#[utoipa::path(
    post,
    path = "/api/v1/admin/data/purge",
    tag = "admin",
    request_body = AdminPurgeRequest,
    responses(
        (status = 200, description = "Область очищена", body = AdminPurgeResultDto),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
        (status = 422, description = "Очистка выключена или слово подтверждения не совпало",
         body = crate::error::Problem),
    )
)]
pub async fn purge_data(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<AdminPurgeRequest>,
) -> Result<Json<AdminPurgeResultDto>, ApiError> {
    user.require(Action::DataPurge)?;
    require_purge_enabled(&state)?;
    if body.confirmation.trim() != PURGE_CONFIRMATION {
        return Err(ApiError::Validation(format!(
            "подтверждение не совпало: ожидается слово «{PURGE_CONFIRMATION}»"
        )));
    }

    let deleted = tou_db::purge::purge(&state.db, user.id(), body.scope.into(), None).await?;
    tracing::warn!(actor = %user.id(), scope = ?body.scope, ?deleted, "массовая очистка администратором");
    Ok(Json(AdminPurgeResultDto { deleted }))
}

/// Удаление одной записи любого вида со всем, что на ней держится: заявка
/// уносит файлы, цену, журнал, голоса, торги и договор по ней; лот -
/// заявки и торги; объект - тендеры, где он выставлен лотом. Слова
/// подтверждения нет - его роль играет диалог в кабинете; рубеж намерения
/// (`ALLOW_DATA_PURGE`) действует и здесь.
#[utoipa::path(
    delete,
    path = "/api/v1/admin/data/records/{kind}/{id}",
    tag = "admin",
    params(
        ("kind" = AdminPurgeScope, Path, description = "Вид данных"),
        ("id" = Uuid, Path, description = "Запись"),
    ),
    responses(
        (status = 200, description = "Запись удалена", body = AdminPurgeResultDto),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
        (status = 404, description = "Запись не найдена", body = crate::error::Problem),
        (status = 422, description = "Очистка выключена или вид без записей",
         body = crate::error::Problem),
    )
)]
pub async fn purge_record(
    user: CurrentUser,
    State(state): State<AppState>,
    Path((kind, id)): Path<(AdminPurgeScope, Uuid)>,
) -> Result<Json<AdminPurgeResultDto>, ApiError> {
    user.require(Action::DataPurge)?;
    require_purge_enabled(&state)?;
    let kind = record_kind(kind)?;
    // Проверка до вызова: очистка несуществующей записи оставила бы
    // в аудите пустое сводное событие
    if !tou_db::purge::record_exists(&state.db, kind, id).await? {
        return Err(ApiError::NotFound);
    }

    let deleted = tou_db::purge::purge(&state.db, user.id(), kind, Some(&[id])).await?;
    tracing::warn!(actor = %user.id(), kind = kind.as_str(), record_id = %id, ?deleted, "запись удалена администратором");
    Ok(Json(AdminPurgeResultDto { deleted }))
}

/// Отключение демо-учеток `*@tou.demo` (Прил. Б) кроме своей: у них один
/// пароль на всех, и на рабочем стенде они не должны входить. Записи не
/// удаляются - их можно вернуть обычным переключением (W-07).
#[utoipa::path(
    post,
    path = "/api/v1/admin/demo-accounts/deactivate",
    tag = "admin",
    responses(
        (status = 200, description = "Демо-учетки отключены", body = AdminDemoAccountsDto),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
    )
)]
pub async fn deactivate_demo_accounts(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<AdminDemoAccountsDto>, ApiError> {
    user.require(Action::UserManage)?;

    let deactivated = tou_db::purge::deactivate_demo_accounts(&state.db, user.id()).await?;
    tracing::info!(actor = %user.id(), deactivated, "демо-учетки отключены");
    Ok(Json(AdminDemoAccountsDto { deactivated }))
}

#[cfg(test)]
mod tests {
    use tou_domain::policy::is_allowed;
    use tou_domain::role::Role;

    use super::*;

    /// Маршруты очистки зарегистрированы в контракте: путь в документе
    /// означает и работающий маршрут, и то, что кабинет соберет запрос
    /// после кодогена (G5).
    #[test]
    fn data_purge_routes_are_registered_in_the_contract() {
        let json = crate::openapi().to_json().expect("сериализация контракта");
        for path in [
            "/api/v1/admin/data",
            "/api/v1/admin/data/records",
            "/api/v1/admin/data/purge",
            "/api/v1/admin/data/records/{kind}/{id}",
            "/api/v1/admin/demo-accounts/deactivate",
        ] {
            assert!(json.contains(path), "маршрут {path} не попал в контракт");
        }
        for schema in [
            "AdminDataOverviewDto",
            "AdminPurgeResultDto",
            "AdminRecordPageDto",
        ] {
            assert!(json.contains(schema), "схема {schema} не попала в контракт");
        }
    }

    /// Очистка данных - право одного admin (INV-POL-01): организатор ведет
    /// тендер, но стирать процедуры целиком ему нельзя даже на стенде.
    #[test]
    fn data_purge_is_admin_only() {
        for role in Role::ALL {
            assert_eq!(
                is_allowed(role, Action::DataPurge),
                role == Role::Admin,
                "право DataPurge у роли {}",
                role.as_str()
            );
        }
    }

    /// Область на проводе: без поля - весь стенд; имена областей совпадают
    /// с проверкой внутри `core.purge_data`, а каждому виду данных
    /// вкладки соответствует область.
    #[test]
    fn purge_scope_defaults_to_everything_and_matches_db_names() {
        let request: AdminPurgeRequest =
            serde_json::from_str(r#"{"confirmation":"purge"}"#).expect("тело без области");
        assert_eq!(request.scope, AdminPurgeScope::Everything);

        for kind in PurgeScope::KINDS {
            let wire = serde_json::to_string(&serde_json::json!(kind.as_str())).expect("json");
            let scope: AdminPurgeScope = serde_json::from_str(&wire)
                .unwrap_or_else(|_| panic!("нет области {}", kind.as_str()));
            let back: PurgeScope = scope.into();
            assert_eq!(back.as_str(), kind.as_str());
        }
    }

    /// `everything` не перечень: запрос записей этой области отказывает,
    /// а не молча отдает пустую страницу.
    #[test]
    fn everything_is_not_a_record_kind() {
        assert!(record_kind(AdminPurgeScope::Everything).is_err());
        assert!(record_kind(AdminPurgeScope::Applications).is_ok());
    }
}
