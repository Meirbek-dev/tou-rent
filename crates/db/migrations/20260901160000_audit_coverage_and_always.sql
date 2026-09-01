-- Полнота аудита (FR-1601) и режим ALWAYS для сторожей журнала (INV-A01).
--
-- Круг 2 гаунтлета показал две дыры в одном рубеже.
--
-- 1. Три таблицы мутируются приложением, но не пишут ни одного события.
--    Живой прогон create → update → delete объекта имущества дал ноль
--    строк в audit.log: у `core.objects` есть `fill_object_kk` и
--    `touch_updated_at`, а `audit_record` — нет. Так же молчат
--    `core.auctions` (там лежит юридический итог торгов:
--    winner_application_id, winner_amount) и `core.ledger_accounts`.
--    Регламент А.5 требует событие на каждую мутацию домена.
--
--    Гейт G15 этого не ловил по построению: он проверяет «у каждой
--    таблицы перечня есть триггер», но не «каждая мутируемая таблица
--    попала в перечень». Новая таблица мимо перечня всегда зеленая.
--    Обратное направление закрывает тест
--    g15_every_mutable_core_table_is_in_inventory.
--
-- 2. Все 46 триггеров `audit.record`, 3 `audit.record_natural_key` и
--    4 заморозки `core.freeze_*` создавались с tgenabled='O' (origin).
--    Такой триггер молчит при `session_replication_role = 'replica'` —
--    двух операторов без всякого DDL хватало, чтобы переписать
--    подписанные условия договора, не оставив следа:
--
--      SET session_replication_role = 'replica';
--      UPDATE core.contracts SET monthly_rate = 1 WHERE ...;  -- audit.log не вырос
--
--    Цепочка при этом остается «целой»: подделывать нечего, событие
--    просто не создано. Круг 1 перевел в ALWAYS только семейство
--    `core.forbid_mutation` (20260901150000), соседние сторожа остались
--    в origin. Этот режим ставит `pg_restore --disable-triggers`, то
--    есть обойти рубеж можно было и не желая того.
--
-- Полной защиты от суперпользователя это не дает и дать не может
-- (ALTER TABLE ... DISABLE TRIGGER остается) — задача та же, что и у
-- круга 1: чтобы обход требовал явного, отдельного и заведомо
-- неслучайного действия.

-- --- 1. Недостающие триггеры перечня INV-AUDIT ------------------------------
DO $$
DECLARE
  target text;
BEGIN
  FOREACH target IN ARRAY ARRAY['objects', 'auctions', 'ledger_accounts']
  LOOP
    IF NOT EXISTS (
      SELECT 1 FROM pg_trigger t
      JOIN pg_class c     ON c.oid = t.tgrelid
      JOIN pg_namespace n ON n.oid = c.relnamespace
      WHERE n.nspname = 'core' AND c.relname = target
        AND NOT t.tgisinternal AND t.tgname = 'audit_record'
    ) THEN
      EXECUTE format(
        'CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.%I
           FOR EACH ROW EXECUTE FUNCTION audit.record()', target
      );
    END IF;
  END LOOP;
END $$;

-- --- 2. Сторожа журнала и заморозки — в режим ALWAYS ------------------------
DO $$
DECLARE
  guard record;
BEGIN
  -- Перечень берется из каталога, а не списком в тексте миграции: список
  -- руками разошелся бы с реальностью на первом же новом триггере.
  FOR guard IN
    SELECT n.nspname AS schema_name, c.relname AS table_name, t.tgname AS trigger_name
    FROM pg_trigger t
    JOIN pg_class c      ON c.oid = t.tgrelid
    JOIN pg_namespace n  ON n.oid = c.relnamespace
    JOIN pg_proc p       ON p.oid = t.tgfoid
    JOIN pg_namespace pn ON pn.oid = p.pronamespace
    WHERE NOT t.tgisinternal
      AND t.tgenabled <> 'A'
      AND (
        (pn.nspname = 'audit' AND p.proname LIKE 'record%')
        OR (pn.nspname = 'core' AND p.proname LIKE 'freeze\_%')
      )
    ORDER BY 1, 2, 3
  LOOP
    EXECUTE format(
      'ALTER TABLE %I.%I ENABLE ALWAYS TRIGGER %I',
      guard.schema_name, guard.table_name, guard.trigger_name
    );
  END LOOP;
END $$;
