-- Год МРП удаляется только тогда, когда на нем ничего не стоит
-- (FR-201, FR-202, Прил. 4).
--
-- Круг 1 гаунтлета (G-141). Величина МРП заводится по годам и правится
-- значением, а не версией, поэтому опечатка в году («2027» вместо «2026»)
-- остается в справочнике навсегда: удаления не было ни в контракте, ни в
-- правах роли приложения (GRANT на refdata.mrp - только a/r/w).
--
-- Удаление года само по себе безобидным не бывает: расчет ставки хранится
-- в лоте и в инвестиционном договоре замороженным снимком (FR-202), и в
-- этом снимке лежит величина МРП того года, по которому ставка посчитана.
-- Года в снимке нет - есть величина, поэтому «используется» здесь означает
-- ровно то, что проверяемо: снимок, в котором стоит величина этого года.
-- Текущий год закрыт и без снимков - по нему считаются новые ставки
-- (`refdata.current_mrp`, A-010), и его удаление обнулило бы калькулятор.
--
-- Сторож стоит триггером таблицы, а не проверкой обработчика: право
-- удалять получает роль приложения целиком, и обойти проверку смогла бы
-- любая другая точка записи. Идентификатор правила стоит первым словом
-- сообщения - по нему `RuleViolation::from_message` собирает причину
-- `mrp_year_in_use` (crates/domain/src/rule.rs).

-- Величина МРП из замороженного снимка расчета (FR-202). Годовой расчет
-- держит ее в `inputs.mrp`, почасовой (FR-205) - прямо в `mrp`; снимок
-- неизвестной формы величины не дает и в сравнение не попадает.
CREATE OR REPLACE FUNCTION core.rate_snapshot_mrp(snapshot jsonb)
RETURNS numeric
LANGUAGE sql IMMUTABLE AS $$
  SELECT CASE
           WHEN raw ~ '^[0-9]+(\.[0-9]+)?$' THEN raw::numeric
         END
  FROM (SELECT COALESCE(snapshot #>> '{inputs,mrp}', snapshot #>> '{mrp}') AS raw) AS s
$$;

COMMENT ON FUNCTION core.rate_snapshot_mrp(jsonb) IS
  'FR-202: величина МРП из снимка расчета ставки (годового либо почасового)';

CREATE OR REPLACE FUNCTION core.check_mrp_year_in_use() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  lots_used     bigint;
  contracts_used bigint;
  current_year  int;
BEGIN
  current_year := extract(year FROM (core.now() AT TIME ZONE 'Asia/Almaty'))::int;
  IF OLD.year = current_year THEN
    RAISE EXCEPTION
      'FR-201: год МРП % - текущий, по нему считаются новые ставки (Прил. 4)',
      OLD.year;
  END IF;

  SELECT count(*) INTO lots_used
  FROM core.lots l
  WHERE core.rate_snapshot_mrp(l.rate_calculation) = OLD.amount;

  SELECT count(*) INTO contracts_used
  FROM core.investment_contracts ic
  WHERE core.rate_snapshot_mrp(ic.rate_calculation) = OLD.amount;

  IF lots_used > 0 OR contracts_used > 0 THEN
    RAISE EXCEPTION
      'FR-201: год МРП % участвует в расчетах: снимков лотов - %, договоров - % (Прил. 4, FR-202)',
      OLD.year, lots_used, contracts_used;
  END IF;

  RETURN OLD;
END $$;

COMMENT ON FUNCTION core.check_mrp_year_in_use() IS
  'FR-201: год МРП под снимком расчета либо текущий год не удаляется (G-141)';

DROP TRIGGER IF EXISTS check_mrp_year_in_use ON refdata.mrp;
CREATE TRIGGER check_mrp_year_in_use
  BEFORE DELETE ON refdata.mrp
  FOR EACH ROW EXECUTE FUNCTION core.check_mrp_year_in_use();

-- Право удалять заведенный по ошибке год. Коэффициентов это не касается:
-- их версии - история расчетов, и удаления у них нет по-прежнему (FR-202).
GRANT DELETE ON refdata.mrp TO tou_rent_app;
