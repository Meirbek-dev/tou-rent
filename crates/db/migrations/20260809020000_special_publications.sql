-- Публикации особого порядка (М12, М14: FR-1403, FR-1202, п. 90, 92, 97).
--
-- Правила требуют публиковать три вещи раздела 12: результат рассмотрения
-- заявки с обоснованием (п. 90, 97), обоснование ставки договора (п. 97)
-- и акт приемки инвестиций (п. 92). Механизм — тот же, что у протоколов
-- (FR-702, INV-076): публикуется сформированный документ, публичный доступ
-- длится шесть месяцев, снятие выполняет джоб, а материал остается в досье
-- решения (FR-1206, T38).
--
-- Отличие от протокола — в том, что публикация здесь отдельная запись,
-- а не состояние документа: у решения, ставки и акта разные источники,
-- и «опубликовать» для них означает выложить на портал именно материал,
-- а не открыть саму запись процесса. Публикация однократна: повторно
-- выложить снятый материал нельзя (A-058, тот же ход, что у протоколов).
--
-- Публикуемость объявляет категория заявки (INV-087, `publishable`):
-- по непубликуемой категории результат на портал не попадает.

CREATE TYPE core.public_record_kind AS ENUM ('decision', 'rate', 'investment_act');

-- Обоснование ставки договора особого порядка (п. 97, FR-201): расчет
-- Прил. 4 замораживается при составлении договора — тем же снимком, что
-- и у лота (`core.lots.rate_calculation`). Без него публиковать нечего.
ALTER TABLE core.investment_contracts ADD COLUMN rate_calculation jsonb;

COMMENT ON COLUMN core.investment_contracts.rate_calculation IS
  'FR-1403 (п. 97): снимок расчета ставки Прил. 4 — публикуемое обоснование';

CREATE TABLE core.public_records (
  id                 uuid                    PRIMARY KEY DEFAULT uuidv7(),
  kind               core.public_record_kind NOT NULL,
  -- Предмет публикации: у решения — заявка, у обоснования ставки — договор,
  -- у акта — сама приемка. Досье у всех троих одно — досье решения (T38).
  special_request_id uuid                    REFERENCES core.special_requests (id),
  contract_id        uuid                    REFERENCES core.contracts (id),
  acceptance_id      uuid                    REFERENCES core.investment_acceptances (id),
  title              text                    NOT NULL,
  -- Печатная форма (решение, акт) в бакете dossiers либо расчет (ставка)
  file_key           text,
  payload            jsonb                   NOT NULL DEFAULT '{}',
  published_at       timestamptz             NOT NULL DEFAULT now(),
  -- Момент автоматического снятия: публикация + 6 месяцев (INV-076)
  unpublish_at       timestamptz             NOT NULL DEFAULT now(),
  unpublished_at     timestamptz,
  published_by       uuid                    NOT NULL REFERENCES core.users (id),
  CONSTRAINT public_record_title_not_empty CHECK (length(btrim(title)) > 0),
  CONSTRAINT public_record_subject CHECK (
    CASE kind
      WHEN 'decision'       THEN special_request_id IS NOT NULL
                                 AND contract_id IS NULL AND acceptance_id IS NULL
      WHEN 'rate'           THEN contract_id IS NOT NULL
                                 AND special_request_id IS NULL AND acceptance_id IS NULL
      WHEN 'investment_act' THEN acceptance_id IS NOT NULL
                                 AND special_request_id IS NULL AND contract_id IS NULL
    END
  )
);

COMMENT ON TABLE core.public_records IS
  'FR-1403 (п. 90, 92, 97): публикации особого порядка — результат, обоснование ставки, акт приемки';

-- Публикация однократна: один материал — одна публикация (п. 97)
CREATE UNIQUE INDEX public_records_decision_idx
  ON core.public_records (special_request_id) WHERE special_request_id IS NOT NULL;
CREATE UNIQUE INDEX public_records_rate_idx
  ON core.public_records (contract_id) WHERE contract_id IS NOT NULL;
CREATE UNIQUE INDEX public_records_act_idx
  ON core.public_records (acceptance_id) WHERE acceptance_id IS NOT NULL;

-- Портал выбирает публичные материалы, свежие сверху (FR-1403)
CREATE INDEX public_records_public_idx
  ON core.public_records (published_at DESC) WHERE unpublished_at IS NULL;

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.public_records
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Публикация — юридический факт: запись не удаляется (п. 97, FR-1602)
CREATE TRIGGER public_records_no_delete BEFORE DELETE ON core.public_records
  FOR EACH ROW EXECUTE FUNCTION core.forbid_mutation('FR-1403');

REVOKE DELETE ON core.public_records FROM tou_rent_app;

-- Правила публикации: что публикуется, по какой категории и на какой срок.
-- Все проверки в БД, потому что это условия Правил, а не экрана.
CREATE FUNCTION core.check_public_record() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  publishable boolean;
  has_pdf     boolean;
BEGIN
  IF TG_OP = 'UPDATE' THEN
    -- Момент публикации и предмет не переписываются, снятие необратимо
    IF NEW.kind IS DISTINCT FROM OLD.kind
       OR NEW.special_request_id IS DISTINCT FROM OLD.special_request_id
       OR NEW.contract_id IS DISTINCT FROM OLD.contract_id
       OR NEW.acceptance_id IS DISTINCT FROM OLD.acceptance_id
       OR NEW.published_at IS DISTINCT FROM OLD.published_at
       OR NEW.unpublish_at IS DISTINCT FROM OLD.unpublish_at THEN
      RAISE EXCEPTION 'FR-1403: публикация особого порядка неизменяема (п. 97)';
    END IF;

    IF OLD.unpublished_at IS NOT NULL AND NEW.unpublished_at IS DISTINCT FROM OLD.unpublished_at THEN
      RAISE EXCEPTION
        'INV-076: снятие публикации необратимо — материал хранится в досье (п. 76)';
    END IF;

    IF NEW.unpublished_at IS NOT NULL AND OLD.unpublished_at IS NULL
       AND NEW.unpublished_at < NEW.unpublish_at THEN
      RAISE EXCEPTION
        'INV-076: публичный доступ длится 6 месяцев, снятие раньше % запрещено (п. 76)',
        NEW.unpublish_at;
    END IF;

    RETURN NEW;
  END IF;

  -- Срок публичного доступа считает БД, а не вызывающий код (INV-076)
  NEW.unpublish_at := NEW.published_at + interval '6 months';

  IF NEW.kind = 'decision' THEN
    SELECT c.publishable INTO publishable
    FROM core.special_requests r
    JOIN refdata.special_categories c ON c.code = r.category
    WHERE r.id = NEW.special_request_id;

    IF NOT coalesce(publishable, false) THEN
      RAISE EXCEPTION
        'FR-1403: категория заявки не публикуется (п. 87, 97)'
        USING ERRCODE = 'raise_exception';
    END IF;

    -- Публикуется результат состоявшегося рассмотрения с печатной формой:
    -- решения без протокола на портале не бывает (тот же ход, что FR-702)
    SELECT d.pdf_key IS NOT NULL INTO has_pdf
    FROM core.special_board_decisions d
    WHERE d.special_request_id = NEW.special_request_id;

    IF has_pdf IS NULL THEN
      RAISE EXCEPTION
        'FR-1403: публикуется результат принятого решения (п. 90, 97)'
        USING ERRCODE = 'raise_exception';
    END IF;
    IF NOT has_pdf THEN
      RAISE EXCEPTION
        'FR-1403: протокол решения не сформирован — публиковать нечего (п. 97)'
        USING ERRCODE = 'raise_exception';
    END IF;
  END IF;

  IF NEW.kind = 'investment_act' AND NEW.file_key IS NULL THEN
    RAISE EXCEPTION
      'FR-1403: печатная форма акта приемки не сформирована (п. 92)'
      USING ERRCODE = 'raise_exception';
  END IF;

  IF NEW.kind = 'rate' AND NEW.payload = '{}'::jsonb THEN
    RAISE EXCEPTION
      'FR-1403: обоснование ставки публикуется расчетом Прил. 4 (п. 97)'
      USING ERRCODE = 'raise_exception';
  END IF;

  RETURN NEW;
END $$;

CREATE TRIGGER check_public_record BEFORE INSERT OR UPDATE ON core.public_records
  FOR EACH ROW EXECUTE FUNCTION core.check_public_record();

-- Публикация ложится в досье решения (FR-1206, T38): у обоснования ставки
-- и акта заявка достается через инвестиционный договор.
CREATE FUNCTION core.dossier_on_public_record() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  request uuid;
BEGIN
  request := CASE NEW.kind
    WHEN 'decision' THEN NEW.special_request_id
    WHEN 'rate' THEN (SELECT i.special_request_id FROM core.investment_contracts i
                      WHERE i.contract_id = NEW.contract_id)
    WHEN 'investment_act' THEN (SELECT i.special_request_id
                                FROM core.investment_acceptances a
                                JOIN core.investment_contracts i ON i.contract_id = a.contract_id
                                WHERE a.id = NEW.acceptance_id)
  END;

  PERFORM core.record_dossier_item(
    NULL, 'publication',
    CASE
      WHEN NEW.unpublished_at IS NOT NULL
        THEN NEW.title || ' — публикация снята по истечении 6 месяцев (п. 76)'
      ELSE NEW.title || ' — опубликовано (' || NEW.kind::text || ')'
    END,
    NEW.file_key, 'core.public_records', NEW.id, request);
  RETURN NULL;
END $$;

CREATE TRIGGER dossier_on_public_record AFTER INSERT OR UPDATE ON core.public_records
  FOR EACH ROW EXECUTE FUNCTION core.dossier_on_public_record();
