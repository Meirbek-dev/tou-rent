//! Вход через внешнего провайдера идентичности (FR-1502, ADR-0003).
//!
//! Zitadel (федерация с AD университета настраивается на его стороне), поток -
//! authorization code + PKCE. Обмен кода на токены и разбор `id_token` идут
//! в api: браузер токенов не видит, наружу остается та же серверная сессия
//! в Redis, что и у локального входа (FR-1501). Роли сотрудников приходят
//! claim'ом и отражаются в `core.role_grants` источником `oidc` (FR-1503).
//!
//! Без переменных `OIDC_*` модуль выключен целиком: система работает как
//! в контуре 1 - это же и якорь отката при недоступном провайдере.

use std::sync::Arc;

use axum::extract::State;
use axum::response::Redirect;
use axum_extra::extract::CookieJar;
use openidconnect::core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata};
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet, EndpointNotSet,
    EndpointSet, IssuerUrl, Nonce, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
};
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;
use tou_db::identities::{self, ExternalIdentity};
use tou_domain::role::Role;
use tower_sessions::Session;
use utoipa::ToSchema;

use crate::csrf;
use crate::error::ApiError;
use crate::extract::SESSION_USER_KEY;
use crate::request::{Json, Query};
use crate::state::AppState;

/// Клиент после `from_provider_metadata`: authorization endpoint известен из
/// discovery, token/userinfo - «возможно заданы» (проверяются при вызове).
type DiscoveredClient = CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

/// Ключ незавершенного потока входа в сессии (state, nonce, PKCE-verifier).
const SESSION_FLOW_KEY: &str = "oidc_flow";
/// `id_token` последнего входа - подсказка провайдеру при выходе
/// (RP-initiated logout) и признак того, что сессию открыл провайдер.
pub(crate) const SESSION_ID_TOKEN_KEY: &str = "oidc_id_token";

/// Claim Zitadel с ролями проекта: `{"organizer": {"<orgId>": "<domain>"}}`.
/// Имя меняется переменной окружения - от провайдера зависит только оно.
const DEFAULT_ROLES_CLAIM: &str = "urn:zitadel:iam:org:project:roles";

/// Конфигурация провайдера (NFR-09: значения - из окружения/SOPS, не из кода).
#[derive(Debug, Clone)]
pub struct OidcConfig {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    /// Абсолютный URL `/api/v1/auth/oidc/callback` этого стенда
    pub redirect_url: String,
    /// Куда вернуть браузер после успешного входа
    pub post_login_path: String,
    /// Куда вернуть браузер при отказе провайдера или порванном потоке
    pub login_path: String,
    /// Куда вернуть браузер после выхода (абсолютный URL - требование провайдера)
    pub post_logout_url: String,
    /// Подпись кнопки входа на странице логина
    pub label: String,
    pub roles_claim: String,
    pub scopes: Vec<String>,
    /// Дополнительные значения `aud`, которым мы доверяем. Zitadel кладет в
    /// аудиторию `id_token` не только client_id, но и id проекта; список
    /// задается явно - «доверять всему подряд» здесь недопустимо.
    pub trusted_audiences: Vec<String>,
}

impl OidcConfig {
    /// `None` - провайдер не настроен: маршруты OIDC отвечают 404,
    /// на странице входа кнопки нет.
    pub fn from_env() -> Option<Self> {
        let issuer = non_empty("OIDC_ISSUER_URL")?;
        let client_id = non_empty("OIDC_CLIENT_ID")?;
        let client_secret = non_empty("OIDC_CLIENT_SECRET")?;
        let redirect_url = non_empty("OIDC_REDIRECT_URL")?;

        // openid обязателен спецификацией; profile/email нужны для core.users;
        // Zitadel отдает роли в id_token по scope `...:aud` конкретного проекта
        let scopes = non_empty("OIDC_SCOPES")
            .unwrap_or_else(|| "openid profile email".to_owned())
            .split_whitespace()
            .map(str::to_owned)
            .collect();

        Some(Self {
            issuer,
            client_id,
            client_secret,
            redirect_url,
            post_login_path: non_empty("OIDC_POST_LOGIN_PATH").unwrap_or_else(|| "/app".to_owned()),
            login_path: non_empty("OIDC_LOGIN_PATH").unwrap_or_else(|| "/auth/login".to_owned()),
            post_logout_url: non_empty("OIDC_POST_LOGOUT_URL").unwrap_or_else(|| "/".to_owned()),
            label: non_empty("OIDC_LABEL").unwrap_or_else(|| "Zitadel".to_owned()),
            roles_claim: non_empty("OIDC_ROLES_CLAIM")
                .unwrap_or_else(|| DEFAULT_ROLES_CLAIM.to_owned()),
            scopes,
            trusted_audiences: non_empty("OIDC_TRUSTED_AUDIENCES")
                .unwrap_or_default()
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
        })
    }
}

fn non_empty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

/// Провайдер: конфигурация + discovery, выполняемое лениво и один раз.
///
/// Ленивое discovery намеренно: недоступный в момент старта Zitadel не должен
/// мешать api подняться (локальный вход и публичный портал от него не зависят),
/// а первая же удачная попытка кешируется.
pub struct OidcProvider {
    config: OidcConfig,
    http: openidconnect::reqwest::Client,
    discovered: OnceCell<Discovered>,
}

struct Discovered {
    client: DiscoveredClient,
    /// Из discovery-документа; отсутствует - выход только локальный
    end_session_endpoint: Option<String>,
}

impl OidcProvider {
    /// `None`, если переменные не заданы (контур 1 без изменений).
    pub fn from_env() -> Option<Arc<Self>> {
        OidcConfig::from_env().map(|config| Arc::new(Self::new(config)))
    }

    pub fn new(config: OidcConfig) -> Self {
        // Редиректы запрещены: клиент ходит по URL из discovery, следование
        // за перенаправлениями - классический вектор SSRF
        let http = openidconnect::reqwest::ClientBuilder::new()
            .redirect(openidconnect::reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default();

        Self {
            config,
            http,
            discovered: OnceCell::new(),
        }
    }

    pub fn config(&self) -> &OidcConfig {
        &self.config
    }

    async fn discovered(&self) -> Result<&Discovered, ApiError> {
        self.discovered
            .get_or_try_init(|| async {
                let issuer =
                    IssuerUrl::new(self.config.issuer.clone()).map_err(provider_unavailable)?;
                let metadata = CoreProviderMetadata::discover_async(issuer, &self.http)
                    .await
                    .map_err(provider_unavailable)?;

                let client = CoreClient::from_provider_metadata(
                    metadata,
                    ClientId::new(self.config.client_id.clone()),
                    Some(ClientSecret::new(self.config.client_secret.clone())),
                )
                .set_redirect_uri(
                    RedirectUrl::new(self.config.redirect_url.clone())
                        .map_err(provider_unavailable)?,
                );

                Ok(Discovered {
                    client,
                    end_session_endpoint: self.end_session_endpoint().await,
                })
            })
            .await
    }

    /// `end_session_endpoint` спецификации RP-initiated logout не входит в
    /// ядро OIDC-метаданных, поэтому читается из того же документа напрямую.
    async fn end_session_endpoint(&self) -> Option<String> {
        #[derive(Deserialize)]
        struct Doc {
            end_session_endpoint: Option<String>,
        }

        let url = format!(
            "{}/.well-known/openid-configuration",
            self.config.issuer.trim_end_matches('/')
        );
        match self.http.get(url).send().await {
            Ok(response) => response
                .text()
                .await
                .ok()
                .and_then(|body| serde_json::from_str::<Doc>(&body).ok())
                .and_then(|doc| doc.end_session_endpoint),
            Err(error) => {
                tracing::warn!(%error, "discovery: end_session_endpoint недоступен");
                None
            }
        }
    }
}

/// Провайдер не отвечает или настроен неверно - это не 500 системы:
/// вход через него временно невозможен, локальный вход работает.
fn provider_unavailable(error: impl std::fmt::Display) -> ApiError {
    // Подробности провайдера - в телеметрию: наружу они ничего не решают (NFR-07)
    tracing::error!(%error, "провайдер идентичности недоступен");
    ApiError::ProviderUnavailable("вход через провайдера временно недоступен".to_owned())
}

/// Незавершенный поток входа. Хранится в серверной сессии (Redis), а не в
/// cookie: `state` и PKCE-verifier не должны быть видны браузеру.
#[derive(Debug, Serialize, Deserialize)]
struct OidcFlow {
    state: String,
    nonce: String,
    verifier: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthProvidersDto {
    /// Внешний провайдер, если настроен (FR-1502)
    pub oidc: Option<OidcProviderDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OidcProviderDto {
    pub label: String,
    /// Ссылка начала входа - обычная навигация, работает без JS (NFR-04)
    pub login_url: String,
}

/// Доступные способы входа: страница логина рисует кнопку провайдера,
/// только если он настроен на этом стенде.
#[utoipa::path(
    get,
    path = "/api/v1/auth/providers",
    tag = "auth",
    responses((status = 200, description = "Способы входа", body = AuthProvidersDto))
)]
pub async fn auth_providers(State(state): State<AppState>) -> Json<AuthProvidersDto> {
    Json(AuthProvidersDto {
        oidc: state.oidc.as_ref().map(|provider| OidcProviderDto {
            label: provider.config.label.clone(),
            login_url: "/api/v1/auth/oidc/login".to_owned(),
        }),
    })
}

/// Начало входа: PKCE + state + nonce кладутся в сессию, браузер уходит
/// на страницу провайдера.
#[utoipa::path(
    get,
    path = "/api/v1/auth/oidc/login",
    tag = "auth",
    responses(
        (status = 303, description = "Переход на страницу провайдера"),
        (status = 404, description = "Провайдер не настроен", body = crate::error::Problem),
        (status = 503, description = "Провайдер недоступен", body = crate::error::Problem),
    )
)]
pub async fn oidc_login(
    State(state): State<AppState>,
    session: Session,
) -> Result<Redirect, ApiError> {
    let provider = state.oidc.as_ref().ok_or(ApiError::NotFound)?;
    let discovered = provider.discovered().await?;

    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let mut request = discovered.client.authorize_url(
        CoreAuthenticationFlow::AuthorizationCode,
        CsrfToken::new_random,
        Nonce::new_random,
    );
    for scope in &provider.config.scopes {
        if scope != "openid" {
            request = request.add_scope(Scope::new(scope.clone()));
        }
    }
    let (auth_url, csrf_state, nonce) = request.set_pkce_challenge(challenge).url();

    session
        .insert(
            SESSION_FLOW_KEY,
            OidcFlow {
                state: csrf_state.secret().clone(),
                nonce: nonce.secret().clone(),
                verifier: verifier.secret().clone(),
            },
        )
        .await
        .map_err(ApiError::internal)?;

    Ok(Redirect::to(auth_url.as_str()))
}

#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    pub code: Option<String>,
    pub state: Option<String>,
    /// Отказ провайдера (`access_denied` и т.д.)
    pub error: Option<String>,
}

/// Возврат от провайдера: сверка `state`, обмен кода на токены, проверка
/// `id_token` (подпись, аудитория, nonce), связывание с `core.users` и вход.
///
/// Ответ - редирект, а не JSON: сюда приходит навигация браузера.
#[utoipa::path(
    get,
    path = "/api/v1/auth/oidc/callback",
    tag = "auth",
    params(("code" = Option<String>, Query,), ("state" = Option<String>, Query,), ("error" = Option<String>, Query,)),
    responses(
        (status = 303, description = "Вход выполнен либо возврат на страницу входа"),
        (status = 404, description = "Провайдер не настроен", body = crate::error::Problem),
    )
)]
pub async fn oidc_callback(
    State(state): State<AppState>,
    session: Session,
    jar: CookieJar,
    Query(params): Query<CallbackParams>,
) -> Result<(CookieJar, Redirect), ApiError> {
    let provider = state.oidc.as_ref().ok_or(ApiError::NotFound)?;
    let config = &provider.config;

    // Поток одноразовый: заново начатый вход не должен принимать старый code
    let flow: Option<OidcFlow> = session
        .get(SESSION_FLOW_KEY)
        .await
        .map_err(ApiError::internal)?;
    session
        .remove::<OidcFlow>(SESSION_FLOW_KEY)
        .await
        .map_err(ApiError::internal)?;

    if let Some(error) = params.error {
        tracing::info!(%error, "провайдер отклонил вход");
        return Ok((
            jar,
            Redirect::to(&failure_url(&config.login_path, "denied")),
        ));
    }

    let (Some(flow), Some(code), Some(returned_state)) = (flow, params.code, params.state) else {
        return Ok((jar, Redirect::to(&failure_url(&config.login_path, "flow"))));
    };
    // Сверка state - защита от подмены кода (CSRF на callback)
    if flow.state != returned_state {
        tracing::warn!("state потока OIDC не совпал");
        return Ok((jar, Redirect::to(&failure_url(&config.login_path, "state"))));
    }

    let discovered = provider.discovered().await?;
    let tokens = discovered
        .client
        .exchange_code(AuthorizationCode::new(code))
        .map_err(provider_unavailable)?
        .set_pkce_verifier(PkceCodeVerifier::new(flow.verifier))
        .request_async(&provider.http)
        .await
        .map_err(provider_unavailable)?;

    let id_token = tokens
        .extra_fields()
        .id_token()
        .ok_or_else(|| provider_unavailable("ответ провайдера без id_token"))?;
    // Аудитория помимо client_id - только из явного списка конфигурации
    let trusted = config.trusted_audiences.clone();
    let verifier = discovered
        .client
        .id_token_verifier()
        .set_other_audience_verifier_fn(move |audience| {
            trusted.iter().any(|allowed| allowed == audience.as_str())
        });
    let claims = id_token
        .claims(&verifier, &Nonce::new(flow.nonce))
        .map_err(provider_unavailable)?;

    let email = claims
        .email()
        .map(|email| email.as_str().to_owned())
        .ok_or_else(|| {
            provider_unavailable("провайдер не вернул email - учетную запись не связать")
        })?;
    let provider_login = claims
        .preferred_username()
        .map(|name| name.as_str().to_owned());
    let full_name = claims
        .name()
        .and_then(|name| name.get(None))
        .map(|name| name.as_str().to_owned())
        .or_else(|| provider_login.clone())
        .unwrap_or_else(|| email.clone());

    let identity = ExternalIdentity {
        issuer: claims.issuer().as_str().to_owned(),
        subject: claims.subject().as_str().to_owned(),
        email,
        full_name,
        locale: locale_of(claims.locale().map(|l| l.as_str())),
        provider_login,
        roles: roles_from_id_token(&id_token.to_string(), &config.roles_claim),
    };

    let (user, outcome) = identities::login_external(&state.db, &identity)
        .await
        .map_err(|error| match error {
            // Запись отключена админом - вход запрещен любым способом
            sqlx::Error::RowNotFound => ApiError::InvalidCredentials,
            other => other.into(),
        })?;

    // Защита от фиксации сессии - как и в локальном входе
    session.cycle_id().await.map_err(ApiError::internal)?;
    session
        .insert(SESSION_USER_KEY, user.id)
        .await
        .map_err(ApiError::internal)?;
    session
        .insert(SESSION_ID_TOKEN_KEY, id_token.to_string())
        .await
        .map_err(ApiError::internal)?;

    // Тот же порядок, что и у локального входа: без токена в сессии
    // вошедший через провайдера не выполнил бы ни одной мутации
    let jar = csrf::issue(&session, jar, state.secure_cookies).await?;
    tracing::info!(user_id = %user.id, ?outcome, "вход через провайдера идентичности");

    Ok((jar, Redirect::to(&config.post_login_path)))
}

/// Выход с завершением сессии у провайдера (RP-initiated logout): без него
/// «выйти» на общем компьютере не выходит - следующий вход пройдет молча.
#[utoipa::path(
    get,
    path = "/api/v1/auth/oidc/logout",
    tag = "auth",
    responses(
        (status = 303, description = "Сессия завершена, переход к провайдеру"),
        (status = 404, description = "Провайдер не настроен", body = crate::error::Problem),
    )
)]
pub async fn oidc_logout(
    State(state): State<AppState>,
    session: Session,
    jar: CookieJar,
) -> Result<(CookieJar, Redirect), ApiError> {
    let provider = state.oidc.as_ref().ok_or(ApiError::NotFound)?;
    let id_token: Option<String> = session
        .get(SESSION_ID_TOKEN_KEY)
        .await
        .map_err(ApiError::internal)?;
    session.flush().await.map_err(ApiError::internal)?;
    // Выход есть выход: cookie токена гасится и здесь, иначе она пережила бы
    // уничтоженную сессию ровно так же, как при локальном выходе
    let jar = csrf::revoke(jar, state.secure_cookies);

    let config = &provider.config;
    let target = match provider.discovered().await {
        Ok(discovered) => discovered
            .end_session_endpoint
            .as_ref()
            .map(|endpoint| end_session_url(endpoint, id_token.as_deref(), &config.post_logout_url))
            .unwrap_or_else(|| config.post_logout_url.clone()),
        // Провайдер недоступен: локальная сессия уже уничтожена - этого достаточно
        Err(_) => config.post_logout_url.clone(),
    };

    Ok((jar, Redirect::to(&target)))
}

fn end_session_url(endpoint: &str, id_token: Option<&str>, post_logout: &str) -> String {
    let mut url = format!(
        "{endpoint}?post_logout_redirect_uri={}",
        urlencode(post_logout)
    );
    if let Some(token) = id_token {
        url.push_str("&id_token_hint=");
        url.push_str(&urlencode(token));
    }
    url
}

/// Процент-кодирование значения query-параметра (unreserved по RFC 3986).
fn urlencode(value: &str) -> String {
    value.bytes().fold(String::new(), |mut acc, byte| {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                acc.push(char::from(byte));
            }
            other => {
                use std::fmt::Write as _;
                let _ = write!(acc, "%{other:02X}");
            }
        }
        acc
    })
}

fn failure_url(login_path: &str, reason: &str) -> String {
    let separator = if login_path.contains('?') { '&' } else { '?' };
    format!("{login_path}{separator}oidc_error={reason}")
}

/// Локаль провайдера → одна из поддерживаемых (NFR-01); `kk-KZ` → `kk`.
fn locale_of(claim: Option<&str>) -> String {
    let language = claim
        .and_then(|value| value.split(['-', '_']).next())
        .unwrap_or("ru")
        .to_ascii_lowercase();
    match language.as_str() {
        "kk" => "kk".to_owned(),
        "en" => "en".to_owned(),
        _ => "ru".to_owned(),
    }
}

/// Роли из проверенного `id_token`.
///
/// Подпись и аудитория токена уже проверены библиотекой - здесь читается
/// полезная нагрузка той же строки. Claim ролей у провайдеров выглядит
/// по-разному (Zitadel - объект «роль → организации», иные - массив строк),
/// поддерживаются обе формы. Неизвестные значения игнорируются: список ролей
/// системы закрыт (INV-POL-01), а `guest` - аноним и в БД не хранится.
fn roles_from_id_token(raw_token: &str, claim: &str) -> Vec<Role> {
    let Some(payload) = raw_token.split('.').nth(1) else {
        return Vec::new();
    };
    let Some(json) = decode_base64url(payload) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&json) else {
        return Vec::new();
    };

    let names: Vec<&str> = match value.get(claim) {
        Some(serde_json::Value::Object(map)) => map.keys().map(String::as_str).collect(),
        Some(serde_json::Value::Array(items)) => {
            items.iter().filter_map(serde_json::Value::as_str).collect()
        }
        _ => Vec::new(),
    };

    let mut roles: Vec<Role> = names
        .into_iter()
        .filter_map(|name| name.parse::<Role>().ok())
        .filter(|role| !matches!(role, Role::Guest))
        .collect();
    roles.sort_by_key(|role| role.as_str());
    roles.dedup();
    roles
}

/// base64url без набивки (RFC 7515 § 2) - формат частей JWT.
fn decode_base64url(input: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buffer: u32 = 0;
    let mut bits = 0u32;
    for byte in input.bytes() {
        if byte == b'=' {
            break;
        }
        let index = ALPHABET.iter().position(|c| *c == byte)? as u32;
        buffer = (buffer << 6) | index;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token_with_payload(payload: serde_json::Value) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let json = payload.to_string();
        let mut encoded = String::new();
        for chunk in json.as_bytes().chunks(3) {
            let b = |i: usize| chunk.get(i).copied().unwrap_or(0) as u32;
            let block = (b(0) << 16) | (b(1) << 8) | b(2);
            let take = chunk.len() + 1;
            for i in 0..take {
                let index = ((block >> (18 - 6 * i)) & 0x3F) as usize;
                encoded.push(char::from(ALPHABET[index]));
            }
        }
        format!("header.{encoded}.signature")
    }

    #[test]
    fn zitadel_roles_claim_maps_to_domain_roles() {
        let token = token_with_payload(serde_json::json!({
            "sub": "42",
            DEFAULT_ROLES_CLAIM: {
                "organizer": { "org-1": "tou.edu.kz" },
                "secretary": { "org-1": "tou.edu.kz" },
            }
        }));

        assert_eq!(
            roles_from_id_token(&token, DEFAULT_ROLES_CLAIM),
            vec![Role::Organizer, Role::Secretary]
        );
    }

    #[test]
    fn array_roles_claim_is_supported() {
        let token = token_with_payload(serde_json::json!({ "roles": ["finance", "board"] }));
        assert_eq!(
            roles_from_id_token(&token, "roles"),
            vec![Role::Board, Role::Finance]
        );
    }

    #[test]
    fn unknown_and_guest_roles_are_ignored() {
        // guest - аноним (в core.role его нет), «superadmin» провайдера нам неизвестен
        let token = token_with_payload(serde_json::json!({
            "roles": ["guest", "superadmin", "admin"]
        }));
        assert_eq!(roles_from_id_token(&token, "roles"), vec![Role::Admin]);
    }

    #[test]
    fn missing_claim_yields_no_roles() {
        let token = token_with_payload(serde_json::json!({ "sub": "42" }));
        assert!(roles_from_id_token(&token, DEFAULT_ROLES_CLAIM).is_empty());
        assert!(roles_from_id_token("not-a-token", DEFAULT_ROLES_CLAIM).is_empty());
    }

    #[test]
    fn locale_claim_narrows_to_supported_languages() {
        assert_eq!(locale_of(Some("kk-KZ")), "kk");
        assert_eq!(locale_of(Some("en")), "en");
        assert_eq!(locale_of(Some("de-DE")), "ru");
        assert_eq!(locale_of(None), "ru");
    }

    #[test]
    fn end_session_url_carries_hint_and_return_target() {
        let url = end_session_url(
            "https://id.tou.edu.kz/oidc/v1/end_session",
            Some("token.value"),
            "https://rent.tou.edu.kz/",
        );
        assert_eq!(
            url,
            "https://id.tou.edu.kz/oidc/v1/end_session\
             ?post_logout_redirect_uri=https%3A%2F%2Frent.tou.edu.kz%2F\
             &id_token_hint=token.value"
        );
    }

    #[test]
    fn failure_url_keeps_existing_query() {
        assert_eq!(
            failure_url("/auth/login", "state"),
            "/auth/login?oidc_error=state"
        );
        assert_eq!(
            failure_url("/auth/login?next=/app", "denied"),
            "/auth/login?next=/app&oidc_error=denied"
        );
    }
}
