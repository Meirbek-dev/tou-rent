//! Вскрытие и допуск (М5, FR-501–503).
//!
//! Вскрытие - переход `accepting → qualification` (законность - триггер
//! INV-021) с `opened_at = core.now()`: CHECK БД не даст вскрыть раньше времени
//! заседания (`opening_at`), audit-событие пишет триггер INV-AUDIT (FR-403).
//! Решение по заявке и голоса членов (вносит секретарь, FR-503/контур 1) -
//! одна транзакция; основание отклонения - FK на закрытый перечень (INV-052).

use time::OffsetDateTime;
use tou_domain::rule::{RuleRejection, RuleViolation};
use uuid::Uuid;

use crate::Db;
use crate::applications::ApplicationRecord;
use crate::tenders::TenderRecord;
use tou_domain::obligation::ObligationAction;

pub struct MeetingRecord {
    pub id: Uuid,
    pub tender_id: Uuid,
    pub commission_id: Uuid,
    pub commission_name: String,
    pub scheduled_at: OffsetDateTime,
    pub held_at: Option<OffsetDateTime>,
    /// Заседание открыто при кворуме (FR-1102); до этого решений нет
    pub opened_at: Option<OffsetDateTime>,
    pub quorum_present: Option<i32>,
    pub quorum_required: Option<i32>,
}

/// Выборка заседания вместе с названием комиссии: общий список столбцов
/// + хвост запроса.
macro_rules! meeting_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            MeetingRecord,
            "SELECT m.id, m.tender_id, m.commission_id, c.name AS commission_name,
                    m.scheduled_at, m.held_at, m.opened_at,
                    m.quorum_present, m.quorum_required
             FROM core.sessions_meetings m
             JOIN core.commissions c ON c.id = m.commission_id" + $tail
            $(, $arg)*
        )
    };
}

pub struct MemberRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub full_name: String,
    pub member_role: String,
}

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("тендер не найден")]
    NotFound,
    /// Отказ БД: переход (INV-021), время вскрытия (CHECK) или неоткрытое
    /// заседание комиссии (FR-1102)
    #[error("{0}")]
    Rejected(RuleRejection),
    /// Нет действующей утвержденной комиссии - заседание невозможно
    #[error("нет действующей тендерной комиссии")]
    NoCommission,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Заседание допуска по тендеру: создается при подготовке (отметка явки),
/// ведет его действующая комиссия с утвержденным составом (FR-1101).
/// Повторный вызов возвращает уже созданное заседание.
pub async fn ensure_qualification_meeting(
    db: &Db,
    actor: Uuid,
    tender_id: Uuid,
) -> Result<MeetingRecord, OpenError> {
    crate::with_actor(db, actor, async |tx| {
        let existing = meeting_query!(
            " WHERE m.tender_id = $1 AND m.kind = 'qualification'",
            tender_id
        )
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(meeting) = existing {
            return Ok(meeting);
        }

        let tender_exists = sqlx::query_scalar!(
            r#"SELECT true AS "exists!" FROM core.tenders WHERE id = $1"#,
            tender_id
        )
        .fetch_optional(&mut *tx)
        .await?;
        if tender_exists.is_none() {
            return Err(OpenError::NotFound);
        }

        // Только утвержденный состав (FR-1101) и действующие полномочия (п. 9–11)
        let commission_id = sqlx::query_scalar!(
            "SELECT id FROM core.commissions
             WHERE valid_from <= (core.now() AT TIME ZONE 'Asia/Almaty')::date
               AND (core.now() AT TIME ZONE 'Asia/Almaty')::date < valid_until
               AND approved_at IS NOT NULL
             ORDER BY approved_at DESC LIMIT 1"
        )
        .fetch_optional(&mut *tx)
        .await?;
        let commission_id = commission_id.ok_or(OpenError::NoCommission)?;

        sqlx::query!(
            "INSERT INTO core.sessions_meetings (tender_id, commission_id, kind, scheduled_at)
             SELECT $1, $2, 'qualification', COALESCE(t.opening_at, core.now())
             FROM core.tenders t WHERE t.id = $1",
            tender_id,
            commission_id
        )
        .execute(&mut *tx)
        .await?;

        let meeting = meeting_query!(
            " WHERE m.tender_id = $1 AND m.kind = 'qualification'",
            tender_id
        )
        .fetch_one(&mut *tx)
        .await?;
        Ok(meeting)
    })
    .await
}

/// Вскрытие конвертов (FR-501): переход в `qualification` и фиксация
/// `opened_at`. Проводится на открытом заседании комиссии - без кворума
/// вскрытие отклонит триггер БД (FR-1102, п. 12, 50).
pub async fn open_tender(
    db: &Db,
    actor: Uuid,
    tender_id: Uuid,
) -> Result<(TenderRecord, MeetingRecord), OpenError> {
    crate::with_actor(db, actor, async |tx| {
        let updated = sqlx::query_as!(
            TenderRecord,
            r#"UPDATE core.tenders
               SET status = 'qualification', opened_at = core.now()
               WHERE id = $1 AND status = 'accepting'
               RETURNING id, status::text AS "status!", title, title_kk, organizer_id,
                         announced_at, submission_deadline, opening_at, opened_at,
                         trading_at, zoom_url, zoom_recording_url, repeat_of,
                         created_at, updated_at"#,
            tender_id
        )
        .fetch_optional(&mut *tx)
        .await;

        let tender = match updated {
            Ok(Some(record)) => record,
            Ok(None) => {
                let exists = sqlx::query_scalar!(
                    r#"SELECT status::text AS "status!" FROM core.tenders WHERE id = $1"#,
                    tender_id
                )
                .fetch_optional(&mut *tx)
                .await?;
                return Err(match exists {
                    Some(current) => OpenError::Rejected(RuleRejection::new(
                        RuleViolation::StatusNotAllowed,
                        format!("вскрытие возможно только в статусе accepting (сейчас {current})"),
                    )),
                    None => OpenError::NotFound,
                });
            }
            // 23514 - CHECK времени вскрытия; P0001 - правило заседания (FR-1102)
            Err(sqlx::Error::Database(db_err))
                if matches!(db_err.code().as_deref(), Some("23514") | Some("P0001")) =>
            {
                return Err(OpenError::Rejected(crate::rule::rejection(db_err.as_ref())));
            }
            Err(other) => return Err(OpenError::Db(other)),
        };

        let meeting = meeting_query!(
            " WHERE m.tender_id = $1 AND m.kind = 'qualification'",
            tender_id
        )
        .fetch_one(&mut *tx)
        .await?;

        Ok((tender, meeting))
    })
    .await
}

/// Заседание допуска по тендеру (если оно уже создано).
pub async fn qualification_meeting(
    db: &Db,
    tender_id: Uuid,
) -> Result<Option<MeetingRecord>, sqlx::Error> {
    meeting_query!(
        " WHERE m.tender_id = $1 AND m.kind = 'qualification'",
        tender_id
    )
    .fetch_optional(db)
    .await
}

pub async fn members_of(db: &Db, commission_id: Uuid) -> Result<Vec<MemberRecord>, sqlx::Error> {
    sqlx::query_as!(
        MemberRecord,
        r#"SELECT cm.id, cm.user_id, u.full_name,
                  cm.member_role::text AS "member_role!"
           FROM core.commission_members cm
           JOIN core.users u ON u.id = cm.user_id
           WHERE cm.commission_id = $1
           ORDER BY cm.member_role, u.full_name"#,
        commission_id
    )
    .fetch_all(db)
    .await
}

#[derive(Debug, thiserror::Error)]
pub enum DecideError {
    /// Заявка не найдена, не в решаемом статусе или тендер не на допуске
    #[error("решение по заявке сейчас невозможно")]
    NotDecidable,
    /// FK/CHECK БД: неизвестное основание (INV-052), чужой член комиссии и т.п.
    #[error("{0}")]
    Rejected(RuleRejection),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Фиксация решения по заявке (FR-502). Сам вердикт - итог голосования
/// членов комиссии (FR-1103), а не выбор секретаря: он вычисляется в домене
/// по голосам и передается сюда. При отклонении обязателен код основания
/// из `refdata.rejection_reasons` (INV-052 - стережет FK).
pub async fn decide(
    db: &Db,
    actor: Uuid,
    application_id: Uuid,
    verdict: &str,
    rejection_reason: Option<&str>,
) -> Result<ApplicationRecord, DecideError> {
    crate::with_actor(db, actor, async |tx| {
        let map_db = |err: sqlx::Error| {
            if let sqlx::Error::Database(db_err) = &err
                && matches!(db_err.code().as_deref(), Some("23514") | Some("23503"))
            {
                return DecideError::Rejected(crate::rule::rejection(db_err.as_ref()));
            }
            DecideError::Db(err)
        };

        // Решение возможно после вскрытия (тендер qualification) по «живой» заявке
        let updated = sqlx::query_scalar!(
            "UPDATE core.applications a
             SET status = $2::text::core.application_status, rejection_reason = $3
             FROM core.tenders t
             WHERE a.id = $1 AND t.id = a.tender_id
               AND t.status = 'qualification'
               AND a.status IN ('submitted', 'fee_confirmed')
               AND core.application_package_complete(a.id)
             RETURNING a.tender_id",
            application_id,
            verdict,
            rejection_reason
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_db)?;

        let tender_id = updated.ok_or(DecideError::NotDecidable)?;

        // Решение принимается на открытом заседании (FR-1102): без него
        // голосовать было невозможно, а значит и фиксировать нечего
        let opened = sqlx::query_scalar!(
            "SELECT opened_at FROM core.sessions_meetings
             WHERE tender_id = $1 AND kind = 'qualification'",
            tender_id
        )
        .fetch_optional(&mut *tx)
        .await?;
        if !matches!(opened, Some(Some(_))) {
            return Err(DecideError::NotDecidable);
        }

        // Недопущенному участнику взнос возвращается (FR-1002, п. 26.3)
        if verdict == "rejected" {
            crate::applications::schedule_refund_if_paid(&mut *tx, application_id).await?;
        }

        let record = crate::applications::application_query!(" WHERE a.id = $1", application_id)
            .fetch_one(&mut *tx)
            .await?;
        Ok(ApplicationRecord::from(record))
    })
    .await
}

pub struct VoteRecord {
    pub application_id: Uuid,
    pub member_id: Uuid,
    pub member_name: String,
    pub value: String,
    pub dissent: Option<String>,
}

/// Голоса заседания (для протокола: мнение каждого члена, п. 55).
pub async fn votes_of_meeting(db: &Db, meeting_id: Uuid) -> Result<Vec<VoteRecord>, sqlx::Error> {
    sqlx::query_as!(
        VoteRecord,
        r#"SELECT v.application_id, v.member_id, u.full_name AS member_name,
                  v.value::text AS "value!", v.dissent
           FROM core.votes v
           JOIN core.commission_members cm ON cm.id = v.member_id
           JOIN core.users u ON u.id = cm.user_id
           WHERE v.meeting_id = $1
           ORDER BY v.application_id, u.full_name"#,
        meeting_id
    )
    .fetch_all(db)
    .await
}

pub struct ProtocolRecord {
    pub id: Uuid,
    pub tender_id: Uuid,
    pub kind: String,
    pub number: Option<String>,
    pub content: serde_json::Value,
    pub pdf_key: Option<String>,
    pub generated_at: OffsetDateTime,
}

macro_rules! protocol_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            ProtocolRecord,
            r#"SELECT id, tender_id, kind::text AS "kind!", number, content,
                      pdf_key, generated_at
               FROM core.protocols"# + $tail
            $(, $arg)*
        )
    };
}
/// То же для `RETURNING`: столбцы идут в конце запроса, поэтому макрос
/// принимает голову.
macro_rules! protocol_query_returning {
    ($head:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            ProtocolRecord,
            $head + r#" RETURNING id, tender_id, kind::text AS "kind!", number,
                                  content, pdf_key, generated_at"#
            $(, $arg)*
        )
    };
}

pub struct NewProtocol<'a> {
    pub tender_id: Uuid,
    /// admission | results | failed | winner2 (enum БД `core.protocol_kind`)
    pub kind: &'a str,
    pub meeting_id: Uuid,
    pub number: &'a str,
    /// Полный jsonb-снимок печатной формы (п. 55, 73–74)
    pub content: &'a serde_json::Value,
    /// Ключ PDF в RustFS (бакет dossiers)
    pub pdf_key: &'a str,
}

/// Протокол одного вида на тендер (UNIQUE): повтор - None (уже сформирован).
pub async fn insert_protocol(
    db: &Db,
    actor: Uuid,
    new: NewProtocol<'_>,
) -> Result<Option<ProtocolRecord>, sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        let inserted = protocol_query_returning!(
            "INSERT INTO core.protocols (tender_id, kind, meeting_id, number, content, pdf_key)
             VALUES ($1, $2::text::core.protocol_kind, $3, $4, $5, $6)
             ON CONFLICT (tender_id, kind) DO NOTHING",
            new.tender_id,
            new.kind,
            new.meeting_id,
            new.number,
            new.content,
            new.pdf_key
        )
        .fetch_optional(&mut *tx)
        .await?;

        // Срок оформления закрыт фактом протокола; протокол допуска сам
        // запускает срок уведомления допущенных (FR-1702, п. 54, 57, 73, 75)
        if inserted.is_some() {
            let subject = crate::obligations::Subject::tender(new.tender_id);
            let (done, next) = match new.kind {
                "admission" => (
                    ObligationAction::AdmissionProtocol,
                    Some(ObligationAction::NotifyAdmitted),
                ),
                "results" => (
                    ObligationAction::ResultsProtocol,
                    Some(ObligationAction::PublishResults),
                ),
                _ => (ObligationAction::ResultsProtocol, None),
            };
            crate::obligations::complete(&mut *tx, done, subject).await?;
            if let Some(next) = next {
                crate::obligations::schedule(&mut *tx, next, subject).await?;
            }
        }
        Ok(inserted)
    })
    .await
}

pub async fn get_protocol(
    db: &Db,
    tender_id: Uuid,
    kind: &str,
) -> Result<Option<ProtocolRecord>, sqlx::Error> {
    protocol_query!(
        " WHERE tender_id = $1 AND kind = $2::text::core.protocol_kind",
        tender_id,
        kind
    )
    .fetch_optional(db)
    .await
}
