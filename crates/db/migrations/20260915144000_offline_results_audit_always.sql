-- Keep the audit trigger active even during replica-mode maintenance and restores.
-- The table was added after the original append-only trigger hardening migration.
ALTER TABLE core.offline_tender_results
    ENABLE ALWAYS TRIGGER audit_record;
