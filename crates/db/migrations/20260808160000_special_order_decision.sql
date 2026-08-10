-- Проверка подразделением и решение Правления (М12, FR-1202, п. 89–90, 97).
--
-- Порядок Правил: заявка → проверка уполномоченным подразделением в срок
-- категории (15 календарных дней, для отдельных категорий 10 рабочих —
-- FR-1201) → заключение → решение Правления в 10 рабочих дней из закрытого
-- перечня «предоставить / отказать / направить в общий порядок».
--
-- INV-090: решение Правления невозможно без заключения подразделения.
-- Закреплено дважды: заключение — единственный путь заявки в состояние
-- `under_review` (без него решение не пройдет проверку переходов T33),
-- и триггер решения требует заключение явно.
--
-- Состояние `under_review` читается как «проверка проведена, заявка вынесена
-- на рассмотрение Правления»: до заключения заявка остается `submitted`,
-- и по ней идет срок проверки (A-068).

CREATE TYPE core.special_decision AS ENUM ('grant', 'refuse', 'redirect');

-- Заключение уполномоченного подразделения (п. 89): одно на заявку.
CREATE TABLE core.special_reviews (
  id                 uuid                  PRIMARY KEY DEFAULT uuidv7(),
  special_request_id uuid                  NOT NULL UNIQUE REFERENCES core.special_requests (id),
  reviewer_id        uuid                  NOT NULL REFERENCES core.users (id),
  conclusion         text                  NOT NULL,
  -- Вывод подразделения — из того же перечня, что и решение Правления (п. 90)
  recommendation     core.special_decision NOT NULL,
  created_at         timestamptz           NOT NULL DEFAULT now(),
  CONSTRAINT special_review_conclusion_not_empty CHECK (length(btrim(conclusion)) > 0)
);

COMMENT ON TABLE core.special_reviews IS
  'FR-1202 (п. 89): заключение уполномоченного подразделения — вход решения Правления (INV-090)';

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.special_reviews
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Заключение выносит заявку на рассмотрение Правления (п. 89–90):
-- следствие применяет БД, а не вызывающий код.
CREATE FUNCTION core.special_review_effects() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  UPDATE core.special_requests
     SET status = 'under_review'
   WHERE id = NEW.special_request_id AND status = 'submitted';

  IF NOT FOUND THEN
    RAISE EXCEPTION
      'FR-1202: заключение выносится по поданной заявке (п. 89)'
      USING ERRCODE = 'raise_exception';
  END IF;

  RETURN NULL;
END $$;

CREATE TRIGGER special_review_effects AFTER INSERT ON core.special_reviews
  FOR EACH ROW EXECUTE FUNCTION core.special_review_effects();

-- Заключение — юридический факт: его не переписывают и не удаляют (п. 97)
CREATE TRIGGER special_reviews_append_only BEFORE UPDATE OR DELETE ON core.special_reviews
  FOR EACH ROW EXECUTE FUNCTION core.forbid_mutation('FR-1202');

REVOKE UPDATE, DELETE ON core.special_reviews FROM tou_rent_app;

-- Решение Правления (п. 90): одно на заявку, с обоснованием — оно публикуется
-- вместе с результатом (п. 97, FR-1403).
CREATE TABLE core.special_board_decisions (
  id                 uuid                  PRIMARY KEY DEFAULT uuidv7(),
  special_request_id uuid                  NOT NULL UNIQUE REFERENCES core.special_requests (id),
  decision           core.special_decision NOT NULL,
  rationale          text                  NOT NULL,
  decided_by         uuid                  NOT NULL REFERENCES core.users (id),
  decided_at         timestamptz           NOT NULL DEFAULT now(),
  -- Протокол решения (Typst-PDF) в бакете dossiers
  pdf_key            text,
  CONSTRAINT special_decision_rationale_not_empty CHECK (length(btrim(rationale)) > 0)
);

COMMENT ON TABLE core.special_board_decisions IS
  'FR-1202 (п. 90): решение Правления из закрытого перечня с обоснованием; INV-090 — только по заключению';

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.special_board_decisions
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- INV-090 и следствие решения: без заключения подразделения решения нет,
-- а принятое решение переводит заявку в свое терминальное состояние (п. 90).
CREATE FUNCTION core.special_decision_effects() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  next_status core.special_request_status;
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM core.special_reviews r WHERE r.special_request_id = NEW.special_request_id
  ) THEN
    RAISE EXCEPTION
      'INV-090: решение Правления невозможно без заключения подразделения (п. 89–90)'
      USING ERRCODE = 'raise_exception';
  END IF;

  next_status := CASE NEW.decision
    WHEN 'grant'    THEN 'granted'
    WHEN 'refuse'   THEN 'refused'
    WHEN 'redirect' THEN 'redirected'
  END::core.special_request_status;

  UPDATE core.special_requests
     SET status = next_status
   WHERE id = NEW.special_request_id AND status = 'under_review';

  IF NOT FOUND THEN
    RAISE EXCEPTION
      'FR-1202: решение принимается по заявке, вынесенной на рассмотрение Правления (п. 90)'
      USING ERRCODE = 'raise_exception';
  END IF;

  RETURN NULL;
END $$;

CREATE TRIGGER special_decision_effects AFTER INSERT ON core.special_board_decisions
  FOR EACH ROW EXECUTE FUNCTION core.special_decision_effects();

-- Решение не пересматривается правкой строки: печатная форма догружается,
-- остальное неизменяемо (п. 90, 97).
CREATE FUNCTION core.freeze_special_decision() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.special_request_id IS DISTINCT FROM OLD.special_request_id
     OR NEW.decision   IS DISTINCT FROM OLD.decision
     OR NEW.rationale  IS DISTINCT FROM OLD.rationale
     OR NEW.decided_by IS DISTINCT FROM OLD.decided_by
     OR NEW.decided_at IS DISTINCT FROM OLD.decided_at THEN
    RAISE EXCEPTION 'FR-1202: решение Правления неизменяемо (п. 90)';
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER freeze_special_decision BEFORE UPDATE ON core.special_board_decisions
  FOR EACH ROW EXECUTE FUNCTION core.freeze_special_decision();

CREATE TRIGGER special_decisions_no_delete BEFORE DELETE ON core.special_board_decisions
  FOR EACH ROW EXECUTE FUNCTION core.forbid_mutation('FR-1202');

REVOKE DELETE ON core.special_board_decisions FROM tou_rent_app;

-- Сроки особого порядка (FR-1702): у обязательства появляется еще один
-- предмет — заявка. Ключ идемпотентности расширяется вместе с ним.
ALTER TABLE core.obligations
  ADD COLUMN special_request_id uuid REFERENCES core.special_requests (id);

ALTER TABLE core.obligations
  DROP CONSTRAINT obligations_unique_per_subject;

ALTER TABLE core.obligations
  ADD CONSTRAINT obligations_unique_per_subject
  UNIQUE NULLS NOT DISTINCT
    (action, tender_id, contract_id, application_id, special_request_id);

COMMENT ON COLUMN core.obligations.special_request_id IS
  'FR-1202: предмет срока особого порядка — заявка (п. 89–90)';
