-- Публикация протоколов и досье тендера (М7, М14, М16: FR-702, FR-703,
-- FR-1402, FR-1602, INV-076, п. 6, 16, 56, 75–76).
--
-- Публичность — состояние со сроком: публикация задает шесть месяцев доступа,
-- по истечении джоб снимает протокол, и он остается в досье. Само досье
-- собирается триггерами: материал попадает в него в момент события, а не
-- когда о нем вспомнят.

ALTER TABLE core.protocols
  ADD COLUMN unpublished_at timestamptz;

COMMENT ON COLUMN core.protocols.unpublish_at IS
  'INV-076: момент автоматического снятия — публикация + 6 месяцев (п. 76)';
COMMENT ON COLUMN core.protocols.unpublished_at IS
  'INV-076: факт снятия джобом; протокол остается в досье и у участников (п. 56)';

-- INV-076: срок публичного доступа задает БД, а не вызывающий код.
-- Публикуется только сформированная печатная форма (п. 75).
CREATE FUNCTION core.check_protocol_publication() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  -- Публикация и снятие — юридические факты: их момент не переписывается,
  -- а снятие необратимо (протокол хранится в досье, п. 76)
  IF OLD.published_at IS NOT NULL AND NEW.published_at IS DISTINCT FROM OLD.published_at THEN
    RAISE EXCEPTION 'FR-702: момент публикации протокола не изменяется (п. 75)';
  END IF;
  IF OLD.unpublished_at IS NOT NULL AND NEW.unpublished_at IS DISTINCT FROM OLD.unpublished_at THEN
    RAISE EXCEPTION
      'INV-076: снятие публикации необратимо — протокол хранится в досье (п. 76)';
  END IF;

  IF NEW.published_at IS NOT NULL AND OLD.published_at IS NULL THEN
    IF NEW.pdf_key IS NULL THEN
      RAISE EXCEPTION 'FR-702: печатная форма протокола не сформирована — публиковать нечего (п. 75)';
    END IF;
    IF OLD.unpublished_at IS NOT NULL THEN
      RAISE EXCEPTION 'INV-076: срок публичного доступа истек — протокол хранится в досье (п. 76)';
    END IF;
    NEW.unpublish_at := NEW.published_at + interval '6 months';
  END IF;

  IF NEW.unpublished_at IS NOT NULL AND OLD.unpublished_at IS NULL THEN
    IF NEW.published_at IS NULL THEN
      RAISE EXCEPTION 'INV-076: снимается только опубликованный протокол (п. 76)';
    END IF;
    IF NEW.unpublished_at < NEW.unpublish_at THEN
      RAISE EXCEPTION
        'INV-076: публичный доступ длится 6 месяцев, снятие раньше % запрещено (п. 76)',
        NEW.unpublish_at;
    END IF;
  END IF;

  RETURN NEW;
END $$;

CREATE TRIGGER check_protocol_publication BEFORE UPDATE ON core.protocols
  FOR EACH ROW EXECUTE FUNCTION core.check_protocol_publication();

-- Досье (FR-1602): материал, его вид и источник. Повторное событие не
-- порождает дубля — досье собирается идемпотентно.
ALTER TABLE core.dossier_items
  ADD COLUMN title       text,
  ADD COLUMN occurred_at timestamptz NOT NULL DEFAULT now();

CREATE UNIQUE INDEX dossier_items_source_idx
  ON core.dossier_items (tender_id, kind, source_table, source_id)
  WHERE tender_id IS NOT NULL AND source_id IS NOT NULL;

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.dossier_items
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Досье — доказательная база: материал из него не изымается
CREATE TRIGGER dossier_items_append_only BEFORE DELETE ON core.dossier_items
  FOR EACH ROW EXECUTE FUNCTION core.forbid_mutation('FR-1602');

REVOKE DELETE ON core.dossier_items FROM tou_rent_app;

-- Регистрация материала досье: одна точка входа для всех триггеров-событий
CREATE FUNCTION core.record_dossier_item(
  p_tender_id    uuid,
  p_kind         text,
  p_title        text,
  p_file_key     text,
  p_source_table text,
  p_source_id    uuid
) RETURNS void
LANGUAGE plpgsql AS $$
BEGIN
  IF p_tender_id IS NULL THEN
    RETURN;  -- материал вне тендера (договор из одного источника) — контур 3
  END IF;

  INSERT INTO core.dossier_items
    (tender_id, kind, title, file_key, source_table, source_id, occurred_at)
  VALUES (p_tender_id, p_kind, p_title, p_file_key, p_source_table, p_source_id, now())
  ON CONFLICT (tender_id, kind, source_table, source_id)
    WHERE tender_id IS NOT NULL AND source_id IS NOT NULL
  DO UPDATE SET file_key = coalesce(EXCLUDED.file_key, core.dossier_items.file_key),
                title    = coalesce(EXCLUDED.title, core.dossier_items.title);
END $$;

-- Заявка участника (Прил. 2, 9, 11)
CREATE FUNCTION core.dossier_on_application() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  PERFORM core.record_dossier_item(
    NEW.tender_id, 'application',
    'Заявка ' || coalesce(NEW.applicant_details->>'name', 'участника'),
    NULL, 'core.applications', NEW.id);
  RETURN NULL;
END $$;

CREATE TRIGGER dossier_on_application AFTER INSERT ON core.applications
  FOR EACH ROW EXECUTE FUNCTION core.dossier_on_application();

-- Протокол комиссии и факт его публикации (п. 55, 73–76)
CREATE FUNCTION core.dossier_on_protocol() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  PERFORM core.record_dossier_item(
    NEW.tender_id, 'protocol',
    'Протокол ' || NEW.kind::text || coalesce(' №' || NEW.number, ''),
    NEW.pdf_key, 'core.protocols', NEW.id);

  IF NEW.published_at IS NOT NULL THEN
    PERFORM core.record_dossier_item(
      NEW.tender_id, 'publication',
      CASE
        WHEN NEW.unpublished_at IS NOT NULL
          THEN 'Публикация протокола ' || NEW.kind::text || ' снята по истечении 6 месяцев (п. 76)'
        ELSE 'Публикация протокола ' || NEW.kind::text || ' (п. 75)'
      END,
      NEW.pdf_key, 'core.protocols', NEW.id);
  END IF;

  RETURN NULL;
END $$;

CREATE TRIGGER dossier_on_protocol AFTER INSERT OR UPDATE ON core.protocols
  FOR EACH ROW EXECUTE FUNCTION core.dossier_on_protocol();

-- Объявление и редакции документации (Прил. 1, п. 5–6, 27)
CREATE FUNCTION core.dossier_on_amendment() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  PERFORM core.record_dossier_item(
    NEW.tender_id, 'announcement',
    'Редакция документации №' || NEW.version || ': ' || NEW.summary,
    NEW.doc_key, 'core.tender_amendments', NEW.id);
  RETURN NULL;
END $$;

CREATE TRIGGER dossier_on_amendment AFTER INSERT OR UPDATE ON core.tender_amendments
  FOR EACH ROW EXECUTE FUNCTION core.dossier_on_amendment();

-- Договор (Прил. 5–6, п. 126)
CREATE FUNCTION core.dossier_on_contract() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  PERFORM core.record_dossier_item(
    NEW.tender_id, 'contract',
    'Договор' || coalesce(' №' || NEW.reg_number, ' (проект)'),
    coalesce(NEW.signed_scan_key, NEW.pdf_key), 'core.contracts', NEW.id);
  RETURN NULL;
END $$;

CREATE TRIGGER dossier_on_contract AFTER INSERT OR UPDATE ON core.contracts
  FOR EACH ROW EXECUTE FUNCTION core.dossier_on_contract();

-- Акты приема-передачи и возврата (Прил. 7–8)
CREATE FUNCTION core.dossier_on_act() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  tender uuid;
BEGIN
  SELECT c.tender_id INTO tender FROM core.contracts c WHERE c.id = NEW.contract_id;
  PERFORM core.record_dossier_item(
    tender, 'act',
    CASE NEW.kind WHEN 'handover' THEN 'Акт приема-передачи' ELSE 'Акт возврата' END
      || ' от ' || to_char(NEW.act_date, 'DD.MM.YYYY'),
    coalesce(NEW.signed_scan_key, NEW.pdf_key), 'core.acts', NEW.id);
  RETURN NULL;
END $$;

CREATE TRIGGER dossier_on_act AFTER INSERT OR UPDATE ON core.acts
  FOR EACH ROW EXECUTE FUNCTION core.dossier_on_act();

-- Уклонение от подписания договора (п. 116)
CREATE FUNCTION core.dossier_on_evasion() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  PERFORM core.record_dossier_item(
    NEW.tender_id, 'evasion',
    'Уклонение от подписания договора: ' || NEW.ground,
    NULL, 'core.evasions', NEW.id);
  RETURN NULL;
END $$;

CREATE TRIGGER dossier_on_evasion AFTER INSERT ON core.evasions
  FOR EACH ROW EXECUTE FUNCTION core.dossier_on_evasion();

-- Досье уже проведенных тендеров: события прошлого триггеры не видели,
-- поэтому материалы переносятся один раз — теми же правилами (FR-1602).
INSERT INTO core.dossier_items
  (tender_id, kind, title, file_key, source_table, source_id, occurred_at)
SELECT a.tender_id, 'application',
       'Заявка ' || coalesce(a.applicant_details->>'name', 'участника'),
       NULL, 'core.applications', a.id, a.submitted_at
FROM core.applications a
ON CONFLICT (tender_id, kind, source_table, source_id)
  WHERE tender_id IS NOT NULL AND source_id IS NOT NULL
DO NOTHING;

INSERT INTO core.dossier_items
  (tender_id, kind, title, file_key, source_table, source_id, occurred_at)
SELECT p.tender_id, 'protocol',
       'Протокол ' || p.kind::text || coalesce(' №' || p.number, ''),
       p.pdf_key, 'core.protocols', p.id, p.generated_at
FROM core.protocols p
ON CONFLICT (tender_id, kind, source_table, source_id)
  WHERE tender_id IS NOT NULL AND source_id IS NOT NULL
DO NOTHING;

INSERT INTO core.dossier_items
  (tender_id, kind, title, file_key, source_table, source_id, occurred_at)
SELECT p.tender_id, 'publication',
       'Публикация протокола ' || p.kind::text || ' (п. 75)',
       p.pdf_key, 'core.protocols', p.id, p.published_at
FROM core.protocols p
WHERE p.published_at IS NOT NULL
ON CONFLICT (tender_id, kind, source_table, source_id)
  WHERE tender_id IS NOT NULL AND source_id IS NOT NULL
DO NOTHING;

INSERT INTO core.dossier_items
  (tender_id, kind, title, file_key, source_table, source_id, occurred_at)
SELECT c.tender_id, 'contract',
       'Договор' || coalesce(' №' || c.reg_number, ' (проект)'),
       coalesce(c.signed_scan_key, c.pdf_key), 'core.contracts', c.id, c.created_at
FROM core.contracts c
WHERE c.tender_id IS NOT NULL
ON CONFLICT (tender_id, kind, source_table, source_id)
  WHERE tender_id IS NOT NULL AND source_id IS NOT NULL
DO NOTHING;

INSERT INTO core.dossier_items
  (tender_id, kind, title, file_key, source_table, source_id, occurred_at)
SELECT c.tender_id, 'act',
       CASE a.kind WHEN 'handover' THEN 'Акт приема-передачи' ELSE 'Акт возврата' END
         || ' от ' || to_char(a.act_date, 'DD.MM.YYYY'),
       coalesce(a.signed_scan_key, a.pdf_key), 'core.acts', a.id, a.created_at
FROM core.acts a
JOIN core.contracts c ON c.id = a.contract_id
WHERE c.tender_id IS NOT NULL
ON CONFLICT (tender_id, kind, source_table, source_id)
  WHERE tender_id IS NOT NULL AND source_id IS NOT NULL
DO NOTHING;

INSERT INTO core.dossier_items
  (tender_id, kind, title, file_key, source_table, source_id, occurred_at)
SELECT a.tender_id, 'announcement',
       'Редакция документации №' || a.version || ': ' || a.summary,
       a.doc_key, 'core.tender_amendments', a.id, a.created_at
FROM core.tender_amendments a
ON CONFLICT (tender_id, kind, source_table, source_id)
  WHERE tender_id IS NOT NULL AND source_id IS NOT NULL
DO NOTHING;

INSERT INTO core.dossier_items
  (tender_id, kind, title, file_key, source_table, source_id, occurred_at)
SELECT e.tender_id, 'evasion',
       'Уклонение от подписания договора: ' || e.ground,
       NULL, 'core.evasions', e.id, e.declared_at
FROM core.evasions e
WHERE e.tender_id IS NOT NULL
ON CONFLICT (tender_id, kind, source_table, source_id)
  WHERE tender_id IS NOT NULL AND source_id IS NOT NULL
DO NOTHING;
