//! Signed offline decisions, separate from votes, admissions and contracts (Q-026).
use serde_json::Value;
use uuid::Uuid;

use crate::{Db, failure::FailureError};

pub async fn recorded(db: &Db, tender: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM core.offline_tender_results WHERE tender_id=$1) AS \"exists!\"",
        tender
    ).fetch_one(db).await
}

/// Preview uses the same live sources as the insertion guard, never client prices.
pub async fn state(db: &Db, actor: Uuid, tender: Uuid) -> Result<Option<Value>, sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query_scalar!(r#"SELECT jsonb_build_object(
          'recorded', r.tender_id IS NOT NULL,
          'protocol_id', r.protocol_id, 'recorded_by', r.recorded_by,
          'recorded_at', r.recorded_at,
          'eligible', coalesce(t.status='qualification' AND t.opened_at IS NOT NULL
            AND t.submission_deadline < core.now() AND t.repeat_of IS NULL
            AND NOT EXISTS(SELECT 1 FROM core.auctions x JOIN core.lots l ON l.id=x.lot_id WHERE l.tender_id=t.id)
            AND NOT EXISTS(SELECT 1 FROM core.contracts WHERE tender_id=t.id)
            AND NOT EXISTS(SELECT 1 FROM core.protocols WHERE tender_id=t.id AND kind::text IN ('results','failed'))
            AND NOT EXISTS(SELECT 1 FROM core.lots WHERE tender_id=t.id AND cancelled_at IS NOT NULL)
            AND NOT EXISTS(SELECT 1 FROM core.applications WHERE tender_id=t.id AND status<>'withdrawn' GROUP BY lot_id HAVING count(*)>1), false),
          'lots', coalesce(r.lots, (SELECT jsonb_agg(jsonb_build_object(
            'lot_id', l.id, 'seq', l.seq, 'application_id', a.id,
            'applicant', a.applicant_details->>'name', 'price', core.price_amount(p)::text,
            'ground', NULL, 'resolution', NULL, 'note', '', 'application_status', a.status::text
          ) ORDER BY l.seq,a.id) FROM core.lots l
            LEFT JOIN core.applications a ON a.lot_id=l.id AND a.status<>'withdrawn'
            LEFT JOIN core.price_proposals p ON p.application_id=a.id
            WHERE l.tender_id=t.id), '[]'::jsonb)
        ) AS "state!" FROM core.tenders t LEFT JOIN core.offline_tender_results r ON r.tender_id=t.id
        WHERE t.id=$1 AND t.opened_at IS NOT NULL"#, tender)
            .fetch_optional(&mut *tx).await
    }).await
}

/// Single atomic insert: DB validates/snapshots lots, closes tender and cancels notify duty.
pub async fn record(
    db: &Db,
    actor: Uuid,
    tender: Uuid,
    protocol: Uuid,
    lots: &Value,
) -> Result<(), FailureError> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query!(
            "INSERT INTO core.offline_tender_results(tender_id,protocol_id,recorded_by,lots)
          VALUES($1,$2,$3,$4)",
            tender,
            protocol,
            actor,
            lots
        )
        .execute(&mut *tx)
        .await
        .map_err(super::failure::map_rule)?;
        Ok(())
    })
    .await
}
