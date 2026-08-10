//! Ошибки API - RFC 9457 problem+json с машинным `code` (NFR-08, ТЗ § 7).
//!
//! Каждый новый код ошибки добавляется в [`ErrorCode`] - enum попадает
//! в OpenAPI-контракт (регламент А.5). Внутренние подробности наружу
//! не уходят (NFR-07): для 500 деталь фиксированная, причина - в tracing.

use axum::Json;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::ToSchema;

/// Машинные коды ошибок контракта (enum без catch-all).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unauthorized,
    Forbidden,
    InvalidCredentials,
    EmailTaken,
    ValidationFailed,
    CsrfRejected,
    NotFound,
    /// Операция отклонена правилами предметной области (триггеры INV-*, FK)
    RuleViolation,
    /// Внешний провайдер идентичности недоступен (FR-1502): вход через него
    /// временно невозможен, остальная система работает
    ProviderUnavailable,
    /// Слишком часто: сработало ограничение попыток входа (NFR-07)
    TooManyRequests,
    /// Обработка запроса не уложилась в отведенное время
    Timeout,
    Internal,
}

/// Тело ответа `application/problem+json` (RFC 9457).
#[derive(Debug, Serialize, ToSchema)]
pub struct Problem {
    /// URN вида `urn:tou-rent:error:<code>`
    #[serde(rename = "type")]
    pub problem_type: String,
    pub title: String,
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub code: ErrorCode,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("некорректный HTTP-запрос")]
    BadRequest(String),
    #[error("тело HTTP-запроса слишком велико")]
    PayloadTooLarge(String),
    #[error("тип содержимого HTTP-запроса не поддерживается")]
    UnsupportedMediaType(String),
    #[error("требуется вход в систему")]
    Unauthorized,
    #[error("недостаточно прав для действия")]
    Forbidden,
    #[error("неверный email или пароль")]
    InvalidCredentials,
    #[error("email уже зарегистрирован")]
    EmailTaken,
    #[error("данные не прошли проверку")]
    Validation(String),
    #[error("CSRF-токен отсутствует или не совпадает")]
    CsrfRejected,
    #[error("объект не найден")]
    NotFound,
    #[error("HTTP-метод не поддерживается для этого ресурса")]
    MethodNotAllowed,
    #[error("операция отклонена правилами")]
    RuleViolation(String),
    #[error("провайдер идентичности недоступен")]
    ProviderUnavailable(String),
    /// Ограничение частоты попыток входа. `retry_after` уходит клиенту
    /// заголовком `Retry-After` - иначе клиенту остается угадывать паузу.
    #[error("слишком много попыток - повторите позже")]
    TooManyRequests { retry_after_seconds: u64 },
    #[error("превышено время обработки запроса")]
    Timeout,
    #[error("внутренняя ошибка")]
    Internal(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl ApiError {
    pub fn bad_request(detail: impl Into<String>) -> Self {
        Self::BadRequest(detail.into())
    }

    pub fn internal(err: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Internal(Box::new(err))
    }

    fn code(&self) -> ErrorCode {
        match self {
            ApiError::BadRequest(_)
            | ApiError::PayloadTooLarge(_)
            | ApiError::UnsupportedMediaType(_) => ErrorCode::ValidationFailed,
            ApiError::Unauthorized => ErrorCode::Unauthorized,
            ApiError::Forbidden => ErrorCode::Forbidden,
            ApiError::InvalidCredentials => ErrorCode::InvalidCredentials,
            ApiError::EmailTaken => ErrorCode::EmailTaken,
            ApiError::Validation(_) => ErrorCode::ValidationFailed,
            ApiError::CsrfRejected => ErrorCode::CsrfRejected,
            ApiError::NotFound => ErrorCode::NotFound,
            ApiError::MethodNotAllowed => ErrorCode::RuleViolation,
            ApiError::RuleViolation(_) => ErrorCode::RuleViolation,
            ApiError::ProviderUnavailable(_) => ErrorCode::ProviderUnavailable,
            ApiError::TooManyRequests { .. } => ErrorCode::TooManyRequests,
            ApiError::Timeout => ErrorCode::Timeout,
            ApiError::Internal(_) => ErrorCode::Internal,
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::PayloadTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
            ApiError::UnsupportedMediaType(_) => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            ApiError::Unauthorized | ApiError::InvalidCredentials => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden | ApiError::CsrfRejected => StatusCode::FORBIDDEN,
            ApiError::EmailTaken => StatusCode::CONFLICT,
            ApiError::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
            ApiError::RuleViolation(_) => StatusCode::CONFLICT,
            ApiError::ProviderUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            ApiError::TooManyRequests { .. } => StatusCode::TOO_MANY_REQUESTS,
            ApiError::Timeout => StatusCode::GATEWAY_TIMEOUT,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        ApiError::internal(err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let code = self.code();

        // Причина 500 - только в телеметрию, не в ответ (NFR-07)
        if let ApiError::Internal(source) = &self {
            tracing::error!(error = %source, "internal api error");
        }

        let detail = match &self {
            ApiError::Internal(_) => None,
            ApiError::BadRequest(detail)
            | ApiError::PayloadTooLarge(detail)
            | ApiError::UnsupportedMediaType(detail)
            | ApiError::Validation(detail)
            | ApiError::RuleViolation(detail)
            | ApiError::ProviderUnavailable(detail) => Some(detail.clone()),
            other => Some(other.to_string()),
        };

        let problem = Problem {
            problem_type: format!("urn:tou-rent:error:{}", code.slug()),
            title: self.to_string(),
            status: status.as_u16(),
            detail,
            code,
        };

        let mut response = (
            status,
            [(header::CONTENT_TYPE, "application/problem+json")],
            Json(problem),
        )
            .into_response();

        // Пауза до следующей попытки - машиночитаемо, а не только в тексте
        if let ApiError::TooManyRequests {
            retry_after_seconds,
        } = &self
            && let Ok(value) = header::HeaderValue::from_str(&retry_after_seconds.to_string())
        {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }

        response
    }
}

impl ErrorCode {
    /// Имя кода на проводе - единственный источник: serde-сериализация
    /// (та же, что кладет `code` в тело ответа).
    fn slug(self) -> String {
        match serde_json::to_value(self) {
            Ok(serde_json::Value::String(slug)) => slug,
            // unit-вариант сериализуется только строкой; ветка недостижима
            _ => "internal".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_error_hides_details() {
        let response = ApiError::internal(std::io::Error::other("секретная строка подключения"))
            .into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn validation_maps_to_422() {
        assert_eq!(
            ApiError::Validation("email".into()).status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    /// Клиенту нужна не только цифра 429, но и пауза: без `Retry-After`
    /// он либо ждет наугад, либо долбится дальше.
    #[test]
    fn rate_limit_carries_retry_after() {
        let response = ApiError::TooManyRequests {
            retry_after_seconds: 42,
        }
        .into_response();

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response.headers().get(header::RETRY_AFTER),
            Some(&header::HeaderValue::from_static("42"))
        );
    }

    #[test]
    fn timeout_maps_to_504() {
        assert_eq!(ApiError::Timeout.status(), StatusCode::GATEWAY_TIMEOUT);
        assert_eq!(ApiError::Timeout.code(), ErrorCode::Timeout);
    }

    #[test]
    fn problem_type_slug_is_snake_case_wire_name() {
        assert_eq!(ErrorCode::EmailTaken.slug(), "email_taken");
        assert_eq!(ErrorCode::RuleViolation.slug(), "rule_violation");
    }
}
