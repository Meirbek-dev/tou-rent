use std::error::Error;
use std::future::Future;

use serde::Serialize;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NotifyAdmittedCommand {
    pub actor_id: Uuid,
    pub tender_id: Uuid,
    pub notification_kind: &'static str,
    pub business_days_until_trading: i32,
}

/// Событие для конкретного получателя. `event` совпадает с публичной формой
/// уведомления, `recipient_id` используется только транспортом доставки.
#[derive(Debug, Clone)]
pub struct NotificationEnvelope {
    pub recipient_id: Uuid,
    pub event: NotificationEvent,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotificationEvent {
    pub id: Uuid,
    pub kind: String,
    pub payload: serde_json::Value,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub read_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone)]
pub struct NotifyAdmittedBatch {
    pub notifications: Vec<NotificationEnvelope>,
    pub trading_at: OffsetDateTime,
}

#[derive(Debug, thiserror::Error)]
pub enum NotifyAdmittedError {
    #[error("тендер не найден")]
    TenderNotFound,
    #[error("уведомление допущенных возможно только после протокола допуска")]
    AdmissionProtocolMissing,
    #[error("допущенные по тендеру уже уведомлены")]
    AlreadyNotified,
    #[error("по тендеру нет допущенных заявок")]
    NoAdmittedApplications,
    #[error("ошибка хранилища")]
    Infrastructure(#[source] Box<dyn Error + Send + Sync + 'static>),
}

impl NotifyAdmittedError {
    pub fn infrastructure(error: impl Error + Send + Sync + 'static) -> Self {
        Self::Infrastructure(Box::new(error))
    }
}

pub trait NotifyAdmittedStore: Sync {
    fn notify_admitted(
        &self,
        command: NotifyAdmittedCommand,
    ) -> impl Future<Output = Result<NotifyAdmittedBatch, NotifyAdmittedError>> + Send;
}

/// Эфемерная доставка уже зафиксированного в БД уведомления.
/// Ошибка транспорта не откатывает доказательную запись: клиент восстановит
/// пропущенное событие запросом истории.
pub trait NotificationPublisher: Sync {
    fn publish(&self, notification: &NotificationEnvelope);
}
