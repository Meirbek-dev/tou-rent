//! Аутентификация контура 1 (FR-1501): email+пароль (Argon2id),
//! сессии в Redis и обязательное подтверждение Email/SMS одноразовым кодом.

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher as _, PasswordVerifier as _};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum_extra::extract::CookieJar;
use garde::Validate as _;
use serde::{Deserialize, Serialize};
use tou_db::users::{self, InsertUserError, UserRecord};
use tou_domain::role::Role;
use tower_sessions::Session;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::csrf;
use crate::error::ApiError;
use crate::extract::{CurrentUser, SESSION_USER_KEY};
use crate::ratelimit::{
    LOGIN_PER_ACCOUNT, LOGIN_PER_ADDRESS, REGISTER_PER_ADDRESS, client_address,
};
use crate::request::Json;
use crate::state::AppState;
use crate::verification::VerificationChannel;

/// Публичное представление пользователя (без password_hash - NFR-07).
#[derive(Debug, Serialize, ToSchema)]
pub struct UserDto {
    pub id: Uuid,
    pub email: String,
    pub full_name: String,
    pub locale: String,
    pub applicant_kind: Option<String>,
    pub id_number: Option<String>,
    pub phone: Option<String>,
    /// Роли из `core.role_grants` (snake_case, см. enum `Role` домена)
    #[schema(value_type = Vec<String>, example = json!(["participant"]))]
    pub roles: Vec<Role>,
    /// Учетная запись действует. Деактивированная не входит и не работает по
    /// уже открытой сессии (W-07); в кабинете админа это состояние видно
    /// и переключается
    pub is_active: bool,
    /// Текущая сессия открыта внешним провайдером (FR-1502): выход должен
    /// завершить сессию и у него, иначе повторный вход пройдет молча
    pub external_session: bool,
}

impl UserDto {
    pub(crate) fn new(record: UserRecord, roles: Vec<Role>) -> Self {
        Self {
            id: record.id,
            email: record.email,
            full_name: record.full_name,
            locale: record.locale,
            applicant_kind: record.applicant_kind,
            id_number: record.id_number,
            phone: record.phone,
            roles,
            is_active: record.is_active,
            external_session: false,
        }
    }

    pub(crate) fn with_external_session(mut self, external: bool) -> Self {
        self.external_session = external;
        self
    }
}

#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct RegisterRequest {
    #[garde(email, length(max = 254))]
    pub email: String,
    /// Минимум 12 символов (TODO-ENGINEER: утвердить парольную политику)
    #[garde(length(chars, min = 12, max = 128))]
    pub password: String,
    #[garde(length(chars, min = 1, max = 200))]
    pub full_name: String,
    #[garde(skip)]
    pub applicant_kind: crate::dto::ApplicantKindDto,
    #[garde(custom(valid_id_number))]
    pub id_number: String,
    #[garde(custom(valid_phone))]
    pub phone: String,
    #[garde(skip)]
    pub verification_channel: VerificationChannel,
    /// kk / ru / en
    #[garde(custom(known_locale))]
    #[serde(default = "default_locale")]
    pub locale: String,
}

fn valid_id_number(value: &str, _ctx: &()) -> garde::Result {
    tou_domain::identity::validate_id_number(value)
        .map_err(|error| garde::Error::new(error.to_string()))
}

fn valid_phone(value: &str, _ctx: &()) -> garde::Result {
    tou_domain::identity::validate_phone(value)
        .map_err(|error| garde::Error::new(error.to_string()))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RegistrationPendingDto {
    pub email: String,
    pub verification_channel: VerificationChannel,
}

#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct ConfirmRegistrationRequest {
    #[garde(email, length(max = 254))]
    pub email: String,
    #[garde(skip)]
    pub verification_channel: VerificationChannel,
    #[garde(custom(valid_verification_code))]
    pub code: String,
}

fn valid_verification_code(value: &str, _ctx: &()) -> garde::Result {
    if value.len() == 8 && value.bytes().all(|byte| byte.is_ascii_digit()) {
        Ok(())
    } else {
        Err(garde::Error::new("код должен состоять из 8 цифр"))
    }
}

fn default_locale() -> String {
    "ru".to_owned()
}

fn known_locale(value: &str, _ctx: &()) -> garde::Result {
    if matches!(value, "kk" | "ru" | "en") {
        Ok(())
    } else {
        Err(garde::Error::new("locale должен быть kk, ru или en"))
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Регистрация участника (FR-1401 мастер - контур web; здесь API).
/// Учетная запись создается неподтвержденной. Одноразовый код уходит через
/// настроенный SMTP или SMS-шлюз; без подтверждения вход закрыт.
#[utoipa::path(
    post,
    path = "/api/v1/auth/register",
    tag = "auth",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "Код подтверждения отправлен", body = RegistrationPendingDto),
        (status = 409, description = "Email или ИИН/БИН занят", body = crate::error::Problem),
        (status = 422, description = "Данные не прошли проверку", body = crate::error::Problem),
        (status = 429, description = "Слишком много попыток", body = crate::error::Problem),
    )
)]
pub async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<RegistrationPendingDto>), ApiError> {
    // Массовая регистрация с одного адреса (FR-1504). Проверка - до разбора
    // тела, чтобы стоимость отказа не нес сервер; счет - после успеха, иначе
    // ограничение било бы по опечаткам живого человека, а не по боту
    let address = client_address(&headers);
    if let Some(address) = address.as_deref() {
        state
            .rate_limit
            .check("register:address", address, REGISTER_PER_ADDRESS)
            .await?;
    }

    body.validate()
        .map_err(|report| ApiError::Validation(report.to_string()))?;

    let password = body.password.clone();
    let password_hash = tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(ApiError::internal)??;
    let code = format!("{:08}", rand::random::<u32>() % 100_000_000);
    let code_for_hash = code.clone();
    let code_hash = tokio::task::spawn_blocking(move || hash_password(&code_for_hash))
        .await
        .map_err(ApiError::internal)??;

    // Сначала убеждаемся, что провайдер действительно принял сообщение.
    // Иначе при недоступном SMTP/SMS оставалась занятая, но неподтверждаемая
    // учетная запись, и повтор регистрации завершался конфликтом.
    let recipient = match body.verification_channel {
        VerificationChannel::Email => body.email.as_str(),
        VerificationChannel::Sms => body.phone.as_str(),
    };
    state
        .verification_delivery
        .send(body.verification_channel, recipient, &code)
        .await
        .map_err(|error| ApiError::ProviderUnavailable(error.to_string()))?;

    let record = users::insert_participant(
        &state.db,
        users::NewParticipant {
            email: &body.email,
            password_hash: &password_hash,
            full_name: &body.full_name,
            locale: &body.locale,
            applicant_kind: body.applicant_kind.as_db(),
            id_number: &body.id_number,
            phone: &body.phone,
            verification_channel: body.verification_channel.as_db(),
            code_hash: &code_hash,
        },
    )
    .await
    .map_err(|err| match err {
        InsertUserError::EmailTaken => ApiError::EmailTaken,
        InsertUserError::IdNumberTaken => ApiError::IdNumberTaken,
        InsertUserError::Db(db) => db.into(),
    })?;

    // Учетная запись заведена - вот это и есть исчерпаемый ресурс
    if let Some(address) = address.as_deref() {
        state
            .rate_limit
            .record("register:address", address, REGISTER_PER_ADDRESS)
            .await;
    }

    tracing::info!(user_id = %record.id, channel = ?body.verification_channel, "код подтверждения отправлен");

    Ok((
        StatusCode::CREATED,
        Json(RegistrationPendingDto {
            email: record.email,
            verification_channel: body.verification_channel,
        }),
    ))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/confirm-registration",
    tag = "auth",
    request_body = ConfirmRegistrationRequest,
    responses(
        (status = 204, description = "Канал подтвержден, вход разрешен"),
        (status = 422, description = "Код неверен или истек", body = crate::error::Problem),
    )
)]
pub async fn confirm_registration(
    State(state): State<AppState>,
    Json(body): Json<ConfirmRegistrationRequest>,
) -> Result<StatusCode, ApiError> {
    body.validate()
        .map_err(|report| ApiError::Validation(report.to_string()))?;
    let verification =
        users::active_verification(&state.db, &body.email, body.verification_channel.as_db())
            .await?
            .ok_or(ApiError::VerificationFailed)?;

    let hash = verification.code_hash.clone();
    let code = body.code.clone();
    let valid = tokio::task::spawn_blocking(move || verify_password(&code, Some(&hash)))
        .await
        .map_err(ApiError::internal)?;
    if !valid {
        users::record_verification_failure(&state.db, verification.id).await?;
        return Err(ApiError::VerificationFailed);
    }

    let confirmed = users::confirm_verification(
        &state.db,
        verification.id,
        verification.user_id,
        body.verification_channel.as_db(),
    )
    .await?;
    if !confirmed {
        return Err(ApiError::VerificationFailed);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Вход по email и паролю. Успех: сессионная cookie (httpOnly, SameSite=Lax)
/// + CSRF-токен double-submit cookie (арх. § 5).
#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Вход выполнен", body = UserDto),
        (status = 401, description = "Неверные учетные данные", body = crate::error::Problem),
        (status = 429, description = "Слишком много попыток", body = crate::error::Problem),
    )
)]
pub async fn login(
    State(state): State<AppState>,
    session: Session,
    jar: CookieJar,
    headers: HeaderMap,
    Json(body): Json<LoginRequest>,
) -> Result<(CookieJar, Json<UserDto>), ApiError> {
    // Перебор пароля (NFR-07). Проверка - до Argon2id, иначе лимит обходится
    // самой дорогой операцией запроса; счет неудач - ниже, после проверки
    let address = client_address(&headers);
    state
        .rate_limit
        .check("login:account", &body.email, LOGIN_PER_ACCOUNT)
        .await?;
    if let Some(address) = address.as_deref() {
        state
            .rate_limit
            .check("login:address", address, LOGIN_PER_ADDRESS)
            .await?;
    }

    let user = users::find_by_email(&state.db, &body.email).await?;
    let (record, stored_hash) = admitted_credentials(user);

    let password = body.password.clone();
    let verified =
        tokio::task::spawn_blocking(move || verify_password(&password, stored_hash.as_deref()))
            .await
            .map_err(ApiError::internal)?;

    let record = match (record, verified) {
        (Some(record), true) => record,
        // Считается и логируется только неудача. Без следа в логе перебор
        // не виден ни там, ни в метриках; ни email, ни адрес в запись
        // не идут (NFR-07, NFR-16) - счетчик знает о них по отпечатку
        _ => {
            state
                .rate_limit
                .record("login:account", &body.email, LOGIN_PER_ACCOUNT)
                .await;
            if let Some(address) = address.as_deref() {
                state
                    .rate_limit
                    .record("login:address", address, LOGIN_PER_ADDRESS)
                    .await;
            }
            tracing::warn!("неудачная попытка входа");
            return Err(ApiError::InvalidCredentials);
        }
    };

    // Верный пароль снимает подозрение: иначе человек, у которого сессия
    // истекла посреди дня, доедал бы остаток счетчика своими же входами
    state.rate_limit.forget("login:account", &body.email).await;

    // Защита от фиксации сессии + запись пользователя
    session.cycle_id().await.map_err(ApiError::internal)?;
    session
        .insert(SESSION_USER_KEY, record.id)
        .await
        .map_err(ApiError::internal)?;

    let roles = users::roles_of(&state.db, record.id).await?;
    // Токен CSRF перевыпускается вместе с идентификатором сессии: привилегии
    // сменились, и прежнее значение подходить больше не должно
    let jar = csrf::issue(&session, jar, state.secure_cookies).await?;

    tracing::info!(user_id = %record.id, "login");
    Ok((jar, Json(UserDto::new(record, roles))))
}

/// Кого вообще пускают к проверке пароля.
///
/// Деактивированная учетная запись (W-07) отсекается здесь, а не отдельной
/// веткой после проверки: иначе ответ на деактивированного приходил бы
/// быстрее, чем на живого, - Argon2id пропущен, - и «отключен» отличалось бы
/// от «неверный пароль» по секундомеру. Хеша нет ни у неизвестного email,
/// ни у отключенного, ни у учетной записи внешнего провайдера, и все три
/// случая доходят до [`verify_password`] одинаково.
fn admitted_credentials(user: Option<UserRecord>) -> (Option<UserRecord>, Option<String>) {
    match user {
        Some(record)
            if record.is_active
                && (record.email_confirmed_at.is_some() || record.phone_confirmed_at.is_some()) =>
        {
            let hash = record.password_hash.clone();
            (Some(record), hash)
        }
        _ => (None, None),
    }
}

#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct ChangePasswordRequest {
    /// Текущий пароль - обязателен: без него угнанная сессия становится
    /// угнанной учетной записью
    #[garde(skip)]
    pub current_password: String,
    /// Те же границы, что и при регистрации
    #[garde(length(chars, min = 12, max = 128))]
    pub new_password: String,
}

/// Смена собственного пароля из сессии (W-07).
///
/// До нее вернуть себе доступ было нечем: маршрута восстановления нет, а
/// сменить скомпрометированный пароль изнутри было негде.
#[utoipa::path(
    post,
    path = "/api/v1/auth/password",
    tag = "auth",
    request_body = ChangePasswordRequest,
    responses(
        (status = 204, description = "Пароль изменен"),
        (status = 401, description = "Текущий пароль неверен", body = crate::error::Problem),
        (status = 422, description = "Данные не прошли проверку", body = crate::error::Problem),
        (status = 429, description = "Слишком много попыток", body = crate::error::Problem),
    )
)]
pub async fn change_password(
    user: CurrentUser,
    State(state): State<AppState>,
    session: Session,
    jar: CookieJar,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<(CookieJar, StatusCode), ApiError> {
    // Тот же счетчик неудач, что и у входа, но своя корзина и своя тема:
    // по email считать нельзя - обладатель сессии запирал бы владельцу
    // учетной записи вход, подбирая старый пароль у себя же
    let subject = user.id().to_string();
    state
        .rate_limit
        .check("password:account", &subject, LOGIN_PER_ACCOUNT)
        .await?;

    body.validate()
        .map_err(|report| ApiError::Validation(report.to_string()))?;

    let stored = user.record.password_hash.clone();
    let current = body.current_password.clone();
    let verdict =
        tokio::task::spawn_blocking(move || authorize_password_change(stored.as_deref(), &current))
            .await
            .map_err(ApiError::internal)?;

    if let Err(error) = verdict {
        if matches!(error, ApiError::InvalidCredentials) {
            state
                .rate_limit
                .record("password:account", &subject, LOGIN_PER_ACCOUNT)
                .await;
            tracing::warn!(user_id = %user.id(), "неверный текущий пароль при смене");
        }
        return Err(error);
    }
    state.rate_limit.forget("password:account", &subject).await;

    let password = body.new_password.clone();
    let password_hash = tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(ApiError::internal)??;

    // Актор - сам пользователь: в аудите смена своего пароля обязана
    // отличаться от сброса админом (W-07)
    let changed = users::set_password(&state.db, user.id(), user.id(), &password_hash).await?;
    if !changed {
        return Err(ApiError::NotFound);
    }

    // Прочие сессии этого пользователя остаются жить, и это не забывчивость.
    // tower-sessions хранит в Redis отображение «идентификатор сессии → данные»
    // и обратного - «пользователь → его сессии» - не ведет: перебрать чужие
    // сессии отсюда нечем, а сканировать хранилище по шаблону значит трогать
    // и сессии всех остальных. Честный способ ровно один - отметка версии
    // пароля в самой учетной записи, которую сверяет экстрактор `CurrentUser`
    // на каждом запросе; это правка `extract.rs` и отдельная задача.
    // TODO-ENGINEER: завершение прочих сессий при смене пароля (W-07).
    //
    // Своя сессия все же обновляется: идентификатор и CSRF-токен
    // перевыпускаются, как при входе, - украденная пара «cookie + токен»,
    // снятая до смены пароля, перестает подходить хотя бы у этой вкладки.
    session.cycle_id().await.map_err(ApiError::internal)?;
    session
        .insert(SESSION_USER_KEY, user.id())
        .await
        .map_err(ApiError::internal)?;
    let jar = csrf::issue(&session, jar, state.secure_cookies).await?;

    tracing::info!(user_id = %user.id(), "password changed");
    Ok((jar, StatusCode::NO_CONTENT))
}

/// Рубеж смены пароля: старый обязан совпасть.
///
/// Вынесен из хендлера, чтобы обе ветки проверялись без БД и сессии: именно
/// здесь стоит то, из-за чего перехваченная сессия не превращается в
/// перехваченную навсегда учетную запись.
fn authorize_password_change(stored_hash: Option<&str>, current: &str) -> Result<(), ApiError> {
    let Some(stored) = stored_hash else {
        // Пароля нет вовсе - учетная запись заведена внешним провайдером
        // (FR-1502). Менять здесь нечего, и заводить ей локальный пароль
        // «по дороге» нельзя: это второй путь входа мимо политик провайдера
        return Err(ApiError::Validation(
            "у учетной записи нет локального пароля - вход выполняется через внешнего провайдера"
                .to_owned(),
        ));
    };
    if verify_password(current, Some(stored)) {
        Ok(())
    } else {
        Err(ApiError::InvalidCredentials)
    }
}

/// Текущий пользователь и его роли (кабинеты по ролям, ТЗ § 8).
#[utoipa::path(
    get,
    path = "/api/v1/auth/me",
    tag = "auth",
    responses(
        (status = 200, description = "Текущий пользователь", body = UserDto),
        (status = 401, description = "Не аутентифицирован", body = crate::error::Problem),
    )
)]
pub async fn me(user: CurrentUser, session: Session) -> Result<Json<UserDto>, ApiError> {
    let external = session
        .get::<String>(crate::oidc::SESSION_ID_TOKEN_KEY)
        .await
        .map_err(ApiError::internal)?
        .is_some();

    Ok(Json(
        UserDto::new(user.record, user.roles).with_external_session(external),
    ))
}

/// Выход: сессия уничтожается в Redis, CSRF-токен гасится в браузере.
#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    tag = "auth",
    responses((status = 204, description = "Сессия завершена"))
)]
pub async fn logout(
    State(state): State<AppState>,
    session: Session,
    jar: CookieJar,
) -> Result<(CookieJar, StatusCode), ApiError> {
    session.flush().await.map_err(ApiError::internal)?;
    // Без этого cookie переживает сессию: страница после выхода продолжала бы
    // отдавать токен, которому на сервере уже ничего не соответствует
    Ok((
        csrf::revoke(jar, state.secure_cookies),
        StatusCode::NO_CONTENT,
    ))
}

fn crypto_error(err: impl std::fmt::Display) -> ApiError {
    ApiError::internal(std::io::Error::other(err.to_string()))
}

pub(crate) fn hash_password(password: &str) -> Result<String, ApiError> {
    // password-hash 0.6: соль генерируется внутри (feature getrandom)
    let hash: PasswordHash = Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(crypto_error)?;
    Ok(hash.to_string())
}

fn verify_password(password: &str, stored_hash: Option<&str>) -> bool {
    /// Фиктивный Argon2id-хеш для выравнивания времени при неизвестном email
    const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$C29tZXNhbHRzb21lc2FsdA$Zg8dLKiCC7EqhFsGvBnBFCoWTVaLksIQFXHy3nAxHQE";
    let hash_str = stored_hash.unwrap_or(DUMMY_HASH);
    let Ok(parsed) = hash_str.parse::<PasswordHash>() else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
        && stored_hash.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(is_active: bool, password_hash: Option<&str>) -> UserRecord {
        UserRecord {
            id: Uuid::now_v7(),
            email: "someone@tou.test".to_owned(),
            password_hash: password_hash.map(str::to_owned),
            full_name: "Проверочная запись".to_owned(),
            locale: "ru".to_owned(),
            applicant_kind: None,
            id_number: None,
            phone: None,
            email_confirmed_at: Some(time::OffsetDateTime::UNIX_EPOCH),
            phone_confirmed_at: None,
            is_active,
        }
    }

    /// Рубеж W-07: смена пароля проходит со старым паролем и не проходит
    /// с чужим. Без этой проверки перехваченная сессия давала бы не доступ
    /// на восемь часов, а учетную запись навсегда.
    #[test]
    fn password_change_requires_the_current_password() {
        let stored = hash_password("прежний-пароль-1").expect("хеш прежнего пароля");

        assert!(
            authorize_password_change(Some(&stored), "прежний-пароль-1").is_ok(),
            "со своим текущим паролем смена обязана проходить"
        );
        assert!(
            matches!(
                authorize_password_change(Some(&stored), "не-тот-пароль-1"),
                Err(ApiError::InvalidCredentials)
            ),
            "с неверным текущим паролем смена обязана отбиваться"
        );
    }

    /// Учетной записи внешнего провайдера локальный пароль не заводится
    /// «по дороге»: это второй путь входа мимо политик самого провайдера.
    #[test]
    fn account_without_local_password_is_not_given_one() {
        assert!(
            matches!(
                authorize_password_change(None, "что-угодно-12"),
                Err(ApiError::Validation(_))
            ),
            "без локального пароля менять нечего"
        );
    }

    /// Деактивированная учетная запись не доходит до своего хеша: вход
    /// сверяет введенное с фиктивным хешем и отвечает так же и за то же
    /// время, что и на неизвестный email.
    #[test]
    fn deactivated_account_is_denied_before_its_hash() {
        let password = "рабочий-пароль-1";
        let stored = hash_password(password).expect("хеш");

        let (found, hash) = admitted_credentials(Some(record(true, Some(&stored))));
        assert!(found.is_some() && hash.is_some(), "живая запись доходит");
        assert!(verify_password(password, hash.as_deref()));

        let (denied, hash) = admitted_credentials(Some(record(false, Some(&stored))));
        assert!(
            denied.is_none() && hash.is_none(),
            "деактивированная запись до проверки пароля не доходит"
        );
        assert!(
            !verify_password(password, hash.as_deref()),
            "верный пароль деактивированной записи вход не открывает"
        );
    }

    /// Отсутствие пользователя и отсутствие пароля - одинаково «неверно»,
    /// и оба стоят одного прохода Argon2id (выравнивание времени, NFR-07).
    #[test]
    fn unknown_account_is_indistinguishable_from_a_wrong_password() {
        let (record, hash) = admitted_credentials(None);
        assert!(record.is_none() && hash.is_none());
        assert!(!verify_password("любой-пароль-12", hash.as_deref()));
    }
}
