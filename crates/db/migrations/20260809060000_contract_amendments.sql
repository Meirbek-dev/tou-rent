-- Допсоглашения к договору (М9: FR-906, FR-901, п. 125).
--
-- Допсоглашение — отдельная сущность с diff-контролем: оно фиксирует, какое
-- поле договора, с какого значения и на какое меняется. Существенные условия
-- (ставка, объект, срок, наниматель) им не меняются — перечень изменяемых
-- полей закрыт справочником, и защищенного поля в нем нет по построению
-- (FR-901; тот же рубеж стережет триггер `freeze_terms`).
--
-- Правка, попавшая в допсоглашение, — юридический факт: ни она, ни само
-- соглашение не переписываются (п. 125–126). Печатная форма и запись
-- в досье (FR-1602) добавляются вместе с фактом.
--
-- TODO-ENGINEER: п. 125 агенту недоступен (Q-017) — состав изменяемых полей
-- заведомо черновой и заполняется данными справочника без правки кода.

-- Что Правила разрешают менять допсоглашением (п. 125). Паритет с типом
-- домена `contract::ContractField::amendable()` проверяет тест.
CREATE TABLE refdata.amendable_fields (
  code     text PRIMARY KEY,
  ordinal  int  NOT NULL UNIQUE CHECK (ordinal > 0),
  label_ru text NOT NULL,
  label_kk text,
  label_en text,
  rule_ref text NOT NULL
);

COMMENT ON TABLE refdata.amendable_fields IS
  'FR-906 (п. 125): закрытый перечень полей договора, изменяемых допсоглашением; существенных условий в нем нет (FR-901)';

INSERT INTO refdata.amendable_fields (code, ordinal, label_ru, label_kk, label_en, rule_ref)
VALUES
  ('bank_details', 1, 'Банковские реквизиты', 'Банк деректемелері', 'Bank details', 'п. 125'),
  ('contact_details', 2, 'Адрес и контактные данные', 'Мекенжай және байланыс деректері',
   'Address and contact details', 'п. 125'),
  ('representative', 3, 'Уполномоченный представитель', 'Уәкілетті өкіл',
   'Authorised representative', 'п. 125'),
  ('payment_order', 4, 'Порядок внесения платы', 'Төлем енгізу тәртібі',
   'Payment order', 'п. 125')
ON CONFLICT DO NOTHING;

-- Допсоглашение (п. 125): номер в рамках договора, основание и дата вступления
CREATE TABLE core.contract_amendments (
  id           uuid        PRIMARY KEY DEFAULT uuidv7(),
  contract_id  uuid        NOT NULL REFERENCES core.contracts (id),
  seq          int         NOT NULL CHECK (seq > 0),
  ground       text        NOT NULL,
  effective_on date        NOT NULL,
  -- Печатная форма (Typst) в бакете dossiers
  pdf_key      text,
  created_by   uuid        NOT NULL REFERENCES core.users (id),
  created_at   timestamptz NOT NULL DEFAULT now(),
  UNIQUE (contract_id, seq),
  CONSTRAINT amendment_ground_not_empty CHECK (length(btrim(ground)) > 0)
);

COMMENT ON TABLE core.contract_amendments IS
  'FR-906 (п. 125): допсоглашение к договору — diff защищенных полей проверяет core.check_amendment_change';

CREATE INDEX contract_amendments_contract_idx
  ON core.contract_amendments (contract_id, seq);

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.contract_amendments
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Допсоглашение заключается к заключенному договору (п. 126): до регистрации
-- меняется сам договор, а не соглашение к нему.
CREATE FUNCTION core.check_contract_amendment() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  registered timestamptz;
  status     core.contract_status;
BEGIN
  SELECT c.registered_at, c.status INTO registered, status
  FROM core.contracts c WHERE c.id = NEW.contract_id;

  IF registered IS NULL THEN
    RAISE EXCEPTION
      'FR-906: допсоглашение заключается к зарегистрированному договору (п. 126)'
      USING ERRCODE = 'raise_exception';
  END IF;

  IF status IN ('terminated', 'cancelled', 'completed') THEN
    RAISE EXCEPTION
      'FR-906: договор в состоянии % — допсоглашение к нему не заключается (п. 125)', status
      USING ERRCODE = 'raise_exception';
  END IF;

  RETURN NEW;
END $$;

CREATE TRIGGER check_contract_amendment BEFORE INSERT ON core.contract_amendments
  FOR EACH ROW EXECUTE FUNCTION core.check_contract_amendment();

-- Само соглашение неизменяемо: догружается только печатная форма (п. 125)
CREATE FUNCTION core.freeze_contract_amendment() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.contract_id  IS DISTINCT FROM OLD.contract_id
     OR NEW.seq          IS DISTINCT FROM OLD.seq
     OR NEW.ground       IS DISTINCT FROM OLD.ground
     OR NEW.effective_on IS DISTINCT FROM OLD.effective_on
     OR NEW.created_by   IS DISTINCT FROM OLD.created_by
     OR NEW.created_at   IS DISTINCT FROM OLD.created_at THEN
    RAISE EXCEPTION 'FR-906: допсоглашение неизменяемо (п. 125)';
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER freeze_contract_amendment BEFORE UPDATE ON core.contract_amendments
  FOR EACH ROW EXECUTE FUNCTION core.freeze_contract_amendment();

CREATE TRIGGER contract_amendments_no_delete BEFORE DELETE ON core.contract_amendments
  FOR EACH ROW EXECUTE FUNCTION core.forbid_mutation('FR-906');

REVOKE DELETE ON core.contract_amendments FROM tou_rent_app;

-- Diff: какое поле, с какого значения и на какое меняется (FR-906)
CREATE TABLE core.contract_amendment_changes (
  id           uuid PRIMARY KEY DEFAULT uuidv7(),
  amendment_id uuid NOT NULL REFERENCES core.contract_amendments (id),
  field_code   text NOT NULL REFERENCES refdata.amendable_fields (code),
  old_value    text NOT NULL,
  new_value    text NOT NULL,
  UNIQUE (amendment_id, field_code),
  CONSTRAINT amendment_change_is_a_change CHECK (btrim(old_value) <> btrim(new_value))
);

COMMENT ON TABLE core.contract_amendment_changes IS
  'FR-906 (п. 125): правки допсоглашения; FK на закрытый перечень не пускает существенные условия (FR-901)';

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.contract_amendment_changes
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Правка — юридический факт: ее не переписывают и не удаляют (п. 125)
CREATE TRIGGER amendment_changes_append_only
  BEFORE UPDATE OR DELETE ON core.contract_amendment_changes
  FOR EACH ROW EXECUTE FUNCTION core.forbid_mutation('FR-906');

REVOKE UPDATE, DELETE ON core.contract_amendment_changes FROM tou_rent_app;

-- FR-901: существенное условие не меняется допсоглашением. Первый рубеж —
-- FK на закрытый перечень, второй — явный отказ по имени поля: если перечень
-- когда-нибудь пополнят существенным условием, правило устоит.
CREATE FUNCTION core.check_amendment_change() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.field_code IN ('monthly_rate', 'object', 'lease_term', 'tenant') THEN
    RAISE EXCEPTION
      'FR-901: существенное условие договора (%) допсоглашением не меняется (п. 108, 125)',
      NEW.field_code
      USING ERRCODE = 'raise_exception';
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER check_amendment_change BEFORE INSERT ON core.contract_amendment_changes
  FOR EACH ROW EXECUTE FUNCTION core.check_amendment_change();

-- Допсоглашение ложится в досье (FR-1602): у договора тендера — в досье
-- тендера, у договора особого порядка — в досье решения (T38).
CREATE FUNCTION core.dossier_on_contract_amendment() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  tender  uuid;
  request uuid;
BEGIN
  SELECT c.tender_id INTO tender FROM core.contracts c WHERE c.id = NEW.contract_id;

  IF tender IS NULL THEN
    SELECT i.special_request_id INTO request
    FROM core.investment_contracts i WHERE i.contract_id = NEW.contract_id;
  END IF;

  PERFORM core.record_dossier_item(
    tender, 'amendment',
    'Допсоглашение №' || NEW.seq || ' от ' || to_char(NEW.effective_on, 'DD.MM.YYYY')
      || ': ' || NEW.ground,
    NEW.pdf_key, 'core.contract_amendments', NEW.id, request);
  RETURN NULL;
END $$;

CREATE TRIGGER dossier_on_contract_amendment
  AFTER INSERT OR UPDATE ON core.contract_amendments
  FOR EACH ROW EXECUTE FUNCTION core.dossier_on_contract_amendment();
