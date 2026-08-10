-- Изменение тендерной документации и отмена (М3, FR-304, FR-305, FR-1004,
-- п. 26.5, 27, 78–79). Новая редакция — юридический факт публикации со своими
-- следствиями: срок приема продлевается, участники извещаются и вправе
-- отказаться с возвратом взноса. Отмена возможна до заключения договора
-- и только с основанием.

-- Редакция тендерной документации (FR-304): что изменено и как сдвинут срок
CREATE TABLE core.tender_amendments (
  id                uuid        PRIMARY KEY DEFAULT uuidv7(),
  tender_id         uuid        NOT NULL REFERENCES core.tenders (id) ON DELETE CASCADE,
  version           integer     NOT NULL CHECK (version > 0),
  summary           text        NOT NULL,  -- существо изменений (баннер участника)
  previous_deadline timestamptz,
  new_deadline      timestamptz NOT NULL,
  doc_key           text,       -- PDF новой редакции объявления в RustFS
  created_by        uuid        REFERENCES core.users (id),
  created_at        timestamptz NOT NULL DEFAULT now(),
  UNIQUE (tender_id, version)
);

CREATE INDEX tender_amendments_tender_idx ON core.tender_amendments (tender_id, created_at DESC);

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.tender_amendments
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Опубликованная редакция не переписывается: участники приняли решение по ней.
-- Дописывается единственное поле — ссылка на печатную форму после рендера.
CREATE FUNCTION core.allow_amendment_doc_key() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.doc_key IS NULL AND NEW.doc_key IS NOT NULL
     AND NEW.tender_id = OLD.tender_id AND NEW.version = OLD.version
     AND NEW.summary = OLD.summary AND NEW.new_deadline = OLD.new_deadline THEN
    RETURN NEW;
  END IF;
  RAISE EXCEPTION 'FR-304: опубликованная редакция документации не изменяется';
END $$;

CREATE TRIGGER tender_amendments_doc_key_only BEFORE UPDATE ON core.tender_amendments
  FOR EACH ROW EXECUTE FUNCTION core.allow_amendment_doc_key();

CREATE TRIGGER tender_amendments_append_only BEFORE DELETE ON core.tender_amendments
  FOR EACH ROW EXECUTE FUNCTION core.forbid_mutation('FR-304');

REVOKE DELETE ON core.tender_amendments FROM tou_rent_app;

-- Условия изменения (п. 27): тендер опубликован и не вскрыт, до дедлайна
-- больше двух календарных дней, новый срок приема — не менее чем через
-- десять календарных дней. Те же правила выражены типом в `domain::amendment`.
CREATE FUNCTION core.check_tender_amendment() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  tender core.tenders%ROWTYPE;
BEGIN
  SELECT * INTO tender FROM core.tenders WHERE id = NEW.tender_id FOR UPDATE;

  IF tender.status NOT IN ('announced', 'accepting', 'repeat_announced') THEN
    RAISE EXCEPTION
      'FR-304: документация изменяется между публикацией и вскрытием (сейчас %)', tender.status;
  END IF;
  IF tender.opened_at IS NOT NULL THEN
    RAISE EXCEPTION 'FR-304: заявки вскрыты — условия тендера больше не меняются (п. 50)';
  END IF;
  IF tender.submission_deadline IS NULL THEN
    RAISE EXCEPTION 'FR-304: у тендера не назначен срок приема заявок';
  END IF;
  IF tender.submission_deadline < now() THEN
    RAISE EXCEPTION 'FR-304: срок приема заявок истек — изменение невозможно (п. 27)';
  END IF;
  IF tender.submission_deadline - now() < interval '2 days' THEN
    RAISE EXCEPTION
      'FR-304: до окончания приема меньше 2 календарных дней — документация не изменяется (п. 27)';
  END IF;
  IF NEW.new_deadline <= tender.submission_deadline THEN
    RAISE EXCEPTION 'FR-304: новая редакция обязана продлить срок приема заявок (п. 27)';
  END IF;
  IF NEW.new_deadline - now() < interval '10 days' THEN
    RAISE EXCEPTION
      'FR-304: срок приема продлевается не менее чем на 10 календарных дней (п. 27)';
  END IF;
  IF btrim(NEW.summary) = '' THEN
    RAISE EXCEPTION 'FR-304: редакция публикуется с описанием изменений (п. 27)';
  END IF;

  NEW.previous_deadline := tender.submission_deadline;
  NEW.version := coalesce(
    (SELECT max(a.version) + 1 FROM core.tender_amendments a WHERE a.tender_id = NEW.tender_id),
    1);
  RETURN NEW;
END $$;

CREATE TRIGGER check_tender_amendment BEFORE INSERT ON core.tender_amendments
  FOR EACH ROW EXECUTE FUNCTION core.check_tender_amendment();

-- Следствие редакции: срок приема продлевается, а вскрытие сдвигается
-- вслед за ним (CHECK deadline_before_opening) — не позднее чем на ту же
-- разницу, чтобы дата заседания оставалась осмысленной.
CREATE FUNCTION core.apply_amendment_effects() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  UPDATE core.tenders
  SET submission_deadline = NEW.new_deadline,
      opening_at = CASE
        WHEN opening_at IS NULL OR opening_at >= NEW.new_deadline THEN opening_at
        ELSE NEW.new_deadline + (opening_at - NEW.previous_deadline)
      END
  WHERE id = NEW.tender_id;

  RETURN NULL;  -- AFTER-триггер
END $$;

CREATE TRIGGER apply_amendment_effects AFTER INSERT ON core.tender_amendments
  FOR EACH ROW EXECUTE FUNCTION core.apply_amendment_effects();

-- Отказ участника от участия из-за изменения условий (FR-1004, п. 26.5):
-- заявка связывается с редакцией, из-за которой она отозвана
ALTER TABLE core.applications
  ADD COLUMN declined_amendment_id uuid REFERENCES core.tender_amendments (id);

COMMENT ON COLUMN core.applications.declined_amendment_id IS
  'FR-1004: заявка отозвана из-за изменения условий тендера — взнос возвращается (п. 26.5)';

-- Отмена тендера и лота (FR-305, п. 78–79): основание обязательно
ALTER TABLE core.tenders
  ADD COLUMN cancelled_at  timestamptz,
  ADD COLUMN cancel_reason text;

ALTER TABLE core.lots
  ADD COLUMN cancelled_at  timestamptz,
  ADD COLUMN cancel_reason text,
  ADD CONSTRAINT lot_cancellation_has_reason
    CHECK (cancelled_at IS NULL OR btrim(coalesce(cancel_reason, '')) <> '');

COMMENT ON COLUMN core.tenders.cancel_reason IS
  'FR-305: нарушение, повлекшее отмену (п. 78) — отмена по усмотрению невозможна';

-- Отмена возможна до заключения договора и только с основанием (п. 78)
CREATE FUNCTION core.check_tender_cancellation() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.status <> 'cancelled' OR OLD.status = 'cancelled' THEN
    RETURN NEW;
  END IF;

  IF btrim(coalesce(NEW.cancel_reason, '')) = '' THEN
    RAISE EXCEPTION 'FR-305: тендер отменяется с указанием нарушения (п. 78)';
  END IF;
  IF EXISTS (
       SELECT 1 FROM core.contracts c
       WHERE c.tender_id = NEW.id AND c.registered_at IS NOT NULL
     ) THEN
    RAISE EXCEPTION
      'FR-305: по тендеру заключен договор — отмена возможна только до его заключения (п. 78)';
  END IF;

  NEW.cancelled_at := coalesce(NEW.cancelled_at, now());
  RETURN NEW;
END $$;

CREATE TRIGGER check_tender_cancellation BEFORE UPDATE ON core.tenders
  FOR EACH ROW EXECUTE FUNCTION core.check_tender_cancellation();

CREATE FUNCTION core.check_lot_cancellation() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.cancelled_at IS NULL OR OLD.cancelled_at IS NOT NULL THEN
    RETURN NEW;
  END IF;

  IF EXISTS (
       SELECT 1 FROM core.contracts c
       WHERE c.lot_id = NEW.id AND c.registered_at IS NOT NULL
     ) THEN
    RAISE EXCEPTION
      'FR-305: по лоту заключен договор — отмена возможна только до его заключения (п. 78)';
  END IF;

  RETURN NEW;
END $$;

CREATE TRIGGER check_lot_cancellation BEFORE UPDATE ON core.lots
  FOR EACH ROW EXECUTE FUNCTION core.check_lot_cancellation();

-- FR-103: отмененный лот больше не держит объект «в тендере»
CREATE OR REPLACE VIEW core.object_statuses AS
SELECT
  o.id AS object_id,
  CASE
    WHEN EXISTS (
      SELECT 1 FROM core.contracts c
      WHERE c.object_id = o.id AND c.status = 'active'
    ) THEN 'leased'
    WHEN EXISTS (
      SELECT 1
      FROM core.lots l
      JOIN core.tenders t ON t.id = l.tender_id
      WHERE l.object_id = o.id
        AND l.cancelled_at IS NULL
        AND t.status IN ('announced', 'accepting', 'qualification', 'trading', 'summed_up')
    ) THEN 'in_tender'
    ELSE 'free'
  END AS status
FROM core.objects o;
