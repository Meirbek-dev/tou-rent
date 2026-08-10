-- Инвестиционные договоры (М12, FR-1204, п. 91–94).
--
-- Инвестиционный договор заключается по решению Правления «предоставить»
-- (п. 90) и живет в общей таблице договоров: объект, наниматель, ставка и
-- период найма у него те же (INV-DB-02 продолжает защищать объект). Своим
-- у него остается инвестиционная часть — обязательные приложения (п. 91),
-- предельный срок (INV-094), приемка инвестиций и продление (п. 92–93).
--
-- INV-091: договор не подписывается, пока не приложены все обязательные
-- документы п. 91 — тот же прием, что и у сверки перед подписанием (INV-115).
-- INV-094: срок инвестиционного договора не превышает семи лет (п. 94).

-- Обязательные приложения инвестиционного проекта (п. 91). Состав — из ТЗ
-- FR-1204; формулировки уточняются по Правилам (TODO-ENGINEER, Q-014).
CREATE TABLE refdata.investment_attachments (
  code     text PRIMARY KEY,
  ordinal  int  NOT NULL UNIQUE CHECK (ordinal > 0),
  label_ru text NOT NULL,
  label_kk text,
  label_en text,
  rule_ref text NOT NULL
);

INSERT INTO refdata.investment_attachments
  (code, ordinal, label_ru, label_kk, label_en, rule_ref) VALUES
  ('estimate', 1, 'Смета инвестиционного проекта', 'Инвестициялық жобаның сметасы',
   'Project cost estimate', 'п. 91'),
  ('schedule', 2, 'График выполнения работ', 'Жұмыстарды орындау кестесі',
   'Works schedule', 'п. 91'),
  ('appraisal', 3, 'Заключение оценщика', 'Бағалаушының қорытындысы',
   'Appraiser report', 'п. 91'),
  ('guarantee', 4, 'Гарантия исполнения обязательств', 'Міндеттемелердің орындалу кепілдігі',
   'Performance guarantee', 'п. 91')
ON CONFLICT DO NOTHING;

-- Инвестиционная часть договора: снимок решения Правления (FR-901) плюс
-- предельный срок и продление (п. 93–94).
CREATE TABLE core.investment_contracts (
  id                  uuid          PRIMARY KEY DEFAULT uuidv7(),
  contract_id         uuid          NOT NULL UNIQUE REFERENCES core.contracts (id),
  special_request_id  uuid          NOT NULL UNIQUE REFERENCES core.special_requests (id),
  -- Объем инвестиций из заявки: существенное условие, снимок решения (FR-901)
  investment_amount   numeric(14,2) NOT NULL CHECK (investment_amount > 0),
  -- INV-094 (п. 94): срок договора не превышает 7 лет
  term_months         int           NOT NULL CHECK (term_months BETWEEN 1 AND 84),
  -- Однократное продление на 3 года при полном исполнении (п. 93)
  extended_at         timestamptz,
  extension_months    int           CHECK (extension_months IS NULL OR extension_months = 36),
  -- Пролонгация на аналогичный период решением Правления (п. 93, от 100 млн ₸)
  prolonged_at        timestamptz,
  prolongation_months int           CHECK (prolongation_months IS NULL OR prolongation_months > 0),
  created_at          timestamptz   NOT NULL DEFAULT now(),
  updated_at          timestamptz   NOT NULL DEFAULT now(),
  CONSTRAINT extension_has_term CHECK ((extended_at IS NULL) = (extension_months IS NULL)),
  CONSTRAINT prolongation_has_term CHECK ((prolonged_at IS NULL) = (prolongation_months IS NULL))
);

COMMENT ON TABLE core.investment_contracts IS
  'FR-1204 (п. 91–94): инвестиционная часть договора; INV-094 — срок не более 7 лет';

CREATE TRIGGER touch_updated_at BEFORE UPDATE ON core.investment_contracts
  FOR EACH ROW EXECUTE FUNCTION core.touch_updated_at();

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.investment_contracts
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Договор заключается по удовлетворенной заявке инвестиционной категории:
-- «предоставить» — единственное основание (п. 90–91).
CREATE FUNCTION core.check_investment_contract() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  request_status core.special_request_status;
BEGIN
  SELECT status INTO request_status
  FROM core.special_requests WHERE id = NEW.special_request_id;

  IF request_status IS DISTINCT FROM 'granted' THEN
    RAISE EXCEPTION
      'FR-1204: инвестиционный договор заключается по удовлетворенной заявке особого порядка (п. 90–91)'
      USING ERRCODE = 'raise_exception';
  END IF;

  RETURN NEW;
END $$;

CREATE TRIGGER check_investment_contract BEFORE INSERT ON core.investment_contracts
  FOR EACH ROW EXECUTE FUNCTION core.check_investment_contract();

-- Приложения договора (п. 91): каждое закрывает позицию закрытого перечня.
CREATE TABLE core.investment_contract_files (
  id           uuid        PRIMARY KEY DEFAULT uuidv7(),
  contract_id  uuid        NOT NULL REFERENCES core.contracts (id) ON DELETE CASCADE,
  code         text        NOT NULL REFERENCES refdata.investment_attachments (code),
  file_key     text        NOT NULL,
  filename     text        NOT NULL,
  content_type text        NOT NULL,
  size_bytes   bigint      NOT NULL CHECK (size_bytes >= 0),
  uploaded_at  timestamptz NOT NULL DEFAULT now(),
  UNIQUE (contract_id, code)
);

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.investment_contract_files
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- INV-091: без полного комплекта приложений договор не подписывается.
-- Проверка стоит на переходе в signing — там же, где INV-115 у обычного
-- договора: до подписания комплект можно досылать.
CREATE FUNCTION core.check_investment_attachments() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  missing int;
BEGIN
  IF NEW.status <> 'signing' OR OLD.status = 'signing' THEN
    RETURN NEW;
  END IF;

  IF NOT EXISTS (SELECT 1 FROM core.investment_contracts i WHERE i.contract_id = NEW.id) THEN
    RETURN NEW;  -- обычный договор тендера: у него свой перечень (п. 113)
  END IF;

  SELECT count(*) INTO missing
  FROM refdata.investment_attachments a
  WHERE NOT EXISTS (
    SELECT 1 FROM core.investment_contract_files f
    WHERE f.contract_id = NEW.id AND f.code = a.code
  );

  IF missing > 0 THEN
    RAISE EXCEPTION
      'INV-091: не приложены обязательные документы инвестиционного проекта (%), п. 91',
      missing
      USING ERRCODE = 'raise_exception';
  END IF;

  RETURN NEW;
END $$;

CREATE TRIGGER check_investment_attachments BEFORE UPDATE ON core.contracts
  FOR EACH ROW EXECUTE FUNCTION core.check_investment_attachments();

-- Приемка инвестиций комиссией (п. 92): акт фиксирует принятый объем.
CREATE TABLE core.investment_acceptances (
  id              uuid          PRIMARY KEY DEFAULT uuidv7(),
  contract_id     uuid          NOT NULL REFERENCES core.contracts (id),
  act_date        date          NOT NULL,
  accepted_amount numeric(14,2) NOT NULL CHECK (accepted_amount > 0),
  note            text,
  accepted_by     uuid          NOT NULL REFERENCES core.users (id),
  pdf_key         text,
  created_at      timestamptz   NOT NULL DEFAULT now()
);

CREATE INDEX investment_acceptances_contract_idx
  ON core.investment_acceptances (contract_id, act_date);

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.investment_acceptances
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Акт приемки — юридический факт: его не переписывают (п. 92)
CREATE FUNCTION core.freeze_investment_acceptance() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.contract_id     IS DISTINCT FROM OLD.contract_id
     OR NEW.act_date        IS DISTINCT FROM OLD.act_date
     OR NEW.accepted_amount IS DISTINCT FROM OLD.accepted_amount
     OR NEW.accepted_by     IS DISTINCT FROM OLD.accepted_by THEN
    RAISE EXCEPTION 'FR-1204: акт приемки инвестиций неизменяем (п. 92)';
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER freeze_investment_acceptance BEFORE UPDATE ON core.investment_acceptances
  FOR EACH ROW EXECUTE FUNCTION core.freeze_investment_acceptance();

CREATE TRIGGER investment_acceptances_no_delete BEFORE DELETE ON core.investment_acceptances
  FOR EACH ROW EXECUTE FUNCTION core.forbid_mutation('FR-1204');

REVOKE DELETE ON core.investment_acceptances FROM tou_rent_app;

-- Принятый объем инвестиций по договору (п. 92–93)
CREATE FUNCTION core.investment_accepted(p_contract_id uuid) RETURNS numeric
LANGUAGE sql STABLE AS $$
  SELECT coalesce(sum(a.accepted_amount), 0)
  FROM core.investment_acceptances a WHERE a.contract_id = p_contract_id;
$$;

-- Продление и пролонгация (п. 93): продление на три года — однократно и
-- только при полном исполнении обязательств от 30 млн ₸; пролонгация —
-- от 100 млн ₸ решением Правления. Пороги и сроки из ТЗ FR-1204.
CREATE FUNCTION core.check_investment_extension() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  accepted numeric;
BEGIN
  -- Оформленное продление — факт: ни момент, ни срок не переписываются
  -- (иначе «однократно» обходится правкой строки)
  IF OLD.extended_at IS NOT NULL
     AND (NEW.extended_at IS DISTINCT FROM OLD.extended_at
          OR NEW.extension_months IS DISTINCT FROM OLD.extension_months) THEN
    RAISE EXCEPTION 'FR-1204: продление договора однократно (п. 93)';
  END IF;
  IF OLD.prolonged_at IS NOT NULL
     AND (NEW.prolonged_at IS DISTINCT FROM OLD.prolonged_at
          OR NEW.prolongation_months IS DISTINCT FROM OLD.prolongation_months) THEN
    RAISE EXCEPTION 'FR-1204: пролонгация уже оформлена (п. 93)';
  END IF;

  IF NEW.extended_at IS NULL AND NEW.prolonged_at IS NULL THEN
    RETURN NEW;
  END IF;

  accepted := core.investment_accepted(NEW.contract_id);

  IF accepted < NEW.investment_amount THEN
    RAISE EXCEPTION
      'FR-1204: обязательства исполнены не полностью (принято % из %), п. 93',
      accepted, NEW.investment_amount
      USING ERRCODE = 'raise_exception';
  END IF;

  IF NEW.extended_at IS NOT NULL AND OLD.extended_at IS NULL
     AND NEW.investment_amount < 30000000 THEN
    RAISE EXCEPTION
      'FR-1204: продление на три года — при объеме инвестиций от 30 млн ₸ (п. 93)'
      USING ERRCODE = 'raise_exception';
  END IF;

  IF NEW.prolonged_at IS NOT NULL AND OLD.prolonged_at IS NULL
     AND NEW.investment_amount < 100000000 THEN
    RAISE EXCEPTION
      'FR-1204: пролонгация — при объеме инвестиций от 100 млн ₸ (п. 93)'
      USING ERRCODE = 'raise_exception';
  END IF;

  RETURN NEW;
END $$;

CREATE TRIGGER check_investment_extension BEFORE UPDATE ON core.investment_contracts
  FOR EACH ROW EXECUTE FUNCTION core.check_investment_extension();
