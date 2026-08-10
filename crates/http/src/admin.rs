//! Администрирование пользователей и ролей (FR-1503, М15).
//! Каждое изменение ролей фиксирует audit-триггер `core.role_grants` (INV-AUDIT).
//!
//! Здесь же - состояние hash-цепочки аудита (INV-A01): сверку делает фоновый
//! воркер, но до кабинета админа ее итог раньше не доходил вовсе.

use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tou_domain::policy::Action;
use tou_domain::role::Role;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::auth::UserDto;
use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::request::{Json, Path, Query};
use crate::state::AppState;

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListUsersParams {
    /// Cursor: id последнего элемента предыдущей страницы (uuid v7 упорядочен по времени)
    pub after: Option<Uuid>,
    /// 1..=100, по умолчанию 50
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserPage {
    pub items: Vec<UserDto>,
    /// Cursor для следующей страницы; None - данных больше нет
    pub next_after: Option<Uuid>,
}

/// Список пользователей (cursor-пагинация, ТЗ § 7).
#[utoipa::path(
    get,
    path = "/api/v1/admin/users",
    tag = "admin",
    params(ListUsersParams),
    responses(
        (status = 200, description = "Страница пользователей", body = UserPage),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
    )
)]
pub async fn list_users(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(params): Query<ListUsersParams>,
) -> Result<Json<UserPage>, ApiError> {
    user.require(Action::UserManage)?;

    let limit = params.limit.unwrap_or(50).clamp(1, 100);
    let records = tou_db::users::list_users(&state.db, params.after, limit).await?;

    let next_after = (records.len() as i64 == limit)
        .then(|| records.last().map(|u| u.id))
        .flatten();

    // Роли всей страницы одним запросом (без N+1)
    let ids: Vec<Uuid> = records.iter().map(|u| u.id).collect();
    let mut roles_by_user = tou_db::users::roles_for(&state.db, &ids).await?;

    let items = records
        .into_iter()
        .map(|record| {
            let roles = roles_by_user.remove(&record.id).unwrap_or_default();
            UserDto::new(record, roles)
        })
        .collect();

    Ok(Json(UserPage { items, next_after }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GrantRoleRequest {
    /// Роль из enum домена; `guest` не хранится и не назначается
    #[schema(value_type = String, example = "organizer")]
    pub role: String,
}

/// Назначить роль пользователю (FR-1503).
#[utoipa::path(
    post,
    path = "/api/v1/admin/users/{user_id}/roles",
    tag = "admin",
    params(("user_id" = Uuid, Path, description = "Пользователь")),
    request_body = GrantRoleRequest,
    responses(
        (status = 204, description = "Роль назначена (или уже была)"),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
        (status = 404, description = "Пользователь не найден", body = crate::error::Problem),
        (status = 422, description = "Неизвестная роль", body = crate::error::Problem),
    )
)]
pub async fn grant_role(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Json(body): Json<GrantRoleRequest>,
) -> Result<StatusCode, ApiError> {
    user.require(Action::RoleGrant)?;
    let role = parse_grantable_role(&body.role)?;

    tou_db::users::find_by_id(&state.db, user_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    tou_db::users::grant_role(&state.db, user.id(), user_id, role).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Отозвать роль (FR-1503).
#[utoipa::path(
    delete,
    path = "/api/v1/admin/users/{user_id}/roles/{role}",
    tag = "admin",
    params(
        ("user_id" = Uuid, Path, description = "Пользователь"),
        ("role" = String, Path, description = "Роль (snake_case)"),
    ),
    responses(
        (status = 204, description = "Роль отозвана (или ее не было)"),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
        (status = 422, description = "Неизвестная роль", body = crate::error::Problem),
    )
)]
pub async fn revoke_role(
    user: CurrentUser,
    State(state): State<AppState>,
    Path((user_id, role)): Path<(Uuid, String)>,
) -> Result<StatusCode, ApiError> {
    user.require(Action::RoleGrant)?;
    let role = parse_grantable_role(&role)?;
    tou_db::users::revoke_role(&state.db, user.id(), user_id, role).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Одноразовый пароль в ответе на сброс - единственный раз, когда он вообще
/// существует в открытом виде.
#[derive(Debug, Serialize, ToSchema)]
pub struct PasswordResetDto {
    /// Показывается админу один раз и нигде не сохраняется
    pub password: String,
}

/// Сброс пароля админом (W-07, FR-1503).
///
/// Канала доставки у контура 1 нет (почта - T41), поэтому «ссылка на
/// восстановление» здесь была бы выдумкой. Честный вариант без канала один:
/// систему генерирует одноразовый пароль сама, показывает его админу ровно
/// в ответе на нажатие кнопки и больше нигде - ни в логе, ни в БД, ни в
/// аудите (там от него остается только отпечаток, см. миграцию
/// `20260811000000_user_account_audit.sql`). Дальше пароль передает человек
/// человеку тем каналом, которым он и так подтверждает личность заявителя,
/// а владелец учетной записи меняет его через `/api/v1/auth/password`.
///
/// Пароль не придумывает админ: придуманный человеком он и слаб, и известен
/// придумавшему заранее - тогда «сброс» ничем не отличается от «узнать
/// чужой пароль». Сгенерированный известен админу ровно так же, но только
/// до первой смены, и владелец видит по журналу, что запись трогали.
#[utoipa::path(
    post,
    path = "/api/v1/admin/users/{user_id}/password-reset",
    tag = "admin",
    params(("user_id" = Uuid, Path, description = "Пользователь")),
    responses(
        (status = 200, description = "Одноразовый пароль выдан", body = PasswordResetDto),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
        (status = 404, description = "Пользователь не найден", body = crate::error::Problem),
        (status = 422, description = "У записи нет локального пароля", body = crate::error::Problem),
    )
)]
pub async fn reset_password(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<PasswordResetDto>, ApiError> {
    user.require(Action::UserManage)?;

    let subject = tou_db::users::find_by_id(&state.db, user_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if subject.password_hash.is_none() {
        // Запись заведена внешним провайдером (FR-1502): выдать ей локальный
        // пароль значит открыть вход мимо провайдера - мимо его политик,
        // его отключения при увольнении и его же журнала
        return Err(ApiError::Validation(
            "учетная запись входит через внешнего провайдера - пароль сбрасывается у него"
                .to_owned(),
        ));
    }

    let password = one_time_password();
    let to_hash = password.clone();
    let password_hash = tokio::task::spawn_blocking(move || crate::auth::hash_password(&to_hash))
        .await
        .map_err(ApiError::internal)??;

    // Актор - админ: в аудите сброс обязан отличаться от смены пароля
    // самим владельцем (W-07)
    let reset = tou_db::users::set_password(&state.db, user.id(), user_id, &password_hash).await?;
    if !reset {
        return Err(ApiError::NotFound);
    }

    // В логе - только факт и участники: сам пароль сюда не попадает никогда
    tracing::warn!(actor = %user.id(), user_id = %user_id, "пароль сброшен админом");
    Ok(Json(PasswordResetDto { password }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetActiveRequest {
    /// `false` - учетная запись отключается, `true` - возвращается
    pub is_active: bool,
}

/// Деактивация и возврат учетной записи (W-07).
///
/// Снятие ролей уволившемуся не отключает саму запись: она входит, видит
/// публичную часть и остается субъектом своих прошлых заявок. Отключение
/// закрывает оба пути сразу - и вход, и уже открытую сессию (`CurrentUser`
/// сверяет `is_active` на каждом запросе).
///
/// Удаления записи здесь нет и не будет: к ней привязаны поданные заявки,
/// договоры и депозит, и «удалить пользователя» означало бы разорвать
/// доказательную базу.
#[utoipa::path(
    put,
    path = "/api/v1/admin/users/{user_id}/active",
    tag = "admin",
    params(("user_id" = Uuid, Path, description = "Пользователь")),
    request_body = SetActiveRequest,
    responses(
        (status = 204, description = "Состояние изменено"),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
        (status = 404, description = "Пользователь не найден", body = crate::error::Problem),
        (status = 422, description = "Отключение самого себя", body = crate::error::Problem),
    )
)]
pub async fn set_user_active(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Json(body): Json<SetActiveRequest>,
) -> Result<StatusCode, ApiError> {
    user.require(Action::UserManage)?;

    // Отключить себя - значит выйти из системы навсегда: вернуть запись
    // некому, право на это есть только у админов, а других может не быть
    if user_id == user.id() && !body.is_active {
        return Err(ApiError::Validation(
            "нельзя отключить собственную учетную запись - вернуть ее будет некому".to_owned(),
        ));
    }

    let changed = tou_db::users::set_active(&state.db, user.id(), user_id, body.is_active).await?;
    if !changed {
        return Err(ApiError::NotFound);
    }

    tracing::info!(actor = %user.id(), user_id = %user_id, is_active = body.is_active, "учетная запись переключена");
    Ok(StatusCode::NO_CONTENT)
}

/// Алфавит одноразового пароля: 32 знака, из которых выброшены неразличимые
/// на слух и в шрифте (`0`/`O`, `1`/`l`/`I`) - пароль диктуют голосом и
/// переписывают глазами. Длина алфавита - степень двойки, поэтому знак
/// выбирается остатком от байта без перекоса: у 256 значений байта на каждый
/// из 32 знаков приходится ровно восемь.
const OTP_ALPHABET: &[u8] = b"23456789abcdefghijkmnpqrstuvwxyz";

/// Одноразовый пароль: 20 знаков по 5 бит - сто бит энтропии, вчетверо
/// больше того, что перебирается словарем. Разбит дефисами по пятеркам:
/// его придется прочитать вслух или перепечатать.
fn one_time_password() -> String {
    const GROUP: usize = 5;
    let bytes: [u8; 20] = rand::random();

    bytes
        .iter()
        .enumerate()
        .fold(String::with_capacity(bytes.len() + 3), |mut acc, (i, b)| {
            if i > 0 && i % GROUP == 0 {
                acc.push('-');
            }
            acc.push(char::from(
                OTP_ALPHABET[usize::from(*b) % OTP_ALPHABET.len()],
            ));
            acc
        })
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuditChainDto {
    /// Момент последней сверки; `null` - сверок еще не было ни одной
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub checked_at: Option<OffsetDateTime>,
    /// Итог последней сверки; `null` - сверок еще не было
    pub intact: Option<bool>,
    /// Записей в журнале на момент последней сверки
    pub entries: Option<i64>,
    /// `audit.log.id` первой разошедшейся записи; заполнен только при разрыве
    pub broken_at: Option<i64>,
    /// Момент последней сверки, на которой цепочка сходилась
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub last_intact_at: Option<OffsetDateTime>,
}

/// Состояние hash-цепочки аудита (INV-A01, FR-1601).
///
/// Отвечает не «цела ли цепочка прямо сейчас», а «что показала последняя
/// сверка и когда она была»: пересчет журнала по запросу из браузера дал бы
/// произвольному нажатию кнопки полный проход по всему аудиту. Сверку ведет
/// фоновый воркер по расписанию, здесь - чтение его следа. Пустой ответ
/// (`checked_at = null`) значит, что сверок не было вовсе, - для дежурного
/// это такой же сигнал, как и разрыв.
#[utoipa::path(
    get,
    path = "/api/v1/admin/audit/chain",
    tag = "admin",
    responses(
        (status = 200, description = "Состояние цепочки аудита", body = AuditChainDto),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
    )
)]
pub async fn audit_chain(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<AuditChainDto>, ApiError> {
    user.require(Action::AuditRead)?;

    let chain = tou_db::audit::chain_state(&state.db).await?;
    Ok(Json(AuditChainDto {
        checked_at: chain.last.map(|check| check.checked_at),
        intact: chain.last.map(|check| check.intact),
        entries: chain.last.map(|check| check.entries),
        broken_at: chain.last.and_then(|check| check.broken_at),
        last_intact_at: chain.last_intact_at,
    }))
}

fn parse_grantable_role(raw: &str) -> Result<Role, ApiError> {
    let role: Role = raw
        .parse()
        .map_err(|_| ApiError::Validation(format!("неизвестная роль: {raw}")))?;
    if role == Role::Guest {
        return Err(ApiError::Validation(
            "роль guest не назначается - это аноним".into(),
        ));
    }
    Ok(role)
}

#[cfg(test)]
mod tests {
    use tou_domain::policy::is_allowed;

    use super::*;

    /// Маршрут состояния цепочки зарегистрирован в реестре `api_router`.
    ///
    /// Проверяется через контракт: `routes!` кладет хендлер и в axum-роутер,
    /// и в OpenAPI одновременно, поэтому путь в документе означает и
    /// работающий маршрут, и то, что кабинет админа сможет собрать запрос
    /// после кодогена (G5).
    #[test]
    fn audit_chain_route_is_registered_in_the_contract() {
        let json = crate::openapi().to_json().expect("сериализация контракта");
        assert!(
            json.contains("/api/v1/admin/audit/chain"),
            "маршрут состояния цепочки аудита не попал в контракт"
        );
        assert!(
            json.contains("AuditChainDto"),
            "схема ответа не попала в контракт - кодоген клиента ее не увидит"
        );
    }

    /// Состояние цепочки читает только admin (INV-POL-01, `Action::AuditRead`).
    ///
    /// Разрыв цепочки - сведение о состоянии доказательной базы, а не
    /// публичный факт: расширение права здесь должно быть решением, а не
    /// побочным следствием правки матрицы.
    #[test]
    fn audit_chain_is_readable_by_admin_only() {
        for role in Role::ALL {
            let expected = role == Role::Admin;
            assert_eq!(
                is_allowed(role, Action::AuditRead),
                expected,
                "право AuditRead у роли {}",
                role.as_str()
            );
        }
    }

    /// Одноразовый пароль обязан быть машинным и разным: длина, алфавит
    /// без неразличимых знаков и отсутствие повторов на подряд идущих
    /// выдачах - все, что можно проверить, не гадая о генераторе.
    #[test]
    fn one_time_password_is_long_random_and_readable() {
        let issued: Vec<String> = (0..64).map(|_| one_time_password()).collect();

        for password in &issued {
            assert_eq!(password.len(), 23, "20 знаков и 3 дефиса: {password}");
            assert!(
                password
                    .bytes()
                    .all(|b| b == b'-' || OTP_ALPHABET.contains(&b)),
                "знак вне алфавита: {password}"
            );
            assert!(
                !password.bytes().any(|b| b"0O1lI".contains(&b)),
                "неразличимые знаки в пароле, который диктуют: {password}"
            );
        }

        let unique: std::collections::BTreeSet<&String> = issued.iter().collect();
        assert_eq!(unique.len(), issued.len(), "пароли повторяются");
    }

    /// Сброс пароля и переключение записи - под тем же правом, что и список
    /// пользователей, и ни у кого больше (INV-POL-01).
    #[test]
    fn account_lifecycle_is_managed_by_admin_only() {
        for role in Role::ALL {
            assert_eq!(
                is_allowed(role, Action::UserManage),
                role == Role::Admin,
                "право UserManage у роли {}",
                role.as_str()
            );
        }
    }

    /// Маршруты жизненного цикла записи зарегистрированы в реестре
    /// `api_router`: путь в контракте означает и работающий маршрут,
    /// и то, что кабинет админа соберет запрос после кодогена (G5).
    #[test]
    fn account_lifecycle_routes_are_registered_in_the_contract() {
        let json = crate::openapi().to_json().expect("сериализация контракта");
        for path in [
            "/api/v1/admin/users/{user_id}/password-reset",
            "/api/v1/admin/users/{user_id}/active",
            "/api/v1/auth/password",
        ] {
            assert!(json.contains(path), "маршрут {path} не попал в контракт");
        }
    }

    /// Пустой журнал сверок отвечает честным `null`, а не «цела».
    ///
    /// Отсутствие сверок и целая цепочка - разные состояния: первое значит,
    /// что не проверял никто, и для дежурного это такой же сигнал, как разрыв.
    #[test]
    fn empty_chain_state_is_not_reported_as_intact() {
        let dto = AuditChainDto {
            checked_at: None,
            intact: None,
            entries: None,
            broken_at: None,
            last_intact_at: None,
        };
        let json = serde_json::to_value(&dto).expect("сериализация ответа");
        assert_eq!(json["intact"], serde_json::Value::Null);
        assert_eq!(json["checked_at"], serde_json::Value::Null);
    }
}
