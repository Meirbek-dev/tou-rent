//! Изменение документации и отмена тендера (М3, FR-304, FR-305, FR-1004).
//!
//! Редакция документации - событие с следствиями: срок приема продлевается
//! (триггер БД), участники извещаются, а отказавшимся ставится срок возврата
//! взноса (п. 26.5). Отмена возможна до заключения договора и только с
//! основанием; взносы всех участников идут на возврат (п. 26.2).

use time::OffsetDateTime;
use tou_domain::amendment::{AmendmentFacts, CancellationFacts};
use tou_domain::obligation::ObligationAction;
use tou_domain::rule::{RuleRejection, RuleViolation};
use uuid::Uuid;

use crate::Db;
use crate::obligations::Subject;

#[derive(Debug, thiserror::Error)]
pub enum AmendmentError {
    #[error("тендер не найден")]
    NotFound,
    /// Правило п. 27 или п. 78 (домен и триггер БД)
    #[error("{0}")]
    Rejected(RuleRejection),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

impl From<tou_domain::amendment::AmendmentError> for AmendmentError {
    fn from(err: tou_domain::amendment::AmendmentError) -> Self {
        AmendmentError::Rejected(RuleRejection::new(
            RuleViolation::TenderDocumentationChange,
            err.to_string(),
        ))
    }
}

impl From<tou_domain::amendment::CancellationError> for AmendmentError {
    fn from(err: tou_domain::amendment::CancellationError) -> Self {
        AmendmentError::Rejected(RuleRejection::new(
            RuleViolation::TenderCancellation,
            err.to_string(),
        ))
    }
}

fn map_rule(err: sqlx::Error) -> AmendmentError {
    if let sqlx::Error::Database(db_err) = &err
        && matches!(
            db_err.code().as_deref(),
            Some("P0001") | Some("23514") | Some("23503") | Some("23505")
        )
    {
        return AmendmentError::Rejected(crate::rule::rejection(db_err.as_ref()));
    }
    AmendmentError::Db(err)
}

pub struct AmendmentRecord {
    pub id: Uuid,
    pub tender_id: Uuid,
    pub version: i32,
    pub summary: String,
    pub previous_deadline: Option<OffsetDateTime>,
    pub new_deadline: OffsetDateTime,
    pub doc_key: Option<String>,
    pub created_by_name: Option<String>,
    pub created_at: OffsetDateTime,
}

/// Выборка редакции: общий список столбцов + хвост запроса.
///
/// `created_by_name` приходит LEFT JOIN'ом: автор может быть не указан
/// (столбец `created_by` необязателен). Нужен явный `?` - sqlx выводит
/// nullability по самому столбцу (`core.users.full_name` - NOT NULL),
/// а не по виду соединения.
macro_rules! amendment_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            AmendmentRecord,
            r#"SELECT a.id, a.tender_id, a.version, a.summary,
                    a.previous_deadline, a.new_deadline, a.doc_key,
                    u.full_name AS "created_by_name?", a.created_at
             FROM core.tender_amendments a
             LEFT JOIN core.users u ON u.id = a.created_by"# + $tail
            $(, $arg)*
        )
    };
}

/// Редакции документации тендера, свежие сверху (баннер изменений, FR-304).
pub async fn list_for_tender(
    db: &Db,
    tender_id: Uuid,
) -> Result<Vec<AmendmentRecord>, sqlx::Error> {
    amendment_query!(" WHERE a.tender_id = $1 ORDER BY a.version DESC", tender_id)
        .fetch_all(db)
        .await
}

pub async fn get(db: &Db, id: Uuid) -> Result<Option<AmendmentRecord>, sqlx::Error> {
    amendment_query!(" WHERE a.id = $1", id)
        .fetch_optional(db)
        .await
}

/// Состояние тендера глазами п. 27: когда правка документации еще возможна.
pub async fn facts(db: &Db, tender_id: Uuid) -> Result<Option<AmendmentFacts>, sqlx::Error> {
    // `IS NOT NULL`, `IN (...)` и результат функции планировщик считает
    // потенциально NULL, хотя ни одно из них им не бывает
    let row = sqlx::query!(
        r#"SELECT submission_deadline,
                  opened_at IS NOT NULL AS "opened!",
                  status IN ('announced', 'accepting', 'repeat_announced') AS "published!",
                  core.now() AS "server_now!"
           FROM core.tenders WHERE id = $1"#,
        tender_id
    )
    .fetch_optional(db)
    .await?;

    let Some(row) = row else { return Ok(None) };

    Ok(Some(AmendmentFacts {
        now: to_timestamp(row.server_now),
        deadline: row.submission_deadline.map(to_timestamp),
        published: row.published,
        opened: row.opened,
    }))
}

/// Время БД в домен: сроки п. 27 считаются от времени сервера (NFR-03).
fn to_timestamp(value: OffsetDateTime) -> tou_domain::amendment::Instant {
    tou_domain::amendment::instant(value.unix_timestamp())
}

/// Публикация новой редакции документации (FR-304, п. 27). Правила окна
/// изменения проверяет домен, номер редакции, прежний срок и продление
/// дедлайна проставляет БД; здесь - извещение участников и его срок.
pub async fn amend(
    db: &Db,
    actor: Uuid,
    tender_id: Uuid,
    summary: &str,
    new_deadline: OffsetDateTime,
) -> Result<AmendmentRecord, AmendmentError> {
    let facts = facts(db, tender_id)
        .await?
        .ok_or(AmendmentError::NotFound)?;
    facts.check(to_timestamp(new_deadline))?;

    crate::with_actor(db, actor, async |tx| {
        let id = sqlx::query_scalar!(
            "INSERT INTO core.tender_amendments
               (tender_id, version, summary, new_deadline, created_by)
             VALUES ($1, 1, $2, $3, $4)
             RETURNING id",
            tender_id,
            summary.trim(),
            new_deadline,
            actor
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_rule)?
        .ok_or(AmendmentError::NotFound)?;

        // Извещение участников - обязанность организатора (п. 27); срок
        // закрывается рассылкой в этой же транзакции
        crate::obligations::schedule(
            &mut *tx,
            ObligationAction::NotifyAmendment,
            Subject::tender(tender_id),
        )
        .await?;

        amendment_query!(" WHERE a.id = $1", id)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_rule)
    })
    .await
}

/// Ключ печатной формы новой редакции (Прил. 1) в RustFS.
pub async fn attach_doc(db: &Db, actor: Uuid, id: Uuid, key: &str) -> Result<(), AmendmentError> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query!(
            "UPDATE core.tender_amendments SET doc_key = $2 WHERE id = $1",
            id,
            key
        )
        .execute(&mut *tx)
        .await
        .map(|_| ())
        .map_err(map_rule)
    })
    .await
}

/// Участники с «живыми» заявками тендера: получатели извещений (п. 27, 79).
pub async fn participants_of(db: &Db, tender_id: Uuid) -> Result<Vec<(Uuid, Uuid)>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT a.participant_id, a.id FROM core.applications a
         WHERE a.tender_id = $1 AND a.status <> 'withdrawn'
         ORDER BY a.submitted_at",
        tender_id
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.participant_id, r.id)).collect())
}

/// Закрытие срока извещения фактом рассылки (FR-1702).
pub async fn complete_notice(
    db: &Db,
    actor: Uuid,
    action: ObligationAction,
    tender_id: Uuid,
) -> Result<(), sqlx::Error> {
    crate::obligations::complete_tender(db, actor, &[action], tender_id).await
}

/// Отказ участника от участия из-за изменения условий (FR-1004, п. 26.5):
/// заявка отзывается со ссылкой на редакцию, взнос идет на возврат.
pub async fn decline_amendment(
    db: &Db,
    actor: Uuid,
    application_id: Uuid,
) -> Result<Uuid, AmendmentError> {
    crate::with_actor(db, actor, async |tx| {
        // Отказаться можно, пока есть неучтенная редакция и заявка «жива»
        let amendment = sqlx::query_scalar!(
            "SELECT a.id FROM core.tender_amendments a
             JOIN core.applications app ON app.tender_id = a.tender_id
             WHERE app.id = $1 AND app.participant_id = $2
               AND app.status IN ('submitted', 'fee_confirmed')
             ORDER BY a.version DESC LIMIT 1",
            application_id,
            actor
        )
        .fetch_optional(&mut *tx)
        .await?;

        let amendment_id = amendment.ok_or_else(|| {
            AmendmentError::Rejected(RuleRejection::new(
                RuleViolation::TenderDocumentationChange,
                "отказ по п. 26.5 возможен по своей действующей заявке тендера, \
                 условия которого изменены (FR-1004)",
            ))
        })?;

        let tender_id = sqlx::query_scalar!(
            "UPDATE core.applications
             SET status = 'withdrawn', withdrawn_at = core.now(), declined_amendment_id = $2
             WHERE id = $1 AND participant_id = $3
             RETURNING tender_id",
            application_id,
            amendment_id,
            actor
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_rule)?;
        let tender_id = tender_id.ok_or(AmendmentError::NotFound)?;

        sqlx::query!(
            "INSERT INTO core.journal_entries (tender_id, entry_kind, application_id, actor_id, note)
             VALUES ($1, 'application_withdrawn', $2, $3, 'отказ от участия: условия тендера изменены (п. 26.5)')",
            tender_id,
            application_id,
            actor
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;

        crate::applications::schedule_refund_if_paid(&mut *tx, application_id).await?;
        Ok(amendment_id)
    })
    .await
}

/// Отмена тендера (FR-305, п. 78–79): основание обязательно, отмена возможна
/// до заключения договора (стережет триггер БД). Взносы всех участников идут
/// на возврат, извещение - обязанность организатора со сроком п. 79.
pub async fn cancel_tender(
    db: &Db,
    actor: Uuid,
    tender_id: Uuid,
    reason: &str,
) -> Result<(), AmendmentError> {
    cancellation_facts(db, tender_id, None)
        .await?
        .ok_or(AmendmentError::NotFound)?
        .check(reason)?;

    crate::with_actor(db, actor, async |tx| {
        let updated = sqlx::query_scalar!(
            "UPDATE core.tenders
             SET status = 'cancelled', cancel_reason = $2, cancelled_at = core.now()
             WHERE id = $1 AND status <> 'cancelled'
             RETURNING id",
            tender_id,
            reason.trim()
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_rule)?;

        if updated.is_none() {
            return Err(AmendmentError::Rejected(RuleRejection::new(
                RuleViolation::TenderStatusTransition,
                "тендер уже отменен либо переход из текущего статуса запрещен (INV-021)",
            )));
        }

        schedule_refunds_of_tender(&mut *tx, tender_id).await?;

        crate::obligations::schedule(
            &mut *tx,
            ObligationAction::NotifyCancellation,
            Subject::tender(tender_id),
        )
        .await?;
        Ok(())
    })
    .await
}

/// Отмена отдельного лота (FR-305): тендер продолжается, объект лота
/// освобождается (FR-103), взносы по лоту идут на возврат.
pub async fn cancel_lot(
    db: &Db,
    actor: Uuid,
    lot_id: Uuid,
    reason: &str,
) -> Result<Uuid, AmendmentError> {
    cancellation_facts(db, Uuid::nil(), Some(lot_id))
        .await?
        .ok_or(AmendmentError::NotFound)?
        .check(reason)?;

    crate::with_actor(db, actor, async |tx| {
        let tender_id = sqlx::query_scalar!(
            "UPDATE core.lots
             SET cancelled_at = core.now(), cancel_reason = $2
             WHERE id = $1 AND cancelled_at IS NULL
             RETURNING tender_id",
            lot_id,
            reason.trim()
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_rule)?;

        let tender_id = tender_id.ok_or_else(|| {
            AmendmentError::Rejected(RuleRejection::new(
                RuleViolation::TenderCancellation,
                "лот не найден либо уже отменен (п. 78)",
            ))
        })?;

        let applications =
            sqlx::query_scalar!("SELECT id FROM core.applications WHERE lot_id = $1", lot_id)
                .fetch_all(&mut *tx)
                .await?;
        for application_id in applications {
            crate::applications::schedule_refund_if_paid(&mut *tx, application_id).await?;
        }

        crate::obligations::schedule(
            &mut *tx,
            ObligationAction::NotifyCancellation,
            Subject::tender(tender_id),
        )
        .await?;
        Ok(tender_id)
    })
    .await
}

/// Состояние предмета отмены глазами п. 78: заключен ли договор и не
/// отменено ли уже. Лот задается вместо тендера - правило у них одно.
pub async fn cancellation_facts(
    db: &Db,
    tender_id: Uuid,
    lot_id: Option<Uuid>,
) -> Result<Option<CancellationFacts>, sqlx::Error> {
    // Ветки дают разные анонимные типы строки, поэтому каждая сразу
    // складывается в доменные факты. `IS NOT NULL`, сравнение и `EXISTS`
    // не бывают NULL - отсюда `!`.
    let facts = match lot_id {
        Some(lot_id) => sqlx::query!(
            r#"SELECT l.cancelled_at IS NOT NULL AS "cancelled!",
                      EXISTS (SELECT 1 FROM core.contracts c
                              WHERE c.lot_id = l.id AND c.registered_at IS NOT NULL)
                        AS "contract_concluded!"
               FROM core.lots l WHERE l.id = $1"#,
            lot_id
        )
        .fetch_optional(db)
        .await?
        .map(|row| CancellationFacts {
            contract_concluded: row.contract_concluded,
            cancelled: row.cancelled,
        }),
        None => sqlx::query!(
            r#"SELECT t.status = 'cancelled' AS "cancelled!",
                      EXISTS (SELECT 1 FROM core.contracts c
                              WHERE c.tender_id = t.id AND c.registered_at IS NOT NULL)
                        AS "contract_concluded!"
               FROM core.tenders t WHERE t.id = $1"#,
            tender_id
        )
        .fetch_optional(db)
        .await?
        .map(|row| CancellationFacts {
            contract_concluded: row.contract_concluded,
            cancelled: row.cancelled,
        }),
    };

    Ok(facts)
}

/// Возврат взносов всех участников отмененного тендера (FR-1002, п. 26.2).
async fn schedule_refunds_of_tender(
    tx: &mut sqlx::PgConnection,
    tender_id: Uuid,
) -> Result<(), sqlx::Error> {
    let applications = sqlx::query_scalar!(
        "SELECT id FROM core.applications WHERE tender_id = $1",
        tender_id
    )
    .fetch_all(&mut *tx)
    .await?;
    for application_id in applications {
        crate::applications::schedule_refund_if_paid(&mut *tx, application_id).await?;
    }
    Ok(())
}
