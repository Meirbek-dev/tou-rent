-- Шифрование ценовых предложений at-rest (М4, INV-040, п. 40, арх. § 6).
--
-- RLS уже запрещает чтение цен до вскрытия, но дамп базы ее обходит: строка
-- лежит открытым числом. Второй рубеж — pgcrypto: цена хранится
-- зашифрованной ключом тендера, а сам ключ выводится из мастер-ключа
-- приложения (GUC `app.price_key`, переменная окружения PRICE_ENCRYPTION_KEY)
-- и в базе не хранится. Без ключа цена не читается и не записывается —
-- даже суперпользователем.

CREATE EXTENSION IF NOT EXISTS pgcrypto;  -- INV-040: шифрование цен (п. 40)

ALTER TABLE core.price_proposals
  ADD COLUMN amount_enc bytea,
  ALTER COLUMN amount DROP NOT NULL,
  ADD CONSTRAINT price_is_stored
    CHECK (amount IS NOT NULL OR amount_enc IS NOT NULL);

COMMENT ON COLUMN core.price_proposals.amount IS
  'Открытая цена: остается только у записей до перехода на шифрование (п. 40)';
COMMENT ON COLUMN core.price_proposals.amount_enc IS
  'INV-040: цена, зашифрованная ключом тендера (pgp_sym_encrypt, п. 40)';

-- Мастер-ключ приложения: как и `app.user_id`, приходит соединением и в базе
-- не хранится. Пустая строка означает «ключа нет».
CREATE FUNCTION core.price_key() RETURNS text
LANGUAGE sql STABLE AS $$
  SELECT nullif(current_setting('app.price_key', true), '')
$$;

-- Ключ тендера (п. 40): производный от мастер-ключа, свой у каждого тендера —
-- компрометация одного тендера не раскрывает остальные.
CREATE FUNCTION core.tender_price_key(p_tender_id uuid) RETURNS text
LANGUAGE sql STABLE AS $$
  SELECT CASE
    WHEN core.price_key() IS NULL OR p_tender_id IS NULL THEN NULL
    ELSE encode(hmac(p_tender_id::text, core.price_key(), 'sha256'), 'hex')
  END
$$;

-- Цена записывается только зашифрованной: открытое значение стирается тем же
-- триггером, а без ключа запись невозможна (INV-040).
CREATE FUNCTION core.encrypt_price_proposal() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  key text;
BEGIN
  IF NEW.amount IS NULL THEN
    RETURN NEW;  -- значение уже зашифровано
  END IF;

  key := core.tender_price_key(
    (SELECT a.tender_id FROM core.applications a WHERE a.id = NEW.application_id));
  IF key IS NULL THEN
    RAISE EXCEPTION
      'INV-040: ценовое предложение не записывается без ключа шифрования (п. 40)';
  END IF;

  NEW.amount_enc := pgp_sym_encrypt(NEW.amount::text, key);
  NEW.amount := NULL;
  RETURN NEW;
END $$;

CREATE TRIGGER encrypt_price_proposal BEFORE INSERT OR UPDATE ON core.price_proposals
  FOR EACH ROW EXECUTE FUNCTION core.encrypt_price_proposal();

-- Чтение цены: расшифровка ключом тендера. Без ключа и с чужим ключом
-- возвращается NULL — цена не раскрывается и ошибкой не выдает себя
-- (RLS INV-040 остается первым рубежом).
CREATE FUNCTION core.price_amount(p core.price_proposals) RETURNS numeric
LANGUAGE plpgsql STABLE AS $$
DECLARE
  key text;
BEGIN
  IF p.amount IS NOT NULL THEN
    RETURN p.amount;  -- записи до перехода на шифрование
  END IF;
  IF p.amount_enc IS NULL THEN
    RETURN NULL;
  END IF;

  key := core.tender_price_key(
    (SELECT a.tender_id FROM core.applications a WHERE a.id = p.application_id));
  IF key IS NULL THEN
    RETURN NULL;
  END IF;

  BEGIN
    RETURN pgp_sym_decrypt(p.amount_enc, key)::numeric;
  EXCEPTION WHEN others THEN
    RETURN NULL;  -- ключ не подходит: цена остается закрытой
  END;
END $$;

COMMENT ON FUNCTION core.price_amount(core.price_proposals) IS
  'INV-040: расшифровка цены ключом тендера; без ключа — NULL (п. 40)';
