use tou_domain::notification::NotificationKind;
use tou_ports::notifications::{
    NotificationPublisher, NotifyAdmittedBatch, NotifyAdmittedCommand, NotifyAdmittedError,
    NotifyAdmittedStore,
};
use uuid::Uuid;

pub struct NotifyAdmitted<'a, Store, Publisher> {
    store: Store,
    publisher: &'a Publisher,
}

impl<'a, Store, Publisher> NotifyAdmitted<'a, Store, Publisher>
where
    Store: NotifyAdmittedStore,
    Publisher: NotificationPublisher,
{
    pub fn new(store: Store, publisher: &'a Publisher) -> Self {
        Self { store, publisher }
    }

    pub async fn execute(
        &self,
        actor_id: Uuid,
        tender_id: Uuid,
    ) -> Result<NotifyAdmittedOutput, NotifyAdmittedError> {
        let batch = self
            .store
            .notify_admitted(NotifyAdmittedCommand {
                actor_id,
                tender_id,
                notification_kind: NotificationKind::AuctionInvitation.as_str(),
                business_days_until_trading: 3,
            })
            .await?;

        self.publish(&batch);

        Ok(NotifyAdmittedOutput {
            notified: batch.notifications.len(),
            trading_at: batch.trading_at,
        })
    }

    fn publish(&self, batch: &NotifyAdmittedBatch) {
        for notification in &batch.notifications {
            self.publisher.publish(notification);
        }
    }
}

pub struct NotifyAdmittedOutput {
    pub notified: usize,
    pub trading_at: time::OffsetDateTime,
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use serde_json::json;
    use time::OffsetDateTime;
    use tou_ports::notifications::{
        NotificationEnvelope, NotificationEvent, NotificationPublisher, NotifyAdmittedBatch,
        NotifyAdmittedCommand, NotifyAdmittedError, NotifyAdmittedStore,
    };

    use super::*;

    struct FakeStore {
        batch: NotifyAdmittedBatch,
        commands: Mutex<Vec<NotifyAdmittedCommand>>,
    }

    impl NotifyAdmittedStore for FakeStore {
        async fn notify_admitted(
            &self,
            command: NotifyAdmittedCommand,
        ) -> Result<NotifyAdmittedBatch, NotifyAdmittedError> {
            let batch = self.batch.clone();
            match self.commands.lock() {
                Ok(mut commands) => {
                    commands.push(command);
                    Ok(batch)
                }
                Err(_) => Err(NotifyAdmittedError::infrastructure(std::io::Error::other(
                    "commands lock poisoned",
                ))),
            }
        }
    }

    #[derive(Default)]
    struct FakePublisher {
        recipients: Mutex<Vec<Uuid>>,
    }

    impl NotificationPublisher for FakePublisher {
        fn publish(&self, notification: &NotificationEnvelope) {
            if let Ok(mut recipients) = self.recipients.lock() {
                recipients.push(notification.recipient_id);
            }
        }
    }

    #[tokio::test]
    async fn persists_once_then_publishes_every_notification() {
        let trading_at: OffsetDateTime = time::macros::datetime!(2026-08-12 05:00 UTC);
        let recipient_id = Uuid::now_v7();
        let store = FakeStore {
            batch: NotifyAdmittedBatch {
                notifications: vec![NotificationEnvelope {
                    recipient_id,
                    event: NotificationEvent {
                        id: Uuid::now_v7(),
                        kind: "auction_invitation".into(),
                        payload: json!({ "tender_id": Uuid::now_v7() }),
                        created_at: trading_at,
                        read_at: None,
                    },
                }],
                trading_at,
            },
            commands: Mutex::new(Vec::new()),
        };
        let publisher = FakePublisher::default();
        let use_case = NotifyAdmitted::new(store, &publisher);

        let output = use_case
            .execute(Uuid::now_v7(), Uuid::now_v7())
            .await
            .expect("use case succeeds");

        assert_eq!(output.notified, 1);
        assert_eq!(output.trading_at, trading_at);
        assert_eq!(
            publisher.recipients.lock().expect("lock").as_slice(),
            [recipient_id]
        );
    }
}
