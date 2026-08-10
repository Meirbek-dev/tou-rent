//! Потолок времени обработки запроса (NFR-02).
//!
//! Без него зависший запрос держит задачу runtime и соединение из пула
//! бесконечно: один заблокированный `SELECT ... FOR UPDATE` в комнате торгов
//! или недоступный RustFS при выгрузке досье выедают пул, и отказывать
//! начинает вся система, а не одна операция.
//!
//! Долгоживущие маршруты исключены поименно: SSE-стрим уведомлений
//! (FR-1301) и WS-комната торгов (FR-603) держат соединение открытым
//! часами - это их нормальная работа, а не зависание.

use std::time::Duration;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

use crate::error::ApiError;

/// Потолок на обычный REST-запрос. Самая тяжелая операция контура - сборка
/// архива досье (FR-1602) и рендер Typst-PDF; они укладываются в единицы
/// секунд, поэтому 30 с - это «что-то сломалось», а не «долго считает».
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Маршруты, которым держать соединение открытым положено по контракту.
///
/// Сверка идет по пути, а не по признаку апгрейда: заголовки к моменту
/// работы middleware уже разобраны, а путь - то же самое, что записано
/// в реестре маршрутов (`lib.rs`).
fn is_long_lived(path: &str) -> bool {
    path == "/api/v1/notifications/stream"
        || (path.starts_with("/api/v1/auctions/") && path.ends_with("/ws"))
}

pub async fn enforce(request: Request, next: Next) -> Result<Response, ApiError> {
    if is_long_lived(request.uri().path()) {
        return Ok(next.run(request).await);
    }

    match tokio::time::timeout(REQUEST_TIMEOUT, next.run(request)).await {
        Ok(response) => Ok(response),
        Err(_) => Err(ApiError::Timeout),
    }
}

#[cfg(test)]
mod tests {
    use super::is_long_lived;

    #[test]
    fn streaming_routes_are_exempt() {
        assert!(is_long_lived("/api/v1/notifications/stream"));
        assert!(is_long_lived(
            "/api/v1/auctions/0198f0d5-0000-7000-8000-000000000000/ws"
        ));
    }

    /// Совпадение по префиксу не должно освобождать обычные маршруты комнаты:
    /// снимок, ставки и продление - те же 30 секунд, что у всех.
    #[test]
    fn ordinary_routes_are_limited() {
        for path in [
            "/api/v1/tenders",
            "/api/v1/notifications",
            "/api/v1/auctions/0198f0d5-0000-7000-8000-000000000000",
            "/api/v1/auctions/0198f0d5-0000-7000-8000-000000000000/bids",
        ] {
            assert!(!is_long_lived(path), "{path} не должен быть исключен");
        }
    }
}
