//! HTTP request extractors whose rejections follow the API problem contract (NFR-08).
//!
//! Отказ экстрактора формулирует serde, и формулирует по-английски: «Failed
//! to deserialize the JSON body into the target type: area_m2: invalid type:
//! null, expected a Decimal type...». Пока эта строка уезжала в `detail`,
//! она же появлялась на экране: веб рисует `detail` как есть, и это была
//! единственная строка интерфейса мимо Paraglide (NFR-01) - да еще и с
//! именами внутренних типов (NFR-07).
//!
//! Поэтому наружу идет машинная пара «поле: причина» (`area_m2: invalid_type`,
//! `limit: invalid_digit`) - по ней клиент подставляет свой перевод и
//! подсвечивает поле, - а полный текст остается в телеметрии дежурному.

use std::ops::{Deref, DerefMut};

use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::ApiError;

pub struct Json<T>(pub T);

impl<S, T> FromRequest<S> for Json<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        axum::Json::<T>::from_request(request, state)
            .await
            .map(|axum::Json(value)| Self(value))
            .map_err(json_rejection)
    }
}

impl<T> IntoResponse for Json<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

pub struct Path<T>(pub T);

impl<S, T> FromRequestParts<S> for Path<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        axum::extract::Path::<T>::from_request_parts(parts, state)
            .await
            .map(|axum::extract::Path(value)| Self(value))
            .map_err(path_rejection)
    }
}

pub struct Query<T>(pub T);

impl<S, T> FromRequestParts<S> for Query<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        axum::extract::Query::<T>::from_request_parts(parts, state)
            .await
            .map(|axum::extract::Query(value)| Self(value))
            .map_err(query_rejection)
    }
}

pub struct Multipart(axum::extract::Multipart);

impl<S> FromRequest<S> for Multipart
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        axum::extract::Multipart::from_request(request, state)
            .await
            .map(Self)
            .map_err(|error| request_rejection(error.status(), error.body_text()))
    }
}

impl Deref for Multipart {
    type Target = axum::extract::Multipart;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Multipart {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

fn json_rejection(error: JsonRejection) -> ApiError {
    request_rejection(error.status(), error.body_text())
}

fn request_rejection(status: axum::http::StatusCode, detail: String) -> ApiError {
    // Полная формулировка serde - дежурному: по ней видно, какой тип не
    // сошелся и на какой строке тела
    tracing::debug!(%status, %detail, "запрос отклонен экстрактором");

    match status {
        axum::http::StatusCode::PAYLOAD_TOO_LARGE => {
            ApiError::PayloadTooLarge("payload_too_large".to_owned())
        }
        axum::http::StatusCode::UNSUPPORTED_MEDIA_TYPE => {
            ApiError::UnsupportedMediaType("unsupported_media_type".to_owned())
        }
        axum::http::StatusCode::UNPROCESSABLE_ENTITY => {
            ApiError::Validation(machine_detail(&detail))
        }
        _ => ApiError::bad_request(machine_detail(&detail)),
    }
}

fn path_rejection(error: PathRejection) -> ApiError {
    request_rejection(axum::http::StatusCode::BAD_REQUEST, error.body_text())
}

fn query_rejection(error: QueryRejection) -> ApiError {
    request_rejection(axum::http::StatusCode::BAD_REQUEST, error.body_text())
}

/// Приставки, которыми axum предваряет формулировку serde.
const PREFIXES: [&str; 4] = [
    "Failed to deserialize the JSON body into the target type: ",
    "Failed to deserialize query string: ",
    "Failed to deserialize the query string: ",
    "Failed to deserialize form body: ",
];

/// Отказ разбора как машинная пара «поле: причина».
///
/// Разбор идет по тексту, потому что другого носителя у него нет: axum
/// отдает отказ одной строкой (`body_text`), а типизированный источник
/// (`serde_path_to_error`) в этот слой не пробрасывается. Не разобралось -
/// наружу уходит `invalid`, а не английская фраза.
fn machine_detail(body_text: &str) -> String {
    if body_text.starts_with("Failed to parse the request body as JSON") {
        return "invalid_json".to_owned();
    }

    let rest = PREFIXES
        .iter()
        .find_map(|prefix| body_text.strip_prefix(prefix))
        .unwrap_or(body_text);

    // Путь serde стоит первым и пробелов не содержит: `area_m2`,
    // `lots[0].area_m2`. Дальше двоеточие встречается и внутри самой причины
    // («invalid type: null, expected...»), поэтому делится только первое.
    let (path, reason) = match rest.split_once(": ") {
        Some((path, reason)) if !path.is_empty() && !path.contains(' ') => (Some(path), reason),
        _ => (None, rest),
    };

    // Имя поля serde называет и в обратных кавычках («missing field `title`»)
    let field = path.or_else(|| backticked(reason));
    let code = reason_code(reason);

    match field {
        Some(field) => format!("{field}: {code}"),
        None => code.to_owned(),
    }
}

/// Первое имя в обратных кавычках - им serde называет поле.
fn backticked(text: &str) -> Option<&str> {
    let (_, tail) = text.split_once('`')?;
    let (name, _) = tail.split_once('`')?;
    (!name.is_empty() && !name.contains(' ')).then_some(name)
}

/// Причина отказа машинным словом. Перечень закрыт: неизвестная причина
/// становится `invalid`, а не английской фразой на экране.
fn reason_code(reason: &str) -> &'static str {
    let reason = reason.to_ascii_lowercase();
    if reason.starts_with("missing field") {
        "missing"
    } else if reason.starts_with("invalid type") {
        "invalid_type"
    } else if reason.starts_with("unknown field") {
        "unknown_field"
    } else if reason.starts_with("duplicate field") {
        "duplicate_field"
    } else if reason.starts_with("invalid length") {
        "invalid_length"
    } else if reason.contains("invalid digit") {
        "invalid_digit"
    } else if reason.contains("out of range") || reason.contains("too large") {
        "out_of_range"
    } else if reason.starts_with("invalid value") || reason.contains("cannot parse") {
        "invalid_value"
    } else {
        "invalid"
    }
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode, header};
    use axum::routing::{get, post};
    use serde::Deserialize;
    use serde_json::Value;
    use tower::ServiceExt as _;
    use uuid::Uuid;

    use super::*;

    #[derive(Deserialize)]
    struct Input {
        value: String,
    }

    #[derive(Deserialize)]
    struct Paging {
        limit: usize,
    }

    async fn path_and_json(Path(id): Path<Uuid>, Json(input): Json<Input>) -> String {
        format!("{id}:{}", input.value)
    }

    async fn query(Query(paging): Query<Paging>) -> String {
        paging.limit.to_string()
    }

    async fn multipart(_multipart: Multipart) {}

    async fn assert_problem(response: Response, expected_status: StatusCode) {
        assert_eq!(response.status(), expected_status);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE),
            Some(&header::HeaderValue::from_static(
                "application/problem+json"
            ))
        );
        let bytes = to_bytes(response.into_body(), 16 * 1024)
            .await
            .expect("problem body");
        let problem: Value = serde_json::from_slice(&bytes).expect("valid problem json");
        assert_eq!(problem["status"], expected_status.as_u16());
        assert!(
            problem["type"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        );
        assert!(
            problem["code"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        );
    }

    #[tokio::test]
    async fn path_and_json_rejections_are_problem_json() {
        let app = Router::new().route("/items/{id}", post(path_and_json));

        let invalid_path = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/items/not-a-uuid")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"value":"ok"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_problem(invalid_path, StatusCode::BAD_REQUEST).await;

        let invalid_json = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/items/{}", Uuid::nil()))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{"))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_problem(invalid_json, StatusCode::BAD_REQUEST).await;
    }

    #[tokio::test]
    async fn query_and_multipart_rejections_are_problem_json() {
        let app = Router::new()
            .route("/query", get(query))
            .route("/upload", post(multipart));

        let invalid_query = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/query?limit=none")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_problem(invalid_query, StatusCode::BAD_REQUEST).await;

        let invalid_multipart = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/upload")
                    .header(header::CONTENT_TYPE, "multipart/form-data")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_problem(invalid_multipart, StatusCode::BAD_REQUEST).await;
    }

    /// Рубеж AC-9: наружу уходит машинная пара «поле: причина», а не
    /// формулировка serde. Веб рисует `detail` как есть, и английская строка
    /// с именем внутреннего типа была единственной строкой интерфейса мимо
    /// Paraglide (NFR-01).
    #[test]
    fn deserializer_wording_does_not_reach_the_client() {
        assert_eq!(
            machine_detail(
                "Failed to deserialize the JSON body into the target type: \
                 area_m2: invalid type: null, expected a Decimal type representing a fixed-point number"
            ),
            "area_m2: invalid_type"
        );
        assert_eq!(
            machine_detail(
                "Failed to deserialize query string: limit: invalid digit found in string"
            ),
            "limit: invalid_digit"
        );
        assert_eq!(
            machine_detail(
                "Failed to deserialize the JSON body into the target type: missing field `title` at line 1 column 2"
            ),
            "title: missing"
        );
        assert_eq!(
            machine_detail("Failed to parse the request body as JSON: EOF while parsing a value"),
            "invalid_json"
        );

        for text in [
            "Failed to deserialize the JSON body into the target type: area_m2: invalid type: null",
            "Failed to deserialize query string: limit: invalid digit found in string",
            "Invalid URL: Cannot parse `id` with value `x` to a `Uuid`",
        ] {
            let detail = machine_detail(text);
            assert!(
                !detail.contains(' ') || detail.split(": ").count() == 2,
                "деталь обязана быть машинной: {detail}"
            );
            assert!(
                !detail.to_ascii_lowercase().contains("failed"),
                "формулировка десериализатора ушла наружу: {detail}"
            );
        }
    }

    /// Тело сверх потолка и чужой тип содержимого объясняются кодом, а не
    /// английской фразой axum.
    #[tokio::test]
    async fn limit_and_media_type_rejections_are_machine_readable() {
        let app = Router::new()
            .route("/items/{id}", post(path_and_json))
            .layer(axum::extract::DefaultBodyLimit::max(4));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/items/{}", Uuid::nil()))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"value":"too large"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");

        let bytes = to_bytes(response.into_body(), 16 * 1024)
            .await
            .expect("problem body");
        let problem: Value = serde_json::from_slice(&bytes).expect("valid problem json");
        assert_eq!(problem["detail"], "payload_too_large");
    }

    #[tokio::test]
    async fn body_limit_rejection_preserves_413_problem_status() {
        let app = Router::new()
            .route("/items/{id}", post(path_and_json))
            .layer(axum::extract::DefaultBodyLimit::max(4));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/items/{}", Uuid::nil()))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"value":"too large"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_problem(response, StatusCode::PAYLOAD_TOO_LARGE).await;
    }
}
