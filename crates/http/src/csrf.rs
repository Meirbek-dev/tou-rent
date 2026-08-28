//! CSRF double-submit cookie с привязкой токена к сессии (арх. § 5).
//!
//! Вход выдает случайный токен сразу в два места: в не-httpOnly cookie -
//! клиент читает ее и возвращает значение заголовком `x-csrf-token` на каждой
//! мутации, - и в саму серверную сессию. Middleware сверяет на небезопасных
//! методах тройку: cookie == заголовок == значение из сессии.
//!
//! Почему тройку, а не пару. Пара, согласованная сама с собой, ничего не
//! доказывает: обе ее половины задает любой, кто может поставить cookie на
//! домен, - например, захваченный поддомен (`*.tou.edu.kz` пишет cookie на
//! родительский домен). Он ставит жертве свой `tou_csrf` и тем же значением
//! шлет заголовок; сравнение cookie с заголовком проходит. Третья половина
//! живет только на сервере, браузеру не видна и подмене не поддается.
//!
//! Почему значение хранится в сессии, а не выводится из ее идентификатора
//! через HMAC. HMAC потребовал бы серверного секрета в конфигурации (еще одна
//! переменная окружения и еще один способ развернуть стенд неправильно), а
//! выдавать токен нужно ровно после `cycle_id()` - и в этот момент новый
//! идентификатор сессии еще не присвоен: `Session::id()` возвращает `None`
//! до сохранения, выводить не из чего. Хранение значения не требует ни
//! секрета, ни знания момента сохранения, и ротация сводится к перезаписи.

use axum::extract::Request;
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use tower_sessions::Session;

use crate::error::ApiError;

pub const CSRF_COOKIE: &str = "tou_csrf";
pub const CSRF_HEADER: &str = "x-csrf-token";

/// Ключ токена в сессии (tower-sessions, Redis) - по образцу
/// [`SESSION_USER_KEY`](crate::extract::SESSION_USER_KEY).
pub const SESSION_CSRF_KEY: &str = "csrf_token";

/// Мутации, доступные до логина: сессии и токена в ней еще нет, требовать их
/// здесь нечего. Все остальные мутации защищены по умолчанию (fail-closed).
///
/// Исключение сохранено осознанно. Убрать его можно было бы, выдавая
/// «пре-сессионный» токен на GET страницы входа, но это заводит серверную
/// сессию каждому анониму, открывшему `/auth/login` (в том числе поисковому
/// роботу), и требует от любого клиента входа предварительного GET - чего
/// не делают ни клиент контракта (`packages/api-client`), ни сценарии e2e,
/// ни вход по ссылке провайдера. Цена - тот же порядок защиты, что и был:
/// login-CSRF (жертву логинят в чужую учетную запись) остается на SameSite=Lax,
/// который отсекает межсайтовый POST. TODO-ENGINEER: если понадобится закрыть
/// и его, это отдельная задача с изменением контракта - точка выдачи токена
/// до входа и обязательный GET перед POST в клиенте.
const CSRF_EXEMPT: &[&str] = &[
    "/api/v1/auth/login",
    "/api/v1/auth/register",
    "/api/v1/auth/confirm-registration",
];

/// Выдать токен: одно и то же значение уходит и в сессию, и в cookie.
///
/// Вызывается сразу после `cycle_id()` на каждом входе - и локальном, и через
/// внешнего провайдера. Смена привилегий обязана менять и токен: иначе
/// значение, полученное до входа, продолжало бы подходить после него.
///
/// `secure` идет тем же значением, что и у сессионной cookie (`COOKIE_SECURE`):
/// токен без него уходил бы и по обычному HTTP, пока сессионная cookie -
/// только по TLS, и защита double-submit разъехалась бы с тем, что защищает.
pub async fn issue(session: &Session, jar: CookieJar, secure: bool) -> Result<CookieJar, ApiError> {
    let token = random_token();
    session
        .insert(SESSION_CSRF_KEY, &token)
        .await
        .map_err(ApiError::internal)?;

    Ok(jar.add(token_cookie(token, secure)))
}

/// Погасить токен в браузере (выход).
///
/// Сессия при выходе уничтожается, но cookie переживает ее: без явного
/// удаления страница после выхода продолжала бы отдавать «валидный» на вид
/// токен, а следующий вход в том же браузере какое-то время нес бы два
/// значения сразу. Гасящая копия обязана повторять `path` исходной, иначе
/// браузер сочтет ее другой cookie и удалит не ту.
pub fn revoke(jar: CookieJar, secure: bool) -> CookieJar {
    let mut cookie = token_cookie(String::new(), secure);
    cookie.make_removal();
    jar.add(cookie)
}

/// Middleware: на мутациях cookie, заголовок и сессия обязаны нести один токен.
pub async fn enforce(jar: CookieJar, request: Request, next: Next) -> Result<Response, ApiError> {
    let safe = matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    if !safe && !CSRF_EXEMPT.contains(&request.uri().path()) {
        // Сессия берется из расширений запроса, а не экстрактором: слой навешан
        // как `from_fn` без состояния, и отказ экстрактора превращал бы любую
        // ошибку сессионного слоя в 500 еще до разбора маршрута. Здесь
        // отсутствие сессии - обычный отказ CSRF, как и отсутствие cookie.
        let expected: Option<String> = match request.extensions().get::<Session>().cloned() {
            Some(session) => session
                .get(SESSION_CSRF_KEY)
                .await
                .map_err(ApiError::internal)?,
            None => None,
        };

        let from_cookie = jar.get(CSRF_COOKIE).map(|cookie| cookie.value().to_owned());
        let from_header = request
            .headers()
            .get(CSRF_HEADER)
            .and_then(|value| value.to_str().ok());

        let matched = match (&from_cookie, from_header, &expected) {
            (Some(cookie), Some(header), Some(session)) => {
                same_secret(cookie, header) && same_secret(cookie, session)
            }
            _ => false,
        };
        if !matched {
            return Err(ApiError::CsrfRejected);
        }
    }
    Ok(next.run(request).await)
}

/// Cookie токена. `http_only` выключен намеренно: значение читает клиент,
/// чтобы вернуть его заголовком, - в этом и состоит double-submit.
fn token_cookie(token: String, secure: bool) -> Cookie<'static> {
    Cookie::build((CSRF_COOKIE, token))
        .path("/")
        .http_only(false)
        .same_site(SameSite::Lax)
        .secure(secure)
        .build()
}

fn random_token() -> String {
    let bytes: [u8; 32] = rand::random();
    bytes.iter().fold(String::with_capacity(64), |mut acc, b| {
        use std::fmt::Write as _;
        // Запись в String не фейлится; неудача форматирования здесь невозможна
        let _ = write!(acc, "{b:02x}");
        acc
    })
}

/// Сравнение секретов за время, не зависящее от длины совпавшего префикса:
/// заголовок приходит от клиента и подбирается им же, а обычное сравнение
/// строк останавливается на первом различии.
fn same_secret(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0u8, |acc, (l, r)| acc | (l ^ r))
            == 0
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use axum::response::Response;
    use axum::routing::{get, post};
    use tower::ServiceExt as _;
    use tower_sessions::{MemoryStore, SessionManagerLayer};

    use super::*;

    /// Стенд ровно из рубежа CSRF: выдача токена, выход и защищаемая мутация.
    /// Полный роутер тянет за собой Postgres и Redis - проверять на нем один
    /// слой нельзя, а сам слой от них не зависит.
    fn stand() -> Router {
        Router::new()
            .route(
                "/issue",
                get(|session: Session, jar: CookieJar| async move {
                    issue(&session, jar, false).await
                }),
            )
            // Выход в бою - POST; здесь GET, чтобы проверять гашение cookie,
            // а не повторно тот же рубеж, который выход и должен пережить
            .route(
                "/logout",
                get(|session: Session, jar: CookieJar| async move {
                    session.flush().await.map_err(ApiError::internal)?;
                    Ok::<_, ApiError>(revoke(jar, false))
                }),
            )
            .route("/mutate", post(|| async { StatusCode::NO_CONTENT }))
            .layer(axum::middleware::from_fn(enforce))
            .layer(SessionManagerLayer::new(MemoryStore::default()).with_secure(false))
    }

    async fn send(
        app: &Router,
        method: &str,
        uri: &str,
        cookies: &str,
        token: Option<&str>,
    ) -> Response {
        let mut builder = Request::builder().method(method).uri(uri);
        if !cookies.is_empty() {
            builder = builder.header(header::COOKIE, cookies);
        }
        if let Some(token) = token {
            builder = builder.header(CSRF_HEADER, token);
        }
        app.clone()
            .oneshot(builder.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    fn set_cookies(response: &Response) -> Vec<Cookie<'static>> {
        response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .filter_map(|value| Cookie::parse(value.to_owned()).ok())
            .collect()
    }

    fn cookie_header(cookies: &[Cookie<'static>]) -> String {
        cookies
            .iter()
            .map(|cookie| format!("{}={}", cookie.name(), cookie.value()))
            .collect::<Vec<_>>()
            .join("; ")
    }

    fn token_of(cookies: &[Cookie<'static>]) -> String {
        cookies
            .iter()
            .find(|cookie| cookie.name() == CSRF_COOKIE)
            .expect("токен выдан")
            .value()
            .to_owned()
    }

    /// Сессионная cookie одного ответа + CSRF-токен другого: пара cookie
    /// и заголовок согласованы, сессия - чужая.
    fn mixed(session_from: &[Cookie<'static>], token: &str) -> String {
        let mut parts: Vec<String> = session_from
            .iter()
            .filter(|cookie| cookie.name() != CSRF_COOKIE)
            .map(|cookie| format!("{}={}", cookie.name(), cookie.value()))
            .collect();
        parts.push(format!("{CSRF_COOKIE}={token}"));
        parts.join("; ")
    }

    /// Токен double-submit защищает сессию и обязан ездить по тем же
    /// правилам: `Secure` в проде, без него - на дев-стенде по HTTP.
    #[tokio::test]
    async fn secure_flag_follows_session_cookie() {
        for secure in [true, false] {
            let session = Session::new(None, Arc::new(MemoryStore::default()), None);
            let jar = issue(&session, CookieJar::new(), secure)
                .await
                .expect("токен выдан");
            let cookie = jar.get(CSRF_COOKIE).expect("cookie выдана");

            assert_eq!(cookie.secure(), Some(secure));
            assert_eq!(cookie.http_only(), Some(false), "токен читает клиент");

            let stored: String = session
                .get(SESSION_CSRF_KEY)
                .await
                .expect("чтение сессии")
                .expect("токен записан в сессию");
            assert_eq!(cookie.value(), stored, "cookie и сессия несут один токен");
        }
    }

    /// Рубеж, ради которого токен и связан с сессией: согласованную пару
    /// cookie+заголовок целиком задает тот, кто может поставить cookie на
    /// домен. Токен обязан подходить только к той сессии, которой выдан.
    #[tokio::test]
    async fn token_of_another_session_is_rejected() {
        let app = stand();

        let mine = set_cookies(&send(&app, "GET", "/issue", "", None).await);
        let stranger = set_cookies(&send(&app, "GET", "/issue", "", None).await);
        let token = token_of(&mine);

        let allowed = send(&app, "POST", "/mutate", &cookie_header(&mine), Some(&token)).await;
        assert_eq!(
            allowed.status(),
            StatusCode::NO_CONTENT,
            "своя пара в своей сессии обязана проходить"
        );

        let forged = send(
            &app,
            "POST",
            "/mutate",
            &mixed(&stranger, &token),
            Some(&token),
        )
        .await;
        assert_eq!(
            forged.status(),
            StatusCode::FORBIDDEN,
            "токен чужой сессии не должен проходить double-submit"
        );
    }

    /// Выход обязан гасить и cookie: сессии уже нет, а токен в браузере
    /// пережил бы ее и достался следующему входу.
    #[tokio::test]
    async fn logout_expires_csrf_cookie() {
        let app = stand();

        let issued = set_cookies(&send(&app, "GET", "/issue", "", None).await);
        let token = token_of(&issued);
        let cookies = cookie_header(&issued);

        let cleared = set_cookies(&send(&app, "GET", "/logout", &cookies, None).await);
        let removal = cleared
            .iter()
            .find(|cookie| cookie.name() == CSRF_COOKIE)
            .expect("cookie токена гасится");
        assert_eq!(removal.value(), "");
        assert_eq!(
            removal.path(),
            Some("/"),
            "гасится та же cookie, что выдана"
        );
        assert!(
            removal.max_age().is_some_and(|age| age.is_zero()),
            "гасящая cookie обязана истекать немедленно"
        );

        let after = send(&app, "POST", "/mutate", &cookies, Some(&token)).await;
        assert_eq!(
            after.status(),
            StatusCode::FORBIDDEN,
            "после выхода прежний токен не действует"
        );
    }

    /// Безопасные методы и дологинные пути токена не требуют - иначе войти
    /// было бы нечем.
    #[tokio::test]
    async fn safe_methods_pass_without_token() {
        let app = stand();
        let response = send(&app, "GET", "/issue", "", None).await;
        assert_eq!(response.status(), StatusCode::OK);
    }
}
