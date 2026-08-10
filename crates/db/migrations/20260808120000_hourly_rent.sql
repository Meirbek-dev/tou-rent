-- Почасовая аренда (М2, FR-205, п. 97, Прил. 4 п. 6).
--
-- Почасовой лот отличается не «галочкой в форме», а единицей ставки:
-- предмет торгов — плата за час, а не за месяц, и разыгрывается объем
-- часов. Гарантийный взнос почасового лота считается от этого объема
-- (FR-206 по смыслу: взнос равен базовой стоимости лота, A-062).

CREATE TYPE core.rate_unit AS ENUM ('monthly', 'hourly');

ALTER TABLE core.lots
  ADD COLUMN rate_unit   core.rate_unit NOT NULL DEFAULT 'monthly',
  ADD COLUMN hours_total integer CHECK (hours_total > 0),
  -- Объем часов есть ровно у почасового лота
  ADD CONSTRAINT hourly_lot_has_hours
    CHECK ((rate_unit = 'hourly') = (hours_total IS NOT NULL));

COMMENT ON COLUMN core.lots.rate_unit IS
  'FR-205: единица базовой ставки — месяц (п. 137) либо час (п. 97)';
COMMENT ON COLUMN core.lots.hours_total IS
  'FR-205: объем разыгрываемых часов почасового лота (п. 97)';
COMMENT ON COLUMN core.lots.base_rate_monthly IS
  'Базовая ставка лота в его единице: за месяц (п. 137) либо за час (п. 97, FR-205)';

-- FR-206 остается прежним для помесячных лотов (взнос = месячная ставка),
-- а у почасовых равен стоимости разыгрываемого объема часов (A-062).
ALTER TABLE core.lots
  DROP CONSTRAINT lots_guarantee_fee_equals_monthly_rate,
  ADD CONSTRAINT lots_guarantee_fee_equals_monthly_rate
    CHECK (rate_unit = 'hourly' OR guarantee_fee = base_rate_monthly);

-- Взнос почасового лота считает БД — «забыть умножить» на стороне
-- приложения невозможно. Лот без объема часов отклонит CHECK выше.
CREATE FUNCTION core.set_hourly_lot_fee() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.rate_unit = 'hourly' AND NEW.hours_total IS NOT NULL THEN
    NEW.guarantee_fee := round(NEW.base_rate_monthly * NEW.hours_total, 2);
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER set_hourly_lot_fee BEFORE INSERT OR UPDATE ON core.lots
  FOR EACH ROW EXECUTE FUNCTION core.set_hourly_lot_fee();
