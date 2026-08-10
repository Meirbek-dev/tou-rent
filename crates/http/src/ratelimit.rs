//! Ограничение частоты попыток на дологинных маршрутах (NFR-07).
//!
//! `/auth/login` и `/auth/register` - единственные мутации, открытые до
//! входа (их исключает и CSRF, см. [`crate::csrf`]). Перебор пароля по ним
//! ничем не ограничен: Argon2id делает попытку дорогой для сервера, но не
//! для атакующего, у которого их миллион.
//!
//! Счетчик живет в том же Redis, что и сессии: два экземпляра api (NFR-12)
//! обязаны считать попытки вместе, иначе лимит удваивается балансировкой.
//!
//! Ключ - не сам email, а его отпечаток: Redis не место для персональных
//! данных (NFR-16), а для счетчика важно только различать обращения.
//!
//! Считаются **неудачные** попытки, а удачная счетчик обнуляет. Иначе
//! ограничение бьет не по перебору, а по работе: у входа нет причин быть
//! однократным - вторая вкладка, второе устройство, истекшая сессия, - и
//! счетчик «любых попыток» запирал бы человека, который каждый раз вводил
//! верный пароль. Так и вышло при первом же сквозном прогоне.
//!
//! Сбой Redis не запирает вход: счетчик - защита, а не правило домена,
//! и превращать его отказ в отказ системы нельзя. Промах пишется в лог.

use fred::prelude::KeysInterface as _;
use sha2::{Digest as _, Sha256};

use crate::error::ApiError;
use crate::realtime::Pool;

/// Сколько попыток за какое окно. Окно фиксированное: скользящее точнее,
/// но требует хранить отметки каждой попытки - для защиты от перебора
/// разница несущественная.
#[derive(Debug, Clone, Copy)]
pub struct Limit {
    pub attempts: u32,
    pub window_seconds: i64,
}

/// Перебор пароля конкретной учетной записи: десять **неудачных** попыток
/// за четверть часа. Вчетверо больше, чем нужно человеку, опечатавшемуся
/// в раскладке, и на порядки меньше, чем нужно словарю.
pub const LOGIN_PER_ACCOUNT: Limit = Limit {
    attempts: 10,
    window_seconds: 900,
};

/// Перебор по многим учетным записям с одного адреса.
///
/// Порог заметно выше, чем у учетной записи, и это не небрежность:
/// университет выходит в сеть через общий адрес, поэтому за одним IP стоит
/// весь кампус. Сотня неудач за четверть часа - это уже не «утро понедельника
/// с забытыми паролями», а перебор, но кампус в такой порог укладывается.
pub const LOGIN_PER_ADDRESS: Limit = Limit {
    attempts: 100,
    window_seconds: 900,
};

/// Массовая регистрация (FR-1504). Считаются заведенные учетные записи,
/// а не попытки: ограничивать нужно создание, а не отказ валидации у живого
/// человека. Десять за час с одного адреса - больше, чем бывает у людей,
/// и мало для бота.
pub const REGISTER_PER_ADDRESS: Limit = Limit {
    attempts: 10,
    window_seconds: 3600,
};

/// Счетчик попыток. `None` - Redis не подключен (тесты, `api openapi`):
/// проверка становится пустой операцией.
#[derive(Clone, Default)]
pub struct RateLimiter {
    pool: Option<Pool>,
}

impl RateLimiter {
    pub fn new(pool: Pool) -> Self {
        Self { pool: Some(pool) }
    }

    /// Отказать, если лимит уже исчерпан. Сама проверка ничего не считает:
    /// попытку засчитывает [`RateLimiter::record`], и только ту, которая
    /// того заслуживает.
    ///
    /// `bucket` разделяет счетчики разных проверок, `subject` - обращения
    /// внутри одной (учетная запись, адрес).
    pub async fn check(&self, bucket: &str, subject: &str, limit: Limit) -> Result<(), ApiError> {
        let Some(pool) = self.pool.as_ref() else {
            return Ok(());
        };
        let key = key_of(bucket, subject);

        let count: Option<i64> = match pool.get(&key).await {
            Ok(count) => count,
            Err(error) => {
                tracing::warn!(%error, bucket, "счетчик попыток недоступен - проверка пропущена");
                return Ok(());
            }
        };

        if count.unwrap_or(0) >= i64::from(limit.attempts) {
            let ttl: i64 = pool.ttl(&key).await.unwrap_or(limit.window_seconds);
            tracing::warn!(bucket, "лимит попыток исчерпан");
            return Err(ApiError::TooManyRequests {
                retry_after_seconds: ttl.clamp(1, limit.window_seconds).unsigned_abs(),
            });
        }

        Ok(())
    }

    /// Засчитать попытку: неудачный вход или заведенную учетную запись.
    pub async fn record(&self, bucket: &str, subject: &str, limit: Limit) {
        let Some(pool) = self.pool.as_ref() else {
            return;
        };
        let key = key_of(bucket, subject);

        let count: i64 = match pool.incr(&key).await {
            Ok(count) => count,
            Err(error) => {
                tracing::warn!(%error, bucket, "счетчик попыток недоступен - попытка не учтена");
                return;
            }
        };

        // Срок ставится на первой попытке окна: продление на каждой
        // превратило бы окно в вечную блокировку
        if count == 1
            && let Err(error) = pool.expire::<(), _>(&key, limit.window_seconds, None).await
        {
            // Ключ без срока запер бы учетную запись навсегда - лучше снять
            tracing::warn!(%error, bucket, "срок счетчика не поставлен - счетчик сбрасывается");
            let _: Result<(), _> = pool.del(&key).await;
        }
    }

    /// Обнулить счетчик: удачный вход снимает подозрение с учетной записи.
    pub async fn forget(&self, bucket: &str, subject: &str) {
        let Some(pool) = self.pool.as_ref() else {
            return;
        };
        if let Err(error) = pool.del::<(), _>(key_of(bucket, subject)).await {
            tracing::warn!(%error, bucket, "счетчик попыток не сброшен");
        }
    }
}

fn key_of(bucket: &str, subject: &str) -> String {
    format!("tou:rl:{bucket}:{}", fingerprint(subject))
}

/// Отпечаток обращения: в Redis уходит он, а не email или адрес (NFR-16).
fn fingerprint(subject: &str) -> String {
    let digest = Sha256::digest(subject.as_bytes());
    // Половины хеша хватает: коллизия здесь стоит одного лишнего счетчика
    digest[..16].iter().fold(String::new(), |mut acc, byte| {
        use std::fmt::Write as _;
        let _ = write!(acc, "{byte:02x}");
        acc
    })
}

/// Адрес клиента для прода: единственная точка входа - Caddy, он и ставит
/// `X-Forwarded-For`. Берется первый элемент - его пишет сам прокси.
///
/// Заголовок подделывается кем угодно, поэтому на нем висит только
/// вспомогательный лимит по адресу; основной - по учетной записи, он
/// заголовков не требует. Без заголовка (прямое обращение на дев-стенде)
/// проверки по адресу нет: общая корзина на всех превратила бы лимит
/// в самоблокировку.
pub fn client_address(headers: &axum::http::HeaderMap) -> Option<String> {
    let raw = headers.get("x-forwarded-for")?.to_str().ok()?;
    let first = raw.split(',').next()?.trim();
    (!first.is_empty()).then(|| first.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{RateLimiter, client_address, fingerprint};
    use axum::http::HeaderMap;

    #[test]
    fn fingerprint_hides_subject_and_is_stable() {
        let subject = "participant@tou.edu.kz";
        let digest = fingerprint(subject);

        assert_eq!(digest, fingerprint(subject));
        assert_ne!(digest, fingerprint("other@tou.edu.kz"));
        assert!(
            !digest.contains("tou.edu.kz"),
            "email не должен попасть в ключ"
        );
        assert_eq!(digest.len(), 32, "16 байт в hex");
    }

    #[test]
    fn address_comes_from_first_forwarded_hop() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.7, 10.0.0.1".parse().unwrap());
        assert_eq!(client_address(&headers).as_deref(), Some("203.0.113.7"));
    }

    #[test]
    fn address_is_absent_without_proxy() {
        assert_eq!(client_address(&HeaderMap::new()), None);
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "  ".parse().unwrap());
        assert_eq!(client_address(&headers), None);
    }

    /// Без Redis (тесты контракта, `api openapi`) проверка не должна ни
    /// падать, ни блокировать.
    #[tokio::test]
    async fn detached_limiter_allows_everything() {
        let limiter = RateLimiter::default();
        for _ in 0..100 {
            assert!(
                limiter
                    .check("login:account", "a@b.kz", super::LOGIN_PER_ACCOUNT)
                    .await
                    .is_ok()
            );
            limiter
                .record("login:account", "a@b.kz", super::LOGIN_PER_ACCOUNT)
                .await;
            limiter.forget("login:account", "a@b.kz").await;
        }
    }

    /// Порог по адресу обязан быть заметно выше, чем по учетной записи:
    /// за одним IP стоит весь кампус, и уравнивать их значит запирать
    /// университет из-за десятка забытых паролей.
    #[test]
    fn address_threshold_leaves_room_for_a_shared_egress() {
        const {
            assert!(
                super::LOGIN_PER_ADDRESS.attempts >= super::LOGIN_PER_ACCOUNT.attempts * 5,
                "порог по адресу не оставляет запаса под общий выход в сеть"
            )
        };
    }
}
