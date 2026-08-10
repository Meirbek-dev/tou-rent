//! Аутентификация контура 1 (FR-1501): email+пароль (Argon2id),
//! сессии в Redis (tower-sessions), авто-подтверждение email со ссылкой в лог.

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

/// Публичное представление пользователя (без password_hash - NFR-07).
#[derive(Debug, Serialize, ToSchema)]
pub struct UserDto {
    pub id: Uuid,
    pub email: String,
    pub full_name: String,
    pub locale: String,
    /// Роли из `core.role_grants` (snake_case, см. enum `Role` домена)
    #[schema(value_type = Vec<String>, example = json!(["participant"]))]
    pub roles: Vec<Role>,
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
            roles,
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
    /// kk / ru / en
    #[garde(custom(known_locale))]
    #[serde(default = "default_locale")]
    pub locale: String,
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
/// Контур 1: email подтверждается автоматически, ссылка пишется в лог (FR-1501).
#[utoipa::path(
    post,
    path = "/api/v1/auth/register",
    tag = "auth",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "Участник зарегистрирован", body = UserDto),
        (status = 409, description = "Email занят", body = crate::error::Problem),
        (status = 422, description = "Данные не прошли проверку", body = crate::error::Problem),
        (status = 429, description = "Слишком много попыток", body = crate::error::Problem),
    )
)]
pub async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<UserDto>), ApiError> {
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

    let record = users::insert_participant(
        &state.db,
        &body.email,
        &password_hash,
        &body.full_name,
        &body.locale,
    )
    .await
    .map_err(|err| match err {
        InsertUserError::EmailTaken => ApiError::EmailTaken,
        InsertUserError::Db(db) => db.into(),
    })?;

    // Учетная запись заведена - вот это и есть исчерпаемый ресурс
    if let Some(address) = address.as_deref() {
        state
            .rate_limit
            .record("register:address", address, REGISTER_PER_ADDRESS)
            .await;
    }

    // FR-1501: доказательство подтверждения - ссылка в логе (контур 1)
    let token: [u8; 16] = rand::random();
    tracing::info!(
        user_id = %record.id,
        link = format!("/auth/confirm-email?token={}", hex(&token)),
        "email подтвержден автоматически (контур 1); ссылка для будущего канала"
    );

    Ok((
        StatusCode::CREATED,
        Json(UserDto::new(record, vec![Role::Participant])),
    ))
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

    // Проверка выполняется и при отсутствии пользователя - выравнивание времени ответа
    let (record, stored_hash) = match user {
        Some(record) if record.is_active => {
            let hash = record.password_hash.clone();
            (Some(record), hash)
        }
        _ => (None, None),
    };

    let password = body.password.clone();
    let verified = tokio::task::spawn_blocking(move || verify_password(&password, stored_hash))
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
    let (jar, _token) = csrf::issue(jar, state.secure_cookies);

    tracing::info!(user_id = %record.id, "login");
    Ok((jar, Json(UserDto::new(record, roles))))
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

/// Выход: сессия уничтожается в Redis.
#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    tag = "auth",
    responses((status = 204, description = "Сессия завершена"))
)]
pub async fn logout(session: Session) -> Result<StatusCode, ApiError> {
    session.flush().await.map_err(ApiError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

fn crypto_error(err: impl std::fmt::Display) -> ApiError {
    ApiError::internal(std::io::Error::other(err.to_string()))
}

fn hash_password(password: &str) -> Result<String, ApiError> {
    // password-hash 0.6: соль генерируется внутри (feature getrandom)
    let hash: PasswordHash = Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(crypto_error)?;
    Ok(hash.to_string())
}

fn verify_password(password: &str, stored_hash: Option<String>) -> bool {
    /// Фиктивный Argon2id-хеш для выравнивания времени при неизвестном email
    const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$C29tZXNhbHRzb21lc2FsdA$Zg8dLKiCC7EqhFsGvBnBFCoWTVaLksIQFXHy3nAxHQE";
    let hash_str = stored_hash.as_deref().unwrap_or(DUMMY_HASH);
    let Ok(parsed) = hash_str.parse::<PasswordHash>() else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
        && stored_hash.is_some()
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut acc, b| {
            use std::fmt::Write as _;
            let _ = write!(acc, "{b:02x}");
            acc
        })
}
