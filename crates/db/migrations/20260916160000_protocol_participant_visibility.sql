-- Отдельная видимость копии протокола в кабинете участника.
-- Публикация и ее шестимесячный срок остаются юридическими фактами:
-- переключатель не удаляет протокол, не меняет PDF и не снимает публичную
-- публикацию раньше срока (INV-076).
ALTER TABLE core.protocols
  ADD COLUMN IF NOT EXISTS visible_to_participants boolean NOT NULL DEFAULT true;

COMMENT ON COLUMN core.protocols.visible_to_participants IS
  'Показывать копию протокола в личных кабинетах участников; не влияет на публичную публикацию и досье';

-- Корректирующие офлайн-итоги допустимы после того, как ошибочный
-- автоматически сформированный протокол скрыт из кабинетов участников.
-- Сам протокол остается WORM-материалом досье и после фиксации корректировки
-- будет связан с ней через superseded_protocol_id.
CREATE OR REPLACE FUNCTION core.validate_offline_tender_results() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
  t core.tenders%ROWTYPE;
  item jsonb;
  l core.lots%ROWTYPE;
  a core.applications%ROWTYPE;
  n integer;
  seen uuid[] := '{}';
  snapshot jsonb := '[]';
  resolution text;
  price numeric;
  failed_protocol uuid;
BEGIN
  IF TG_OP <> 'INSERT' THEN
    RAISE EXCEPTION 'FR-801: recorded offline results are immutable';
  END IF;
  SELECT * INTO t FROM core.tenders WHERE id = NEW.tender_id FOR UPDATE;
  IF t.id IS NULL OR t.status NOT IN ('qualification','failed') OR t.opened_at IS NULL
     OR t.submission_deadline IS NULL OR t.submission_deadline >= core.now()
     OR t.repeat_of IS NOT NULL
     OR (t.status='failed' AND t.failure_ground IS NULL) THEN
    RAISE EXCEPTION 'FR-801: offline closure requires an opened first tender after the deadline';
  END IF;
  IF NOT EXISTS (SELECT 1 FROM core.commission_documents d
    WHERE d.id = NEW.protocol_id AND d.tender_id = t.id AND d.application_id IS NULL
      AND d.document_date <= (core.now() AT TIME ZONE 'Asia/Almaty')::date) THEN
    RAISE EXCEPTION 'FR-801: select a general commission document for this tender';
  END IF;
  IF EXISTS (SELECT 1 FROM core.auctions x JOIN core.lots y ON y.id=x.lot_id WHERE y.tender_id=t.id)
     OR EXISTS (SELECT 1 FROM core.contracts WHERE tender_id=t.id)
     OR EXISTS (SELECT 1 FROM core.protocols WHERE tender_id=t.id AND kind::text='results')
     OR EXISTS (SELECT 1 FROM core.lots WHERE tender_id=t.id AND cancelled_at IS NOT NULL)
     OR EXISTS (SELECT 1 FROM core.tenders WHERE repeat_of=t.id) THEN
    RAISE EXCEPTION 'FR-801: existing auctions, contracts, results, repeats or cancelled lots require separate review';
  END IF;

  SELECT p.id INTO failed_protocol FROM core.protocols p
    WHERE p.tender_id=t.id AND p.kind::text='failed';
  IF (t.status='qualification' AND failed_protocol IS NOT NULL)
     OR EXISTS (SELECT 1 FROM core.protocols p WHERE p.id=failed_protocol
       AND p.published_at IS NOT NULL AND p.visible_to_participants) THEN
    RAISE EXCEPTION 'FR-801: hide the published failure protocol from participant cabinets before correction';
  END IF;
  NEW.superseded_protocol_id := CASE WHEN t.status='failed' THEN failed_protocol ELSE NULL END;

  IF jsonb_typeof(NEW.lots) IS DISTINCT FROM 'array' THEN
    RAISE EXCEPTION 'FR-801: decisions for every lot are required';
  END IF;
  FOR item IN SELECT value FROM jsonb_array_elements(NEW.lots) LOOP
    SELECT * INTO l FROM core.lots WHERE id=(item->>'lot_id')::uuid AND tender_id=t.id FOR UPDATE;
    IF l.id IS NULL OR l.id = ANY(seen) THEN
      RAISE EXCEPTION 'FR-801: unknown or duplicate lot';
    END IF;
    seen := array_append(seen,l.id);
    PERFORM 1 FROM core.applications WHERE lot_id=l.id FOR UPDATE;
    SELECT count(*) INTO n FROM core.applications WHERE lot_id=l.id AND status<>'withdrawn';
    resolution := item->>'resolution';
    IF n > 1 OR resolution IS NULL OR resolution NOT IN ('no_applications','rejected','single_source') THEN
      RAISE EXCEPTION 'FR-801: offline closure supports only lots with zero or one active application';
    END IF;
    IF char_length(btrim(coalesce(item->>'note',''))) NOT BETWEEN 1 AND 2000 THEN
      RAISE EXCEPTION 'FR-801: a decision excerpt is required for each lot';
    END IF;
    IF n = 0 THEN
      IF resolution <> 'no_applications' OR item->>'application_id' IS NOT NULL THEN
        RAISE EXCEPTION 'FR-801: a lot without applications cannot have a recipient';
      END IF;
      snapshot := snapshot || jsonb_build_array(jsonb_build_object(
        'lot_id',l.id,'seq',l.seq,'application_id',NULL,'applicant',NULL,'price',NULL,
        'ground','no_applications','resolution',resolution,'note',btrim(item->>'note')));
    ELSE
      SELECT * INTO a FROM core.applications WHERE lot_id=l.id AND status<>'withdrawn';
      IF a.id IS DISTINCT FROM (item->>'application_id')::uuid OR resolution='no_applications'
         OR a.status NOT IN ('submitted','admitted','rejected')
         OR (a.status='admitted' AND resolution='rejected')
         OR (a.status='rejected' AND resolution='single_source') THEN
        RAISE EXCEPTION 'FR-801: decision conflicts with the application';
      END IF;
      SELECT core.price_amount(p) INTO price FROM core.price_proposals p WHERE application_id=a.id;
      IF resolution='single_source' AND (price IS NULL OR price<=0) THEN
        RAISE EXCEPTION 'FR-801: a single-source recommendation requires a recorded price';
      END IF;
      snapshot := snapshot || jsonb_build_array(jsonb_build_object(
        'lot_id',l.id,'seq',l.seq,'application_id',a.id,
        'applicant',coalesce(a.applicant_details->>'name','—'),'price',price::text,
        'ground','single_application','resolution',resolution,'note',btrim(item->>'note')));
    END IF;
  END LOOP;
  IF cardinality(seen) <> (SELECT count(*) FROM core.lots WHERE tender_id=t.id) OR cardinality(seen)=0 THEN
    RAISE EXCEPTION 'FR-801: decisions must cover every lot';
  END IF;
  NEW.lots := snapshot;
  NEW.recorded_at := core.now();
  RETURN NEW;
END $$;
