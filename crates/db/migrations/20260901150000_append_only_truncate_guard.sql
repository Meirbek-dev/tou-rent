-- Запрет TRUNCATE на append-only таблицах (INV-A01 и соседние инварианты).
--
-- Рубеж «append-only» строился двумя слоями: REVOKE UPDATE/DELETE у роли
-- приложения и триггеры core.forbid_mutation, про которые в
-- 20260806100014_app_role_grants.sql прямо сказано, что они работают
-- «и для владельца БД, которого privileges не ограничивают».
--
-- Но все 17 триггеров объявлены `FOR EACH ROW`, а TRUNCATE не вызывает
-- строковых триггеров вовсе - он не удаляет строки по одной. То есть
-- владелец (а прод-подключение ходит именно суперпользователем, см. SEC-1
-- в specs/GAUNTLET.md) мог стереть весь журнал аудита одной командой,
-- не задев ни одного триггера:
--
--   TRUNCATE audit.log;          -- проходило
--   SELECT audit.verify_chain(); -- t: пустая цепочка «цела»
--
-- Последнее и есть худшая часть: механизм, построенный ради обнаружения
-- подделки, после стирания рапортует о целостности. Сверка не отличает
-- «ничего не подделано» от «нечего сверять».
--
-- Лечится тем же самым core.forbid_mutation: функция ничего не берет из
-- NEW/OLD, только RAISE, поэтому годится и как statement-триггер.
-- Уровень FOR EACH STATEMENT для TRUNCATE - единственно возможный.
--
-- Полной защиты от владельца это не дает (ALTER TABLE ... DISABLE TRIGGER
-- остается) и дать не может: внутри БД от ее суперпользователя не
-- закрыться. Задача рубежа - чтобы стирание журнала требовало явного,
-- отдельного и заведомо неслучайного действия, а не одной команды.
-- Внешний якорь последнего row_hash - отдельный вопрос (Q-018,
-- наблюдаемости у бэкенда нет), здесь он не решается.

DO $$
DECLARE
  guarded record;
  code    text;
BEGIN
  -- Перечень берется не списком в тексте миграции, а из самого каталога:
  -- у какой таблицы есть строковый запрет, у той обязан быть и TRUNCATE.
  -- Список руками разошелся бы с реальностью на первой же новой таблице.
  FOR guarded IN
    SELECT n.nspname AS schema_name,
           c.relname AS table_name,
           encode(t.tgargs, 'escape') AS raw_args
    FROM pg_trigger t
    JOIN pg_class c     ON c.oid = t.tgrelid
    JOIN pg_namespace n ON n.oid = c.relnamespace
    JOIN pg_proc p      ON p.oid = t.tgfoid
    WHERE p.proname = 'forbid_mutation'
      AND NOT t.tgisinternal
      -- бит 32 (TRIGGER_TYPE_TRUNCATE) - чтобы не отражать самих себя
      -- при повторном накате; строковые UPDATE/DELETE его не несут
      AND (t.tgtype & 32) = 0
    ORDER BY 1, 2
  LOOP
    -- Код инварианта хранится в tgargs с завершающим NUL - в сообщении
    -- об отказе он должен остаться тем же, что и у строкового триггера
    code := rtrim(guarded.raw_args, E'\\000');

    IF NOT EXISTS (
      SELECT 1 FROM pg_trigger t2
      JOIN pg_class c2     ON c2.oid = t2.tgrelid
      JOIN pg_namespace n2 ON n2.oid = c2.relnamespace
      WHERE n2.nspname = guarded.schema_name
        AND c2.relname = guarded.table_name
        AND t2.tgname  = guarded.table_name || '_no_truncate'
    ) THEN
      EXECUTE format(
        'CREATE TRIGGER %I BEFORE TRUNCATE ON %I.%I
           FOR EACH STATEMENT EXECUTE FUNCTION core.forbid_mutation(%L)',
        guarded.table_name || '_no_truncate',
        guarded.schema_name,
        guarded.table_name,
        coalesce(nullif(code, ''), 'append-only')
      );
    END IF;
  END LOOP;
END $$;

-- Право TRUNCATE роли приложения не выдавалось и не выдается: триггер -
-- рубеж против владельца, а не замена привилегиям.
REVOKE TRUNCATE ON ALL TABLES IN SCHEMA core, audit FROM PUBLIC;
