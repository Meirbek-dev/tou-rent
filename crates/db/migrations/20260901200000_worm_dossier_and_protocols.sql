-- WORM досье и протоколов: неизменяемость сверх срока хранения (INV-042, FR-702).
--
-- Круг 2 гаунтлета, пробы в одноразовой БД:
--
-- 1. Материал досье переписывался на месте целиком:
--      UPDATE core.dossier_items SET kind='rewritten', title='rewritten',
--             source_table='nowhere', occurred_at = core.now() - '10 years';
--      -> UPDATE 2
--    Отбивались только сокращение `retain_until`, подмена предмета досье
--    и отвязка файла: `core.check_dossier_retention` перечисляет
--    запрещенные изменения явным списком, и вид материала, его источник и
--    момент события в этот список не попали. Между тем `occurred_at`
--    задает `retain_until` при вставке - то есть сдвинув момент назад,
--    срок хранения можно было бы пересчитать в свою пользу.
--
-- 2. Протокол о результатах роль приложения переписывала и удаляла:
--      SET LOCAL ROLE tou_rent_app;
--      UPDATE core.protocols SET content='{"winner":"B"}' ... -> UPDATE 1
--      DELETE FROM core.protocols                        ... -> DELETE 1
--    `core.protocols` есть в перечне INV-AUDIT (удаление залогируется),
--    но среди 17 таблиц под `core.forbid_mutation` протокола не было -
--    след оставался, а сам документ исчезал. Удаление протокола не
--    предусмотрено ни одним сценарием: снятие публикации - это
--    `unpublished_at`, а не DELETE.
--
-- UPDATE протокола запретить нельзя: публикация и снятие идут именно им
-- (`check_protocol_publication` сторожит, что переписывается только
-- разрешенное). Поэтому здесь закрывается DELETE и TRUNCATE.

-- --- 1. Материал досье неизменен ------------------------------------------
CREATE OR REPLACE FUNCTION core.check_dossier_retention() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'INSERT' THEN
    NEW.retain_until := NEW.occurred_at + make_interval(years =>
      CASE WHEN NEW.tender_id IS NOT NULL THEN 5 ELSE 3 END);
    RETURN NEW;
  END IF;

  -- Предмет досье задает срок: подменив его, срок можно было бы сократить
  IF NEW.tender_id IS DISTINCT FROM OLD.tender_id
     OR NEW.special_request_id IS DISTINCT FROM OLD.special_request_id THEN
    RAISE EXCEPTION 'INV-042: предмет досье не переписывается (п. 16.15, 42)';
  END IF;

  -- Вид материала, его источник и момент события - то же тело документа.
  -- `occurred_at` вдобавок задает `retain_until` при вставке, поэтому его
  -- сдвиг назад - способ сократить срок хранения в обход проверки ниже.
  IF NEW.kind IS DISTINCT FROM OLD.kind
     OR NEW.source_table IS DISTINCT FROM OLD.source_table
     OR NEW.source_id IS DISTINCT FROM OLD.source_id
     OR NEW.occurred_at IS DISTINCT FROM OLD.occurred_at THEN
    RAISE EXCEPTION
      'INV-042: вид, источник и момент материала досье не переписываются (п. 16.15, 42)';
  END IF;

  IF NEW.retain_until < OLD.retain_until THEN
    RAISE EXCEPTION
      'INV-042: срок хранения материала досье не сокращается (хранится до %), п. 16.15, 42',
      OLD.retain_until;
  END IF;

  -- Обнулить ссылку на файл — то же изъятие материала, только другим
  -- способом; замена одного документа другим (подписанный скан вместо
  -- проекта) — обычный ход дела и запретом не считается
  IF OLD.file_key IS NOT NULL AND NEW.file_key IS NULL THEN
    RAISE EXCEPTION 'INV-042: файл материала досье не отвязывается (FR-1602)';
  END IF;

  RETURN NEW;
END $$;

-- --- 2. Протокол не удаляется ---------------------------------------------
REVOKE DELETE, TRUNCATE ON core.protocols FROM tou_rent_app;

CREATE TRIGGER protocols_no_delete BEFORE DELETE ON core.protocols
  FOR EACH ROW EXECUTE FUNCTION core.forbid_mutation('FR-702');
ALTER TABLE core.protocols ENABLE ALWAYS TRIGGER protocols_no_delete;

-- Парный страж на TRUNCATE - того же вида, что заводит
-- 20260901150000_append_only_truncate_guard.sql: строковый триггер при
-- TRUNCATE не срабатывает вовсе, таблица стирается одним оператором.
CREATE TRIGGER protocols_no_truncate BEFORE TRUNCATE ON core.protocols
  FOR EACH STATEMENT EXECUTE FUNCTION core.forbid_mutation('FR-702');
ALTER TABLE core.protocols ENABLE ALWAYS TRIGGER protocols_no_truncate;
