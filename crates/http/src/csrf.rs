//! CSRF double-submit cookie (арх. § 5).
//!
//! Логин выдает случайный токен в не-httpOnly cookie; клиент возвращает его
//! в заголовке `x-csrf-token` при каждой мутации. Middleware сверяет значения
//! на небезопасных методах. Сессионная cookie httpOnly SameSite=Lax -
//! double-submit добирает случаи, которые Lax не покрывает.

use axum::extract::Request;
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};

use crate::error::ApiError;

pub const CSRF_COOKIE: &str = "tou_csrf";
pub const CSRF_HEADER: &str = "x-csrf-token";

/// Мутации, доступные до логина: CSRF-cookie еще не выдана, их защищает
/// SameSite=Lax. Все остальные мутации защищены по умолчанию (fail-closed).
const CSRF_EXEMPT: &[&str] = &["/api/v1/auth/login", "/api/v1/auth/register"];

/// Случайный токен и cookie для него (выдается при логине).
///
/// `secure` идет тем же значением, что и у сессионной cookie (`COOKIE_SECURE`):
/// токен без него уходил бы и по обычному HTTP, пока сессионная cookie -
/// только по TLS, и защита double-submit разъехалась бы с тем, что защищает.
pub fn issue(jar: CookieJar, secure: bool) -> (CookieJar, String) {
    let bytes: [u8; 32] = rand::random();
    let token: String = bytes.iter().fold(String::with_capacity(64), |mut acc, b| {
        use std::fmt::Write as _;
        // Запись в String не фейлится; неудача форматирования здесь невозможна
        let _ = write!(acc, "{b:02x}");
        acc
    });

    let cookie = Cookie::build((CSRF_COOKIE, token.clone()))
        .path("/")
        .http_only(false) // клиент читает токен, чтобы вернуть его заголовком
        .same_site(SameSite::Lax)
        .secure(secure)
        .build();

    (jar.add(cookie), token)
}

/// Middleware: на мутациях cookie и заголовок обязаны совпадать.
pub async fn enforce(jar: CookieJar, request: Request, next: Next) -> Result<Response, ApiError> {
    let safe = matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    if !safe && !CSRF_EXEMPT.contains(&request.uri().path()) {
        let cookie_token = jar.get(CSRF_COOKIE).map(|c| c.value().to_owned());
        let header_token = request
            .headers()
            .get(CSRF_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);

        match (cookie_token, header_token) {
            (Some(from_cookie), Some(from_header)) if from_cookie == from_header => {}
            _ => return Err(ApiError::CsrfRejected),
        }
    }
    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::{CSRF_COOKIE, issue};
    use axum_extra::extract::CookieJar;

    /// Токен double-submit защищает сессию и обязан ездить по тем же
    /// правилам: `Secure` в проде, без него - на дев-стенде по HTTP.
    #[test]
    fn secure_flag_follows_session_cookie() {
        for secure in [true, false] {
            let (jar, token) = issue(CookieJar::new(), secure);
            let cookie = jar.get(CSRF_COOKIE).expect("cookie выдана");

            assert_eq!(cookie.secure(), Some(secure));
            assert_eq!(cookie.value(), token);
            assert_eq!(cookie.http_only(), Some(false), "токен читает клиент");
        }
    }
}
