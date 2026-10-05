//! Узкая административная корректировка несостоявшегося тендера.
//!
//! Выбранные заявки и цены не переписывают заявки, торги или договоры:
//! неизменяемый снимок хранится в самом тендере, а его UPDATE аудируется.

use serde_json::Value;
use uuid::Uuid;

use crate::{Db, failure::FailureError};

/// Форма получает только данные, относящиеся к выбранному тендеру.
pub async fn state(db: &Db, actor: Uuid, tender: Uuid) -> Result<Option<Value>, sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query_scalar::<_, Value>(
            r#"SELECT jsonb_build_object(
              'tender_id',t.id,
              'title',t.title,
              'status',t.status::text,
              'eligible',t.status='failed' AND t.admin_outcome_protocol_id IS NULL
                AND NOT EXISTS(SELECT 1 FROM core.auctions x JOIN core.lots l ON l.id=x.lot_id WHERE l.tender_id=t.id)
                AND NOT EXISTS(SELECT 1 FROM core.contracts WHERE tender_id=t.id)
                AND NOT EXISTS(SELECT 1 FROM core.protocols WHERE tender_id=t.id AND kind::text='results')
                AND NOT EXISTS(SELECT 1 FROM core.lots WHERE tender_id=t.id AND cancelled_at IS NOT NULL)
                AND NOT EXISTS(SELECT 1 FROM core.tenders child WHERE child.repeat_of=t.id),
              'protocols',coalesce((SELECT jsonb_agg(jsonb_build_object(
                'id',d.id,'title',d.title,'number',d.number,'document_date',d.document_date::text
              ) ORDER BY d.document_date DESC,d.uploaded_at DESC,d.id)
                FROM core.commission_documents d
                WHERE d.tender_id=t.id AND d.application_id IS NULL
                  AND d.document_date <= (core.now() AT TIME ZONE 'Asia/Almaty')::date),'[]'::jsonb),
              'lots',coalesce((SELECT jsonb_agg(jsonb_build_object(
                'id',l.id,'seq',l.seq,'purpose',l.purpose,
                'applications',coalesce((SELECT jsonb_agg(jsonb_build_object(
                  'id',a.id,'applicant',coalesce(a.applicant_details->>'name','—'),
                  'status',a.status::text,'price',core.price_amount(p)::text
                ) ORDER BY a.submitted_at,a.id)
                  FROM core.applications a
                  LEFT JOIN core.price_proposals p ON p.application_id=a.id
                  WHERE a.lot_id=l.id AND a.status IN ('submitted','admitted')),'[]'::jsonb)
              ) ORDER BY l.seq) FROM core.lots l WHERE l.tender_id=t.id),'[]'::jsonb),
              'recorded_protocol_id',t.admin_outcome_protocol_id,
              'recorded_lots',t.admin_outcome_lots,
              'recorded_reason',t.admin_outcome_reason,
              'recorded_at',t.admin_outcome_recorded_at
            ) FROM core.tenders t WHERE t.id=$1"#,
        )
        .bind(tender)
        .fetch_optional(&mut *tx)
        .await
    })
    .await
}

/// Один UPDATE является и сменой статуса, и фиксацией основания/снимка.
pub async fn record(
    db: &Db,
    actor: Uuid,
    tender: Uuid,
    protocol: Uuid,
    reason: &str,
    lots: &Value,
) -> Result<bool, FailureError> {
    crate::with_actor(db, actor, async |tx| {
        let updated = sqlx::query_scalar::<_, Uuid>(
            r#"UPDATE core.tenders SET
              status='summed_up',
              failure_ground=NULL,
              consequence=NULL,
              failed_at=NULL,
              admin_outcome_protocol_id=$2,
              admin_outcome_lots=$3,
              admin_outcome_reason=$4,
              admin_outcome_recorded_by=$5,
              admin_outcome_recorded_at=core.now()
            WHERE id=$1 AND status='failed' AND admin_outcome_protocol_id IS NULL
            RETURNING id"#,
        )
        .bind(tender)
        .bind(protocol)
        .bind(lots)
        .bind(reason)
        .bind(actor)
        .fetch_optional(&mut *tx)
        .await
        .map_err(super::failure::map_rule)?;
        Ok(updated.is_some())
    })
    .await
}
