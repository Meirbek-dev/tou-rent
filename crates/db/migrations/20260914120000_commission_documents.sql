-- FR-703, FR-1601, FR-1602; Q-025: external offline decisions are documents,
-- not electronic votes or changes to the outcome of a tender.
CREATE TABLE core.commission_documents (
  id uuid PRIMARY KEY DEFAULT uuidv7(),
  tender_id uuid NOT NULL REFERENCES core.tenders(id) ON DELETE CASCADE,
  application_id uuid REFERENCES core.applications(id) ON DELETE CASCADE,
  title text NOT NULL CHECK (char_length(btrim(title)) BETWEEN 1 AND 200),
  number text NOT NULL CHECK (char_length(btrim(number)) BETWEEN 1 AND 100),
  document_date date NOT NULL,
  filename text NOT NULL,
  file_key text NOT NULL UNIQUE,
  size_bytes bigint NOT NULL CHECK (size_bytes BETWEEN 1 AND 10485760),
  uploaded_by uuid NOT NULL REFERENCES core.users(id),
  uploaded_at timestamptz NOT NULL DEFAULT core.now(),
  shared_at timestamptz
);
CREATE INDEX commission_documents_tender_idx ON core.commission_documents(tender_id, uploaded_at, id);

CREATE FUNCTION core.check_commission_document() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'UPDATE' THEN
    IF (to_jsonb(NEW) - 'shared_at') IS DISTINCT FROM (to_jsonb(OLD) - 'shared_at') THEN
      RAISE EXCEPTION 'FR-1602: stored commission documents cannot be replaced';
    END IF;
  ELSE
    IF NOT EXISTS (SELECT 1 FROM core.tenders WHERE id = NEW.tender_id AND opened_at IS NOT NULL) THEN
      RAISE EXCEPTION 'FR-703: tender envelopes must be opened first';
    END IF;
    IF NEW.application_id IS NOT NULL AND NOT EXISTS (
      SELECT 1 FROM core.applications WHERE id = NEW.application_id AND tender_id = NEW.tender_id
    ) THEN
      RAISE EXCEPTION 'FR-703: application belongs to another tender';
    END IF;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER check_commission_document BEFORE INSERT OR UPDATE ON core.commission_documents
  FOR EACH ROW EXECUTE FUNCTION core.check_commission_document();
CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.commission_documents
  FOR EACH ROW EXECUTE FUNCTION audit.record();

CREATE FUNCTION core.dossier_on_commission_document() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  PERFORM core.record_dossier_item(NEW.tender_id, 'commission_document', NEW.title,
    NEW.file_key, 'core.commission_documents', NEW.id);
  RETURN NULL;
END $$;
CREATE TRIGGER dossier_on_commission_document AFTER INSERT ON core.commission_documents
  FOR EACH ROW EXECUTE FUNCTION core.dossier_on_commission_document();
GRANT SELECT, INSERT, UPDATE ON core.commission_documents TO tou_rent_app;
REVOKE DELETE, TRUNCATE ON core.commission_documents FROM tou_rent_app;
