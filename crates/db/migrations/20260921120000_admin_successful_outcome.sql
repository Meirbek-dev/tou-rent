-- Административная фиксация состоявшихся офлайн-итогов.
-- Это не свободное редактирование статуса: администратор прикладывает
-- подписанный общий документ комиссии и по каждому лоту фиксирует одну
-- существующую заявку и итоговую цену. Договоры, уведомления и прежние
-- протоколы операция не изменяет; след остается в audit.log тендера.

ALTER TABLE core.tenders
  ADD COLUMN admin_outcome_protocol_id uuid REFERENCES core.commission_documents(id),
  ADD COLUMN admin_outcome_lots jsonb,
  ADD COLUMN admin_outcome_reason text,
  ADD COLUMN admin_outcome_recorded_by uuid REFERENCES core.users(id),
  ADD COLUMN admin_outcome_recorded_at timestamptz,
  ADD CONSTRAINT admin_outcome_complete CHECK (
    (admin_outcome_protocol_id IS NULL
      AND admin_outcome_lots IS NULL
      AND admin_outcome_reason IS NULL
      AND admin_outcome_recorded_by IS NULL
      AND admin_outcome_recorded_at IS NULL)
    OR
    (admin_outcome_protocol_id IS NOT NULL
      AND jsonb_typeof(admin_outcome_lots) = 'array'
      AND jsonb_array_length(admin_outcome_lots) > 0
      AND char_length(btrim(admin_outcome_reason)) BETWEEN 1 AND 2000
      AND admin_outcome_recorded_by IS NOT NULL
      AND admin_outcome_recorded_at IS NOT NULL
      AND status IN ('summed_up', 'contracted'))
  );

COMMENT ON COLUMN core.tenders.admin_outcome_protocol_id IS
  'Подписанный документ комиссии, на основании которого администратор исправил несостоявшийся тендер';
COMMENT ON COLUMN core.tenders.admin_outcome_lots IS
  'Неизменяемый снимок выбранной заявки и итоговой цены по каждому лоту';

INSERT INTO refdata.tender_status_transitions(from_status,to_status)
VALUES ('failed','summed_up')
ON CONFLICT DO NOTHING;

CREATE FUNCTION core.validate_admin_successful_outcome() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
  item jsonb;
  lot_row core.lots%ROWTYPE;
  application_row core.applications%ROWTYPE;
  application_id uuid;
  price numeric(14,2);
  seen uuid[] := '{}';
  snapshot jsonb := '[]'::jsonb;
BEGIN
  IF OLD.admin_outcome_protocol_id IS NOT NULL THEN
    IF NEW.admin_outcome_protocol_id IS DISTINCT FROM OLD.admin_outcome_protocol_id
       OR NEW.admin_outcome_lots IS DISTINCT FROM OLD.admin_outcome_lots
       OR NEW.admin_outcome_reason IS DISTINCT FROM OLD.admin_outcome_reason
       OR NEW.admin_outcome_recorded_by IS DISTINCT FROM OLD.admin_outcome_recorded_by
       OR NEW.admin_outcome_recorded_at IS DISTINCT FROM OLD.admin_outcome_recorded_at THEN
      RAISE EXCEPTION 'FR-801: administrative outcome is immutable';
    END IF;
    RETURN NEW;
  END IF;

  IF NEW.admin_outcome_protocol_id IS NULL THEN
    RETURN NEW;
  END IF;

  IF OLD.status <> 'failed' OR NEW.status <> 'summed_up'
     OR NEW.failure_ground IS NOT NULL OR NEW.consequence IS NOT NULL
     OR NEW.failed_at IS NOT NULL THEN
    RAISE EXCEPTION 'FR-801: only a failed tender can receive a successful administrative outcome';
  END IF;
  IF NEW.admin_outcome_recorded_by IS DISTINCT FROM core.current_app_user()
     OR jsonb_typeof(NEW.admin_outcome_lots) IS DISTINCT FROM 'array'
     OR char_length(btrim(coalesce(NEW.admin_outcome_reason,''))) NOT BETWEEN 1 AND 2000 THEN
    RAISE EXCEPTION 'FR-801: actor, reason and decisions for every lot are required';
  END IF;
  IF NOT EXISTS (
    SELECT 1 FROM core.commission_documents d
    WHERE d.id=NEW.admin_outcome_protocol_id AND d.tender_id=NEW.id
      AND d.application_id IS NULL
      AND d.document_date <= (core.now() AT TIME ZONE 'Asia/Almaty')::date
  ) THEN
    RAISE EXCEPTION 'FR-801: select a signed general commission document for this tender';
  END IF;
  IF EXISTS (SELECT 1 FROM core.auctions x JOIN core.lots l ON l.id=x.lot_id WHERE l.tender_id=NEW.id)
     OR EXISTS (SELECT 1 FROM core.contracts WHERE tender_id=NEW.id)
     OR EXISTS (SELECT 1 FROM core.protocols WHERE tender_id=NEW.id AND kind::text='results')
     OR EXISTS (SELECT 1 FROM core.lots WHERE tender_id=NEW.id AND cancelled_at IS NOT NULL)
     OR EXISTS (SELECT 1 FROM core.tenders WHERE repeat_of=NEW.id) THEN
    RAISE EXCEPTION 'FR-801: auctions, contracts, results, repeats or cancelled lots require separate review';
  END IF;

  FOR item IN SELECT value FROM jsonb_array_elements(NEW.admin_outcome_lots) LOOP
    BEGIN
      SELECT * INTO lot_row
      FROM core.lots
      WHERE id=(item->>'lot_id')::uuid AND tender_id=NEW.id
      FOR UPDATE;
      application_id := (item->>'application_id')::uuid;
    EXCEPTION WHEN invalid_text_representation OR null_value_not_allowed THEN
      RAISE EXCEPTION 'FR-801: lot and application identifiers are required';
    END;
    IF lot_row.id IS NULL OR lot_row.id=ANY(seen) THEN
      RAISE EXCEPTION 'FR-801: unknown or duplicate lot';
    END IF;
    seen := array_append(seen,lot_row.id);

    SELECT * INTO application_row
    FROM core.applications
    WHERE id=application_id AND tender_id=NEW.id AND lot_id=lot_row.id
      AND status IN ('submitted','admitted')
    FOR UPDATE;
    IF application_row.id IS NULL THEN
      RAISE EXCEPTION 'FR-801: selected application is not eligible for this lot';
    END IF;
    IF coalesce(item->>'price','') !~ '^[0-9]{1,12}([.][0-9]{1,2})?$' THEN
      RAISE EXCEPTION 'FR-801: final price must be a positive amount with two decimal places';
    END IF;
    price := (item->>'price')::numeric(14,2);
    IF price <= 0 THEN
      RAISE EXCEPTION 'FR-801: final price must be positive';
    END IF;

    snapshot := snapshot || jsonb_build_array(jsonb_build_object(
      'lot_id',lot_row.id,
      'seq',lot_row.seq,
      'purpose',lot_row.purpose,
      'application_id',application_row.id,
      'applicant',coalesce(application_row.applicant_details->>'name','—'),
      'price',price::text
    ));
  END LOOP;

  IF cardinality(seen) <> (SELECT count(*) FROM core.lots WHERE tender_id=NEW.id)
     OR cardinality(seen)=0 THEN
    RAISE EXCEPTION 'FR-801: decisions must cover every lot';
  END IF;
  NEW.admin_outcome_lots := snapshot;
  NEW.admin_outcome_reason := btrim(NEW.admin_outcome_reason);
  NEW.admin_outcome_recorded_at := core.now();
  RETURN NEW;
END $$;

CREATE TRIGGER validate_admin_successful_outcome
  BEFORE UPDATE ON core.tenders
  FOR EACH ROW EXECUTE FUNCTION core.validate_admin_successful_outcome();

-- Ранее импортированные офлайн-итоги остаются доказательным материалом,
-- но не должны блокировать узкий подтвержденный переход failed -> summed_up.
CREATE OR REPLACE FUNCTION core.check_failure_ground() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.status='failed' AND OLD.status IS DISTINCT FROM 'failed' AND NEW.failure_ground IS NULL
     AND NOT EXISTS (SELECT 1 FROM core.offline_tender_results WHERE tender_id=NEW.id) THEN
    RAISE EXCEPTION 'FR-801: тендер признается несостоявшимся только по основанию п. 81';
  END IF;
  IF NEW.status='failed' AND OLD.status IS DISTINCT FROM 'failed' THEN
    NEW.failed_at := coalesce(NEW.failed_at,core.now());
  END IF;
  IF OLD.status='failed' AND EXISTS (SELECT 1 FROM core.offline_tender_results WHERE tender_id=OLD.id)
     AND (NEW.status IS DISTINCT FROM OLD.status OR NEW.failure_ground IS DISTINCT FROM OLD.failure_ground
       OR NEW.consequence IS DISTINCT FROM OLD.consequence OR NEW.failed_at IS DISTINCT FROM OLD.failed_at)
     AND NOT (NEW.status='failed' AND OLD.failure_ground IS NOT NULL
       AND NEW.failure_ground IS NULL AND NEW.consequence IS NULL
       AND NEW.failed_at IS NOT DISTINCT FROM OLD.failed_at)
     AND NOT (NEW.status='summed_up' AND NEW.admin_outcome_protocol_id IS NOT NULL
       AND NEW.failure_ground IS NULL AND NEW.consequence IS NULL AND NEW.failed_at IS NULL) THEN
    RAISE EXCEPTION 'FR-801: offline results cannot be replaced by an electronic outcome';
  END IF;
  RETURN NEW;
END $$;

ALTER TABLE core.tenders ENABLE ALWAYS TRIGGER validate_admin_successful_outcome;
