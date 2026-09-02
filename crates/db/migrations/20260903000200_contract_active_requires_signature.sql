-- INV-115: договор вступает в силу только подписанным наймодателем
-- (п. 113–115, 126).
--
-- Круг 2 гаунтлета (G-044). Сторож `core.enforce_checklist_before_signing`
-- стережет ровно один момент - простановку `landlord_signed_at`:
--
--   IF NEW.landlord_signed_at IS NULL OR OLD.landlord_signed_at IS NOT NULL
--     THEN RETURN NEW;   -- договор без подписи проходит мимо целиком
--
-- Поэтому неподписанный договор переводился в действующий одной строкой:
--
--   UPDATE core.contracts SET status = 'active' WHERE id = ...;  -- UPDATE 1
--   INSERT INTO core.contracts (..., status) VALUES (..., 'active');
--
-- След в порядке п. 113–115 - сверка, подпись, регистрация, действие -
-- держался тем, что штатная дорога идет через акт приема-передачи
-- (`core.apply_act_effects`), а тот требует регистрации, а регистрация -
-- обеих подписей (FR-905). Все три звена проверяются, но ни одно не
-- проверяет само состояние: строка, попавшая в `active` мимо акта,
-- законна для БД. Объект при этом считается сданным
-- (`core.object_statuses`), а начисления идут по договору, которого
-- наймодатель не подписывал.
--
-- Рубеж ставится на само состояние: `active` и `completed` требуют
-- подписи наймодателя и завершенной сверки. Проверка идет на входе
-- в состояние (INSERT либо смена статуса) - у строки, которая уже
-- действует, правило проверено при входе, и перепроверять его на
-- каждой правке незачем.
--
-- `terminated` и `cancelled` намеренно вне правила: договор прекращают
-- и до подписания (уклонение победителя, FR-903, п. 116-117), и такой
-- договор обязан уметь закрыться.
--
-- Имя инварианта то же, что у сверки (INV-115): правило одно - договор
-- без пройденной п. 113 и подписи наймодателя не имеет силы, - и
-- причина перечня у него одна (`document_check_incomplete`).
--
-- Имя триггера начинается на `enforce_`, чтобы он срабатывал после
-- `check_contract_registration` (BEFORE-триггеры строки идут по имени):
-- договор, рожденный сразу зарегистрированным, обязан по-прежнему
-- получать отказ FR-905, а не INV-115.

CREATE OR REPLACE FUNCTION core.enforce_signed_before_active() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  total   integer;
  checked integer;
BEGIN
  IF NEW.status NOT IN ('active', 'completed') THEN
    RETURN NEW;
  END IF;

  -- Строка уже действовала: правило проверено при входе в состояние
  IF TG_OP = 'UPDATE' AND OLD.status = NEW.status THEN
    RETURN NEW;
  END IF;

  IF NEW.landlord_signed_at IS NULL THEN
    RAISE EXCEPTION
      'INV-115: договор без подписи наймодателя не вступает в силу (п. 113–115)'
      USING ERRCODE = 'check_violation';
  END IF;

  SELECT count(*), count(*) FILTER (WHERE checked_at IS NOT NULL)
  INTO total, checked
  FROM core.contract_checklists WHERE contract_id = NEW.id;

  IF total = 0 THEN
    RAISE EXCEPTION 'INV-115: чек-лист сверки документов не сформирован (п. 113)'
      USING ERRCODE = 'check_violation';
  END IF;
  IF checked < total THEN
    RAISE EXCEPTION
      'INV-115: сверка документов не завершена (%/% позиций) — договор не вступает в силу (п. 113, 115)',
      checked, total
      USING ERRCODE = 'check_violation';
  END IF;

  RETURN NEW;
END $$;

DROP TRIGGER IF EXISTS enforce_signed_before_active ON core.contracts;
CREATE TRIGGER enforce_signed_before_active BEFORE INSERT OR UPDATE ON core.contracts
  FOR EACH ROW EXECUTE FUNCTION core.enforce_signed_before_active();
