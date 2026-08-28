-- RU remains the canonical value used by older clients; KK is explicit for
-- the bilingual public showcase. Existing rows receive a safe fallback.
ALTER TABLE core.objects
  ADD COLUMN name_kk text,
  ADD COLUMN address_kk text;

UPDATE core.objects SET name_kk = name, address_kk = address;

ALTER TABLE core.objects
  ALTER COLUMN name_kk SET NOT NULL,
  ALTER COLUMN address_kk SET NOT NULL,
  ADD CONSTRAINT objects_name_kk_not_blank CHECK (btrim(name_kk) <> ''),
  ADD CONSTRAINT objects_address_kk_not_blank CHECK (btrim(address_kk) <> '');

ALTER TABLE core.lots ADD COLUMN purpose_kk text;
UPDATE core.lots SET purpose_kk = purpose;
ALTER TABLE core.lots
  ALTER COLUMN purpose_kk SET NOT NULL,
  ADD CONSTRAINT lots_purpose_kk_not_blank CHECK (btrim(purpose_kk) <> '');

-- Tender documentation is domain state and must be visible in the immutable
-- audit chain just like tenders, lots and application files.
CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.tender_docs
  FOR EACH ROW EXECUTE FUNCTION audit.record();

CREATE INDEX tender_docs_tender_version_idx
  ON core.tender_docs (tender_id, version DESC, published_at DESC);
