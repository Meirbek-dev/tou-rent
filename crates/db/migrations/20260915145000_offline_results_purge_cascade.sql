-- Offline results belong to their tender and must leave with it during the
-- explicitly enabled development-data purge. A superseded protocol is deleted
-- earlier in that purge, so it owns the same record as well.
ALTER TABLE core.offline_tender_results
    DROP CONSTRAINT offline_tender_results_tender_id_fkey,
    ADD CONSTRAINT offline_tender_results_tender_id_fkey
        FOREIGN KEY (tender_id) REFERENCES core.tenders(id) ON DELETE CASCADE,
    DROP CONSTRAINT offline_tender_results_superseded_protocol_id_fkey,
    ADD CONSTRAINT offline_tender_results_superseded_protocol_id_fkey
        FOREIGN KEY (superseded_protocol_id) REFERENCES core.protocols(id) ON DELETE CASCADE;
