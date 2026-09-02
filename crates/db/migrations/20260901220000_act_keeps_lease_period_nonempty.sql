-- Возврат объекта не обнуляет период аренды (INV-DB-02).
--
-- `core.apply_act_effects` при акте возврата закрывает период датой акта:
--   lease_period = tstzrange(lower(lease_period), NEW.act_date::timestamptz, '[)')
--
-- Если акт возврата подписан в тот же день, что и передача (а дата акта -
-- это полночь, тогда как передача отмечена временем), верхняя граница
-- оказывается не позже нижней, и `tstzrange` возвращает **пустой**
-- диапазон. Пустой диапазон не пересекается ни с чем, то есть договор
-- перестает занимать объект в EXCLUDE-ограничении `no_overlapping_lease`:
-- по одному объекту становится возможна вторая «непересекающаяся» аренда.
--
-- Прежде это проходило молча. После того как круг 2 гаунтлета закрыл
-- пустой диапазон проверкой `contracts_lease_period_not_empty`
-- (20260901210000), тот же путь стал отказом на ровном месте: акт
-- возврата в день передачи переставал регистрироваться вовсе.
--
-- Правильно здесь не ослабить проверку, а не порождать пустоту: аренда,
-- закончившаяся в день начала, - это одна точка на оси времени, и
-- записывается она замкнутым с обеих сторон диапазоном.

CREATE OR REPLACE FUNCTION core.apply_act_effects() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.kind = 'handover' THEN
    UPDATE core.contracts
    SET status = 'active',
        rent_starts_on = NEW.act_date,
        lease_period = tstzrange(
          NEW.act_date::timestamptz,
          NEW.act_date::timestamptz + make_interval(months => coalesce(lease_months, 12)),
          '[)')
    WHERE id = NEW.contract_id;
  ELSE
    UPDATE core.contracts
    SET status = 'completed',
        lease_period = CASE
          WHEN lower(lease_period) IS NOT NULL
               AND NEW.act_date::timestamptz <= lower(lease_period)
          THEN tstzrange(lower(lease_period), lower(lease_period), '[]')
          ELSE tstzrange(lower(lease_period), NEW.act_date::timestamptz, '[)')
        END
    WHERE id = NEW.contract_id;
  END IF;

  RETURN NULL;  -- AFTER-триггер
END $$;
