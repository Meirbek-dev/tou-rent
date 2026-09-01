-- Сторожа состояния должны срабатывать и на INSERT (INV-076, INV-115, FR-905).
--
-- Класс дефекта, найденный в круге 2 гаунтлета: все три сторожа объявлены
-- `BEFORE UPDATE`, поэтому строка, рожденная сразу в конечном состоянии,
-- проходит мимо них целиком. Пробы в одноразовой БД:
--
--   INSERT INTO core.contracts (..., status, tenant_signed_at,
--     landlord_signed_at, registered_at, reg_number)
--     VALUES (..., 'active', now, now, now, 'REG-1');   -- INSERT 0 1,
--     чек-лист сверки: 0 позиций
--   INSERT INTO core.protocols (..., published_at, unpublish_at)
--     VALUES (..., now, now + '10 years');              -- окно 10 лет
--     (через UPDATE тот же протокол получает ровно 6 месяцев)
--
-- Через UPDATE все три отбиваются штатно - то есть инвариант был не
-- «запрещено», а «запрещено, если идти правильной дорогой».
--
-- Правка минимальная: те же функции, тот же текст правил, но с ветвью
-- для INSERT (обращаться к OLD на INSERT нельзя - запись не назначена,
-- поэтому ветвление по TG_OP, а не coalesce по полям OLD).
--
-- Заодно `core.now()` вместо `now()` в отметке завершения сверки: единый
-- источник времени (ADR-0005), сторожевой тест смотрит на умолчания
-- колонок и не видел этой строки внутри тела функции.

-- --- INV-076: публикация протокола ----------------------------------------
CREATE OR REPLACE FUNCTION core.check_protocol_publication() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'INSERT' THEN
    IF NEW.published_at IS NOT NULL THEN
      IF NEW.pdf_key IS NULL THEN
        RAISE EXCEPTION
          'FR-702: печатная форма протокола не сформирована - публиковать нечего (п. 75)';
      END IF;
      -- Срок публичного доступа считает система, а не автор строки
      NEW.unpublish_at := NEW.published_at + interval '6 months';
    END IF;
    IF NEW.unpublished_at IS NOT NULL THEN
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
  END IF;

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

DROP TRIGGER IF EXISTS check_protocol_publication ON core.protocols;
CREATE TRIGGER check_protocol_publication BEFORE INSERT OR UPDATE ON core.protocols
  FOR EACH ROW EXECUTE FUNCTION core.check_protocol_publication();

-- --- INV-115: подпись только после завершенной сверки ----------------------
CREATE OR REPLACE FUNCTION core.enforce_checklist_before_signing() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  total   integer;
  checked integer;
BEGIN
  IF NEW.landlord_signed_at IS NULL THEN
    RETURN NEW;
  END IF;
  -- На UPDATE правило касается только момента подписания
  IF TG_OP = 'UPDATE' AND OLD.landlord_signed_at IS NOT NULL THEN
    RETURN NEW;
  END IF;

  SELECT count(*), count(*) FILTER (WHERE checked_at IS NOT NULL)
  INTO total, checked
  FROM core.contract_checklists WHERE contract_id = NEW.id;

  IF total = 0 THEN
    RAISE EXCEPTION 'INV-115: чек-лист сверки документов не сформирован (п. 113)';
  END IF;
  IF checked < total THEN
    RAISE EXCEPTION
      'INV-115: сверка документов не завершена (%/% позиций) — договор не подписывается (п. 113, 115)',
      checked, total;
  END IF;

  NEW.checklist_done_at := coalesce(NEW.checklist_done_at, core.now());
  RETURN NEW;
END $$;

DROP TRIGGER IF EXISTS enforce_checklist_before_signing ON core.contracts;
CREATE TRIGGER enforce_checklist_before_signing BEFORE INSERT OR UPDATE ON core.contracts
  FOR EACH ROW EXECUTE FUNCTION core.enforce_checklist_before_signing();

-- --- FR-905: регистрация только подписанного обеими сторонами --------------
CREATE OR REPLACE FUNCTION core.check_contract_registration() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.registered_at IS NOT NULL
     AND (TG_OP = 'INSERT' OR OLD.registered_at IS NULL) THEN
    IF NEW.tenant_signed_at IS NULL OR NEW.landlord_signed_at IS NULL THEN
      RAISE EXCEPTION 'FR-905: регистрируется договор, подписанный обеими сторонами (п. 126)';
    END IF;
    IF NEW.reg_number IS NULL THEN
      RAISE EXCEPTION 'FR-905: регистрация без номера в журнале невозможна (п. 126)';
    END IF;
  END IF;
  RETURN NEW;
END $$;

DROP TRIGGER IF EXISTS check_contract_registration ON core.contracts;
CREATE TRIGGER check_contract_registration BEFORE INSERT OR UPDATE ON core.contracts
  FOR EACH ROW EXECUTE FUNCTION core.check_contract_registration();
