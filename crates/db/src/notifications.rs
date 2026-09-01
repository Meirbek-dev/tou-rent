//! Центр уведомлений (М13, FR-1301–1302).
//!
//! Запись всегда попадает в `core.notifications` (канал `in_app`, контур 1);
//! факт и время создания с получателем фиксирует audit-триггер INV-AUDIT -
//! доказательная база процессуальных уведомлений (FR-1302, аналог п. 58).
//! SSE-доставка живет в слое http; здесь - только персистентность.

use std::collections::HashMap;

use rust_decimal::Decimal;
use serde_json::json;
use time::OffsetDateTime;
use tou_ports::notifications::{
    NotificationEnvelope, NotificationEvent, NotifyAdmittedBatch, NotifyAdmittedCommand,
    NotifyAdmittedError, NotifyAdmittedStore,
};
use uuid::Uuid;

use crate::Db;

pub struct NotificationRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    /// Тип события - `domain::notification::NotificationKind` на проводе
    pub kind: String,
    pub payload: serde_json::Value,
    pub created_at: OffsetDateTime,
    pub read_at: Option<OffsetDateTime>,
}

/// Выборка уведомления: общий список столбцов + хвост запроса (см. `acts.rs`).
macro_rules! notification_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            NotificationRecord,
            "SELECT id, user_id, kind, payload, created_at, read_at
             FROM core.notifications" + $tail
            $(, $arg)*
        )
    };
}
/// То же для `RETURNING`: столбцы идут в конце запроса (см. `identities.rs`).
macro_rules! notification_query_returning {
    ($head:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            NotificationRecord,
            $head + " RETURNING id, user_id, kind, payload, created_at, read_at"
            $(, $arg)*
        )
    };
}

pub struct NewNotification {
    pub user_id: Uuid,
    pub payload: serde_json::Value,
}

/// Пакет уведомлений одного события - одна транзакция `with_actor`:
/// либо уведомлены все получатели (и каждый факт в аудите), либо никто.
pub async fn insert(
    db: &Db,
    actor: Uuid,
    kind: &str,
    items: &[NewNotification],
) -> Result<Vec<NotificationRecord>, sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        let mut created = Vec::with_capacity(items.len());
        for item in items {
            let record = notification_query_returning!(
                "INSERT INTO core.notifications (user_id, kind, payload)
                 VALUES ($1, $2, $3)",
                item.user_id,
                kind,
                item.payload
            )
            .fetch_one(&mut *tx)
            .await?;
            created.push(record);
        }
        Ok(created)
    })
    .await
}

/// История уведомлений получателя, новые сверху. Курсор `after` - id
/// последней показанной записи (uuid v7 монотонен по времени создания).
pub async fn list_for_user(
    db: &Db,
    user_id: Uuid,
    after: Option<Uuid>,
    limit: i64,
) -> Result<Vec<NotificationRecord>, sqlx::Error> {
    notification_query!(
        " WHERE user_id = $1 AND ($2::uuid IS NULL OR id < $2)
          ORDER BY id DESC LIMIT $3",
        user_id,
        after,
        limit
    )
    .fetch_all(db)
    .await
}

/// Счетчик непрочитанных для колокольчика (FR-1301) - частичный индекс
/// `notifications_unread_idx`.
pub async fn unread_count(db: &Db, user_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM core.notifications
           WHERE user_id = $1 AND read_at IS NULL"#,
        user_id
    )
    .fetch_one(db)
    .await
}

/// Отметка о прочтении своих уведомлений; `ids = None` - все непрочитанные.
/// Идет через `with_actor`: notifications в перечне INV-AUDIT, актор
/// изменения обязан попасть в аудит.
pub async fn mark_read(db: &Db, actor: Uuid, ids: Option<&[Uuid]>) -> Result<u64, sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        let result = sqlx::query!(
            "UPDATE core.notifications SET read_at = core.now()
             WHERE user_id = $1 AND read_at IS NULL
               AND ($2::uuid[] IS NULL OR id = ANY($2))",
            actor,
            ids
        )
        .execute(&mut *tx)
        .await?;
        Ok(result.rows_affected())
    })
    .await
}

/// Было ли событие такого вида уже разослано по тендеру - однократность
/// процессуального уведомления (FR-504: повторная рассылка запрещена).
pub async fn tender_notified(db: &Db, tender_id: Uuid, kind: &str) -> Result<bool, sqlx::Error> {
    // `$1::uuid::text`: без первого приведения планировщик выводит для
    // параметра тип text, а сюда приходит Uuid
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
             SELECT 1 FROM core.notifications
             WHERE kind = $2 AND payload->>'tender_id' = $1::uuid::text
           ) AS "exists!""#,
        tender_id,
        kind
    )
    .fetch_one(db)
    .await
}

/// PostgreSQL-адаптер атомарного сценария FR-504. Все проверки, назначение
/// времени торгов и доказательные уведомления выполняются в одной транзакции.
pub struct PgNotifyAdmittedStore<'a> {
    db: &'a Db,
}

impl<'a> PgNotifyAdmittedStore<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }
}

#[derive(Debug, thiserror::Error)]
enum AtomicNotifyError {
    #[error("тендер не найден")]
    TenderNotFound,
    #[error("нет протокола допуска")]
    AdmissionProtocolMissing,
    #[error("уведомления уже созданы")]
    AlreadyNotified,
    #[error("нет допущенных заявок")]
    NoAdmittedApplications,
    #[error("неполные данные лота {0}")]
    IncompleteLot(Uuid),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error("форматирование времени: {0}")]
    Time(String),
}

impl NotifyAdmittedStore for PgNotifyAdmittedStore<'_> {
    async fn notify_admitted(
        &self,
        command: NotifyAdmittedCommand,
    ) -> Result<NotifyAdmittedBatch, NotifyAdmittedError> {
        let result = crate::with_actor(self.db, command.actor_id, async |tx| {
            // Сериализуем повторные запросы по одному тендеру. В отличие от
            // check-then-insert в HTTP два конкурентных запроса не создадут дубли.
            // `pg_advisory_xact_lock` возвращает столбец, поэтому не
            // `execute`, а `fetch_one` с отбрасыванием результата
            sqlx::query!(
                "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
                format!("notify-admitted:{}", command.tender_id)
            )
            .fetch_one(&mut *tx)
            .await?;

            let tender = sqlx::query!(
                "SELECT title, zoom_url, trading_at FROM core.tenders WHERE id = $1",
                command.tender_id
            )
            .fetch_optional(&mut *tx)
            .await?;
            let Some(tender) = tender else {
                return Err(AtomicNotifyError::TenderNotFound);
            };
            let (tender_title, zoom_url, current_trading_at) =
                (tender.title, tender.zoom_url, tender.trading_at);

            let has_protocol = sqlx::query_scalar!(
                r#"SELECT EXISTS (
                       SELECT 1 FROM core.protocols
                       WHERE tender_id = $1 AND kind = 'admission'
                     ) AS "exists!""#,
                command.tender_id
            )
            .fetch_one(&mut *tx)
            .await?;
            if !has_protocol {
                return Err(AtomicNotifyError::AdmissionProtocolMissing);
            }

            let already_notified = sqlx::query_scalar!(
                r#"SELECT EXISTS (
                       SELECT 1 FROM core.notifications
                       WHERE kind = $2 AND payload->>'tender_id' = $1::uuid::text
                     ) AS "exists!""#,
                command.tender_id,
                command.notification_kind
            )
            .fetch_one(&mut *tx)
            .await?;
            if already_notified {
                return Err(AtomicNotifyError::AlreadyNotified);
            }

            // `!` у `amount`: результат функции планировщик считает
            // потенциально NULL, но цена допущенной заявки есть всегда
            let admitted: Vec<(Uuid, Uuid, Uuid, i32, String, Decimal)> = sqlx::query!(
                r#"SELECT a.id, a.participant_id, a.lot_id, l.seq, l.purpose,
                          core.price_amount(p) AS "amount!"
                     FROM core.applications a
                     JOIN core.lots l ON l.id = a.lot_id
                     JOIN core.price_proposals p ON p.application_id = a.id
                     WHERE a.tender_id = $1 AND a.status = 'admitted'
                     ORDER BY a.submitted_at"#,
                command.tender_id
            )
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|r| (r.id, r.participant_id, r.lot_id, r.seq, r.purpose, r.amount))
            .collect();
            if admitted.is_empty() {
                return Err(AtomicNotifyError::NoAdmittedApplications);
            }

            let mut starting_bids = HashMap::<Uuid, Decimal>::new();
            for (_, _, lot_id, _, _, price) in &admitted {
                starting_bids
                    .entry(*lot_id)
                    .and_modify(|maximum| *maximum = (*maximum).max(*price))
                    .or_insert(*price);
            }

            // Юридический день определяется явно в Asia/Almaty, независимо
            // от timezone соединения PostgreSQL.
            let trading_at = match current_trading_at {
                Some(value) => value,
                None => {
                    // `!` у RETURNING: столбец nullable, но присвоенное
                    // выражение NULL быть не может - оба аргумента NOT NULL
                    sqlx::query_scalar!(
                        r#"UPDATE core.tenders
                           SET trading_at = (
                             refdata.add_business_days(
                               (core.now() AT TIME ZONE 'Asia/Almaty')::date,
                               $2
                             )::timestamp + time '10:00'
                           ) AT TIME ZONE 'Asia/Almaty'
                           WHERE id = $1
                           RETURNING trading_at AS "trading_at!""#,
                        command.tender_id,
                        command.business_days_until_trading
                    )
                    .fetch_one(&mut *tx)
                    .await?
                }
            };
            let trading_at_wire = trading_at
                .format(&time::format_description::well_known::Rfc3339)
                .map_err(|error| AtomicNotifyError::Time(error.to_string()))?;

            let mut notifications = Vec::with_capacity(admitted.len());
            for (application_id, participant_id, lot_id, lot_seq, purpose, _) in admitted {
                let starting_bid = starting_bids
                    .get(&lot_id)
                    .ok_or(AtomicNotifyError::IncompleteLot(lot_id))?;
                let payload = json!({
                    "tender_id": command.tender_id,
                    "tender_title": tender_title.clone(),
                    "lot_id": lot_id,
                    "lot": format!("№{lot_seq} - {purpose}"),
                    "application_id": application_id,
                    "starting_bid": starting_bid.to_string(),
                    "trading_at": trading_at_wire.clone(),
                    "place": zoom_url.clone(),
                });
                let record = notification_query_returning!(
                    "INSERT INTO core.notifications (user_id, kind, payload)
                     VALUES ($1, $2, $3)",
                    participant_id,
                    command.notification_kind,
                    payload
                )
                .fetch_one(&mut *tx)
                .await?;

                notifications.push(NotificationEnvelope {
                    recipient_id: record.user_id,
                    event: NotificationEvent {
                        id: record.id,
                        kind: record.kind,
                        payload: record.payload,
                        created_at: record.created_at,
                        read_at: record.read_at,
                    },
                });
            }

            Ok(NotifyAdmittedBatch {
                notifications,
                trading_at,
            })
        })
        .await;

        result.map_err(|error| match error {
            AtomicNotifyError::TenderNotFound => NotifyAdmittedError::TenderNotFound,
            AtomicNotifyError::AdmissionProtocolMissing => {
                NotifyAdmittedError::AdmissionProtocolMissing
            }
            AtomicNotifyError::AlreadyNotified => NotifyAdmittedError::AlreadyNotified,
            AtomicNotifyError::NoAdmittedApplications => {
                NotifyAdmittedError::NoAdmittedApplications
            }
            other => NotifyAdmittedError::infrastructure(other),
        })
    }
}
