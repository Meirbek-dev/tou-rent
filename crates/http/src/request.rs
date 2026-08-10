//! HTTP request extractors whose rejections follow the API problem contract (NFR-08).

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
    match status {
        axum::http::StatusCode::PAYLOAD_TOO_LARGE => ApiError::PayloadTooLarge(detail),
        axum::http::StatusCode::UNSUPPORTED_MEDIA_TYPE => ApiError::UnsupportedMediaType(detail),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY => ApiError::Validation(detail),
        _ => ApiError::bad_request(detail),
    }
}

fn path_rejection(error: PathRejection) -> ApiError {
    ApiError::bad_request(error.body_text())
}

fn query_rejection(error: QueryRejection) -> ApiError {
    ApiError::bad_request(error.body_text())
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
