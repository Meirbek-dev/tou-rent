-- Справочники (арх. § 6): версионируемые коэффициенты, МРП, производственный
-- календарь РК, таблица переходов статусов, основания отклонения.

-- МРП по годам (FR-201: Рбс = 1,5 МРП за м²/год). Значения вносит admin.
CREATE TABLE refdata.mrp (
  year   int           PRIMARY KEY CHECK (year BETWEEN 2000 AND 2100),
  amount numeric(14,2) NOT NULL CHECK (amount > 0)
);
COMMENT ON TABLE refdata.mrp IS 'Месячный расчетный показатель по годам (FR-201, Прил. 4)';

-- Коэффициенты Прил. 4, версионируются по effective_from (FR-202):
-- расчет использует версию на дату, снимок замораживается в лоте тендера.
CREATE TABLE refdata.rate_coefficients (
  id             uuid          PRIMARY KEY DEFAULT uuidv7(),
  coefficient    text          NOT NULL,  -- код множителя: kt, kk, ksk, kr, kvd, kopf, kfu, ksots, k, kn, kv
  option_code    text          NOT NULL,  -- код опции внутри множителя
  label_ru       text          NOT NULL,
  label_kk       text,
  label_en       text,
  value          numeric(8,4)  NOT NULL CHECK (value > 0),
  effective_from date          NOT NULL,
  UNIQUE (coefficient, option_code, effective_from)
);
COMMENT ON TABLE refdata.rate_coefficients IS
  'Множители ставки аренды Прил. 4 (FR-201–202); изменение не влияет на прошлые расчеты';

-- Производственный календарь РК: только праздники, выходные (сб/вс) считает
-- add_business_days. Редактирует admin (FR-1701).
CREATE TABLE refdata.holidays (
  day      date PRIMARY KEY,
  label_ru text NOT NULL
);

-- Разрешенные переходы статусов тендера — единственный источник для триггера INV-021.
-- Изменение перечня — только миграцией (роль приложения имеет лишь SELECT).
CREATE TABLE refdata.tender_status_transitions (
  from_status core.tender_status NOT NULL,
  to_status   core.tender_status NOT NULL,
  PRIMARY KEY (from_status, to_status)
);
COMMENT ON TABLE refdata.tender_status_transitions IS
  'INV-021: переходы статусов тендера (FR-302); паритет с typestate домена проверяет тест';

-- Закрытый перечень оснований отклонения заявки (INV-052, п. 52) — без catch-all.
CREATE TABLE refdata.rejection_reasons (
  code     text PRIMARY KEY,
  label_ru text NOT NULL,
  label_kk text,
  label_en text,
  rule_ref text NOT NULL  -- пункт Правил
);

-- Рабочие дни (G12): SQL-половина; Rust-паритет — domain::calendar (FR-1701).
-- Выходные — сб/вс (isodow 6,7) и дни из refdata.holidays.
CREATE FUNCTION refdata.add_business_days(start_date date, days int) RETURNS date
LANGUAGE plpgsql STABLE AS $$
DECLARE
  d date := start_date;
  remaining int := days;
BEGIN
  IF days < 0 THEN
    RAISE EXCEPTION 'add_business_days: количество дней должно быть >= 0, получено %', days;
  END IF;
  WHILE remaining > 0 LOOP
    d := d + 1;
    IF extract(isodow FROM d) < 6
       AND NOT EXISTS (SELECT 1 FROM refdata.holidays h WHERE h.day = d) THEN
      remaining := remaining - 1;
    END IF;
  END LOOP;
  RETURN d;
END $$;
