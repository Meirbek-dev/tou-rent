//! Итоги торгов (М7, FR-701): данные протокола итогов и заседание,
//! на котором он утверждается.
//!
//! Сам протокол пишется через [`crate::admission::insert_protocol`] с видом
//! `results` - UNIQUE (tender_id, kind) делает его однократным, как и протокол
//! допуска. Здесь - сбор данных п. 73–74: лоты с объектами, результат торгов
//! по каждому лоту и срок формирования (3 рабочих дня, `refdata.add_business_days`).

use rust_decimal::Decimal;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::Db;
use crate::admission::MeetingRecord;

/// Лот тендера с объектом и итогом торгов по нему (FR-701).
pub struct LotResultRecord {
    pub lot_id: Uuid,
    pub seq: i32,
    pub purpose: String,
    pub lease_months: i32,
    pub base_rate_monthly: Decimal,
    pub guarantee_fee: Decimal,
    pub object_name: String,
    pub object_address: String,
    pub object_area_m2: Decimal,
    /// None - торги по лоту не открывались
    pub auction_status: Option<String>,
    pub starting_bid: Option<Decimal>,
    pub bid_step: Option<Decimal>,
    pub finished_at: Option<OffsetDateTime>,
    pub finished_early: Option<bool>,
    pub winner_name: Option<String>,
    pub winner_amount: Option<Decimal>,
    pub runner_up_name: Option<String>,
    pub runner_up_amount: Option<Decimal>,
}

/// Лоты тендера с объектами и результатами торгов, по порядку лотов.
///
/// `?` у столбцов торгов: торги приходят `LEFT JOIN`'ом, а `starting_bid`,
/// `bid_step` и `finished_early` - NOT NULL, и sqlx выводит nullability
/// по самому столбцу, а не по виду соединения.
pub async fn lot_results(db: &Db, tender_id: Uuid) -> Result<Vec<LotResultRecord>, sqlx::Error> {
    sqlx::query_as!(
        LotResultRecord,
        r#"SELECT l.id AS lot_id, l.seq, l.purpose, l.lease_months,
                l.base_rate_monthly, l.guarantee_fee,
                o.name AS object_name, o.address AS object_address, o.area_m2 AS object_area_m2,
                a.status::text AS auction_status,
                a.starting_bid AS "starting_bid?", a.bid_step AS "bid_step?",
                a.finished_at, a.finished_early AS "finished_early?",
                win.applicant_details->>'name' AS winner_name, a.winner_amount,
                second.applicant_details->>'name' AS runner_up_name, a.runner_up_amount
         FROM core.lots l
         JOIN core.objects o ON o.id = l.object_id
         LEFT JOIN core.auctions a ON a.lot_id = l.id
         LEFT JOIN core.applications win ON win.id = a.winner_application_id
         LEFT JOIN core.applications second ON second.id = a.runner_up_application_id
         WHERE l.tender_id = $1
         ORDER BY l.seq"#,
        tender_id
    )
    .fetch_all(db)
    .await
}

#[derive(Debug, thiserror::Error)]
pub enum MeetingError {
    /// Нет действующей комиссии - заседание итогов провести некому (seed М11)
    #[error("нет действующей тендерной комиссии")]
    NoCommission,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Заседание об итогах (п. 73): берется существующее, иначе создается -
/// назначенное на время окончания торгов, проведенное сейчас.
pub async fn results_meeting(
    db: &Db,
    actor: Uuid,
    tender_id: Uuid,
) -> Result<MeetingRecord, MeetingError> {
    crate::with_actor(db, actor, async |tx| {
        let existing = sqlx::query_as!(
            MeetingRecord,
            "SELECT m.id, m.tender_id, m.commission_id, c.name AS commission_name,
                    m.scheduled_at, m.held_at, m.opened_at,
                    m.quorum_present, m.quorum_required
             FROM core.sessions_meetings m
             JOIN core.commissions c ON c.id = m.commission_id
             WHERE m.tender_id = $1 AND m.kind = 'results'",
            tender_id
        )
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(meeting) = existing {
            return Ok(meeting);
        }

        // Итоги подводит та же комиссия, что и допуск; если заседания допуска
        // нет - действующая на сегодня (срок полномочий, п. 9–11)
        let commission_id = sqlx::query_scalar!(
            "SELECT COALESCE(
                 (SELECT commission_id FROM core.sessions_meetings
                  WHERE tender_id = $1 AND kind = 'qualification'),
                 (SELECT id FROM core.commissions
                  WHERE valid_from <= current_date AND current_date < valid_until
                  ORDER BY valid_from DESC LIMIT 1))",
            tender_id
        )
        .fetch_one(&mut *tx)
        .await?;
        let commission_id = commission_id.ok_or(MeetingError::NoCommission)?;

        let meeting = sqlx::query_as!(
            MeetingRecord,
            r#"INSERT INTO core.sessions_meetings
               (tender_id, commission_id, kind, scheduled_at, held_at)
             SELECT $1, $2, 'results',
                    COALESCE(
                      (SELECT max(a.finished_at) FROM core.auctions a
                       JOIN core.lots l ON l.id = a.lot_id WHERE l.tender_id = $1),
                      core.now()),
                    core.now()
             RETURNING id, tender_id, commission_id,
               (SELECT name FROM core.commissions c WHERE c.id = commission_id)
                 AS "commission_name!",
               scheduled_at, held_at, opened_at, quorum_present, quorum_required"#,
            tender_id,
            commission_id
        )
        .fetch_one(&mut *tx)
        .await?;

        Ok(meeting)
    })
    .await
}

/// Крайний срок протокола итогов (FR-701): `days` рабочих дней после торгов
/// по производственному календарю (`refdata.add_business_days`, G12).
pub async fn protocol_deadline(
    db: &Db,
    from: OffsetDateTime,
    days: i32,
) -> Result<OffsetDateTime, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT (refdata.add_business_days(
                   ($1 AT TIME ZONE 'Asia/Almaty')::date, $2
                 )::timestamp + time '18:00') AT TIME ZONE 'Asia/Almaty'
                 AS "deadline!""#,
        from,
        days
    )
    .fetch_one(db)
    .await
}
