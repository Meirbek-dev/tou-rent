-- The administrator's explicitly enabled development-data purge must remove
-- the append-only offline outcome before it starts deleting applications and
-- protocols. Keep ordinary parent deletes restrictive: only purge_data()
-- temporarily disables the outcome's mutation guard.
ALTER TABLE core.offline_tender_results
    DROP CONSTRAINT offline_tender_results_tender_id_fkey,
    ADD CONSTRAINT offline_tender_results_tender_id_fkey
        FOREIGN KEY (tender_id) REFERENCES core.tenders(id),
    DROP CONSTRAINT offline_tender_results_superseded_protocol_id_fkey,
    ADD CONSTRAINT offline_tender_results_superseded_protocol_id_fkey
        FOREIGN KEY (superseded_protocol_id) REFERENCES core.protocols(id);

DO $migration$
DECLARE
  original text;
  patched text;
BEGIN
  SELECT pg_get_functiondef('core.purge_data(text,uuid[])'::regprocedure)
    INTO original;

  patched := replace(
    original,
    E'  steps constant text[][] := ARRAY[\n    [''public_records'',',
    E'  steps constant text[][] := ARRAY[\n    [''offline_tender_results'', ''tender_id = ANY($1)''],\n    [''public_records'','
  );
  IF patched = original THEN
    RAISE EXCEPTION 'offline results: purge step insertion point was not found';
  END IF;

  original := patched;
  patched := replace(
    original,
    E'      AND pr_.proname = ''forbid_mutation''',
    E'      AND pr_.proname IN (''forbid_mutation'', ''validate_offline_tender_results'')'
  );
  IF patched = original THEN
    RAISE EXCEPTION 'offline results: purge guard insertion point was not found';
  END IF;

  EXECUTE patched;
END $migration$;

-- Marker consumed by the completeness test together with the original purge
-- migration. The live function receives the same row above.
-- ['offline_tender_results', 'tender_id = ANY($1)']
