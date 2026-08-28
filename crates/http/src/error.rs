//! Ошибки API - RFC 9457 problem+json с машинным `code` (NFR-08, ТЗ § 7).
//!
//! Каждый новый код ошибки добавляется в [`ErrorCode`] - enum попадает
//! в OpenAPI-контракт (регламент А.5). Внутренние подробности наружу
//! не уходят (NFR-07): для 500 деталь фиксированная, причина - в tracing.
//!
//! То же и для отказа по правилу: `code` говорит «rule_violation» всем
//! отказам сразу, поэтому причину несет отдельное поле `rule` из закрытого
//! перечня [`tou_domain::rule::RuleViolation`]. Текст правила остается
//! внутри - он написан по-русски мимо Paraglide и назван именами
//! инвариантов, а интерфейс обязан говорить на языке пользователя (NFR-01).

use axum::Json;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use tou_domain::rule::{RuleRejection, RuleViolation};
use utoipa::ToSchema;

/// Машинные коды ошибок контракта (enum без catch-all).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unauthorized,
    Forbidden,
    InvalidCredentials,
    EmailTaken,
    IdNumberTaken,
    VerificationFailed,
    ValidationFailed,
    CsrfRejected,
    NotFound,
    /// Операция отклонена правилами предметной области (триггеры INV-*, FK)
    RuleViolation,
    /// Внешний провайдер идентичности недоступен (FR-1502): вход через него
    /// временно невозможен, остальная система работает
    ProviderUnavailable,
    /// Слишком часто: сработало ограничение попыток входа либо обращений
    /// к дорогим маршрутам (NFR-07)
    TooManyRequests,
    /// Запрос с тем же `Idempotency-Key` еще выполняется (ТЗ § 7): повторять
    /// нужно после того, как завершится первый, - тогда придет его ответ
    IdempotencyInFlight,
    /// Обработка запроса не уложилась в отведенное время
    Timeout,
    Internal,
}

/// Причина отказа по правилу на проводе.
///
/// Схема собирается из [`RuleViolation::ALL`], а не переписывается вторым
/// enum'ом рядом: вариантов полсотни, и зеркало разошлось бы с перечнем при
/// первой же новой причине - молча и в контракте.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(transparent)]
pub struct RuleDto(pub RuleViolation);

impl utoipa::PartialSchema for RuleDto {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::String)
            .description(Some(
                "Причина отказа по правилу предметной области (закрытый перечень)",
            ))
            .enum_values(Some(RuleViolation::ALL.iter().map(|rule| rule.as_str())))
            .into()
    }
}

impl utoipa::ToSchema for RuleDto {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Rule")
    }
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
    /// Причина отказа по правилу - только у `rule_violation`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<RuleDto>,
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
    #[error("ИИН/БИН уже зарегистрирован")]
    IdNumberTaken,
    #[error("код подтверждения неверен или истёк")]
    VerificationFailed,
    #[error("данные не прошли проверку")]
    Validation(String),
    #[error("CSRF-токен отсутствует или не совпадает")]
    CsrfRejected,
    #[error("объект не найден")]
    NotFound,
    #[error("HTTP-метод не поддерживается для этого ресурса")]
    MethodNotAllowed,
    #[error("операция отклонена правилами")]
    RuleViolation(RuleRejection),
    #[error("провайдер идентичности недоступен")]
    ProviderUnavailable(String),
    /// Ограничение частоты попыток входа. `retry_after` уходит клиенту
    /// заголовком `Retry-After` - иначе клиенту остается угадывать паузу.
    #[error("слишком много попыток - повторите позже")]
    TooManyRequests { retry_after_seconds: u64 },
    /// Параллельный повтор мутации с тем же `Idempotency-Key`: первый запрос
    /// еще выполняется, и выполнять операцию второй раз нельзя
    /// (см. [`crate::idempotency`]).
    #[error("запрос с этим ключом идемпотентности еще выполняется")]
    IdempotencyInFlight,
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

    /// Отказ по правилу с явно названной причиной.
    ///
    /// Причина ставится в месте отказа, а не выводится из текста: правила,
    /// проверенные приложением (а не триггером), идентификатора в сообщении
    /// не несут, и угадывать по русской строке - тот же провод, только
    /// хрупче.
    pub fn rule(rule: RuleViolation, internal: impl Into<String>) -> Self {
        Self::RuleViolation(RuleRejection::new(rule, internal))
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
            ApiError::IdNumberTaken => ErrorCode::IdNumberTaken,
            ApiError::VerificationFailed => ErrorCode::VerificationFailed,
            ApiError::Validation(_) => ErrorCode::ValidationFailed,
            ApiError::CsrfRejected => ErrorCode::CsrfRejected,
            ApiError::NotFound => ErrorCode::NotFound,
            ApiError::MethodNotAllowed => ErrorCode::RuleViolation,
            ApiError::RuleViolation(_) => ErrorCode::RuleViolation,
            ApiError::ProviderUnavailable(_) => ErrorCode::ProviderUnavailable,
            ApiError::TooManyRequests { .. } => ErrorCode::TooManyRequests,
            ApiError::IdempotencyInFlight => ErrorCode::IdempotencyInFlight,
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
            ApiError::EmailTaken | ApiError::IdNumberTaken | ApiError::IdempotencyInFlight => {
                StatusCode::CONFLICT
            }
            ApiError::Validation(_) | ApiError::VerificationFailed => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
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

        // Текст правила - тоже только в телеметрию: дежурному он нужен
        // целиком, с именем инварианта и значениями полей, а пользователю
        // предназначена переведенная строка по `rule`.
        if let ApiError::RuleViolation(rejection) = &self {
            tracing::info!(
                rule = rejection.rule().as_str(),
                reason = %rejection.internal(),
                "rule violation"
            );
        }

        let detail = match &self {
            // Отказ по правилу объясняется полем `rule`, а не текстом
            ApiError::Internal(_) | ApiError::RuleViolation(_) => None,
            ApiError::BadRequest(detail)
            | ApiError::PayloadTooLarge(detail)
            | ApiError::UnsupportedMediaType(detail)
            | ApiError::Validation(detail)
            | ApiError::ProviderUnavailable(detail) => Some(detail.clone()),
            other => Some(other.to_string()),
        };

        let rule = match &self {
            ApiError::RuleViolation(rejection) => Some(RuleDto(rejection.rule())),
            _ => None,
        };

        let problem = Problem {
            problem_type: format!("urn:tou-rent:error:{}", code.slug()),
            title: self.to_string(),
            status: status.as_u16(),
            detail,
            code,
            rule,
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

    /// Параллельный повтор - конфликт состояния, а не отказ по правилу
    /// предметной области: причины из перечня `RuleViolation` у него нет,
    /// и подставлять чужую нельзя.
    #[test]
    fn parallel_repeat_maps_to_409_with_its_own_code() {
        assert_eq!(ApiError::IdempotencyInFlight.status(), StatusCode::CONFLICT);
        assert_eq!(
            ApiError::IdempotencyInFlight.code(),
            ErrorCode::IdempotencyInFlight
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

    /// Тело ответа как его увидит клиент.
    async fn problem_of(error: ApiError) -> serde_json::Value {
        let response = error.into_response();
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("тело problem+json читается целиком");
        serde_json::from_slice(&bytes).expect("problem+json разбирается")
    }

    /// Рубеж задачи W-09: текст отказа приходит из PostgreSQL по-русски, и
    /// пока он попадал в `detail`, участник с локалью kk или en читал русскую
    /// строку с именем инварианта. Кириллица в `detail` отказа по правилу
    /// означает ровно одно - сообщение БД снова пробросили наружу.
    #[tokio::test]
    async fn rule_violation_does_not_leak_database_text() {
        let problem = problem_of(ApiError::RuleViolation(RuleRejection::classify(
            "INV-063: ставка 100 ниже минимально допустимой 105 (максимум 100 + шаг 5)",
        )))
        .await;

        assert_eq!(problem["status"], 409);
        assert_eq!(problem["code"], "rule_violation");
        assert_eq!(problem["rule"], "bid_below_minimum");
        assert!(
            problem.get("detail").is_none(),
            "деталь отказа по правилу уходит клиенту: {problem}"
        );
        assert!(
            !problem["rule"]
                .as_str()
                .unwrap_or_default()
                .chars()
                .any(|c| ('\u{0400}'..='\u{04FF}').contains(&c)),
            "причина отказа на проводе обязана быть машинной"
        );
    }

    /// Причина уезжает клиенту у каждого отказа перечня, а не только у тех,
    /// что нашлись по префиксу сообщения.
    #[tokio::test]
    async fn every_rule_of_the_catalogue_reaches_the_wire() {
        for rule in RuleViolation::ALL {
            let problem = problem_of(ApiError::rule(*rule, "внутреннее описание")).await;
            assert_eq!(problem["rule"], rule.as_str());
            assert!(problem.get("detail").is_none());
        }
    }

    /// Прочие ошибки деталь сохраняют: она написана приложением, а не БД,
    /// и для 422 это единственное объяснение, что именно не так с полем.
    #[tokio::test]
    async fn other_errors_keep_their_detail() {
        let problem = problem_of(ApiError::Validation("email не заполнен".to_owned())).await;
        assert_eq!(problem["detail"], "email не заполнен");
    }
}
