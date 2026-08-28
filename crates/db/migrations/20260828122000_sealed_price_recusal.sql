-- The stricter pre-opening seal must retain the commission-recusal barrier:
-- after opening, a recused member still cannot read materials of that lot.
DROP POLICY sealed_until_opening ON core.price_proposals;

CREATE POLICY sealed_until_opening ON core.price_proposals FOR SELECT
  USING (
    EXISTS (
      SELECT 1
      FROM core.applications a
      JOIN core.tenders t ON t.id = a.tender_id
      WHERE a.id = price_proposals.application_id
        AND t.opened_at IS NOT NULL
    )
    AND NOT EXISTS (
      SELECT 1
      FROM core.applications a
      JOIN core.commission_members cm ON cm.user_id = core.current_app_user()
      WHERE a.id = price_proposals.application_id
        AND core.member_recused(cm.id, a.tender_id, a.lot_id)
    )
  );
