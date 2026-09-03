-- Редакция документации не ломает порядок сроков (FR-303, FR-304, п. 27, 50, 59).
--
-- Круг 3 гаунтлета (G-099, регрессия G-043). Публикация новой редакции на
-- тендере с назначенной датой торгов отбивалась ошибкой 409
-- `tender_schedule_order`:
--
--   1. INSERT в `core.tender_amendments` -> AFTER-триггер
--      `core.apply_amendment_effects` переносит `submission_deadline` на
--      новый срок и сдвигает вслед за ним `opening_at`, но `trading_at`
--      оставляет на месте;
--   2. промежуточная строка получает `opening_at > trading_at` и падает на
--      CHECK `opening_before_trading` (20260903000100);
--   3. собственный UPDATE обработчика с новой датой торгов
--      (`crates/db/src/amendments.rs`) до этого места уже не доходит.
--
-- То есть сроки процедуры расходились внутри одной вставки: следствие
-- редакции переносило две отметки из трех. Лечится там же, где возникло, -
-- в самом триггере: строка обновляется один раз и сразу целиком, а дата
-- торгов идет за вскрытием той же разницей, какой сдвинулось вскрытие.
-- Разрыв «вскрытие -> торги» при этом сохраняется, как сохраняется разрыв
-- «прием -> вскрытие».
--
-- Своей даты торгов у редакции нет: `core.tender_amendments` хранит только
-- новый срок приема (`new_deadline`), а желаемые вскрытие и торги приходят
-- отдельным UPDATE обработчика уже после вставки. Поэтому здесь - перенос
-- на ту же разницу, а не подстановка запрошенного значения.
--
-- Чего здесь намеренно НЕ меняется: окно п. 27 (правка не позже чем за два
-- календарных дня до дедлайна, продление не менее чем на десять) - его
-- держит BEFORE-триггер `core.check_tender_amendment`, и он не тронут;
-- обязанность известить участников (FR-1702) - ее ставит слой данных.

CREATE OR REPLACE FUNCTION core.apply_amendment_effects() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  before        core.tenders%ROWTYPE;
  moved_opening timestamptz;
  moved_trading timestamptz;
BEGIN
  SELECT * INTO before FROM core.tenders WHERE id = NEW.tender_id;

  -- Вскрытие идет за приемом: если оно оказывалось раньше нового срока,
  -- его переносят на ту же разницу, на какую продлен прием (п. 27)
  moved_opening := CASE
    WHEN before.opening_at IS NULL OR NEW.previous_deadline IS NULL
      THEN before.opening_at
    WHEN before.opening_at >= NEW.new_deadline
      THEN before.opening_at
    ELSE NEW.new_deadline + (before.opening_at - NEW.previous_deadline)
  END;

  -- Торги идут за вскрытием (п. 59): дата сдвигается той же разницей,
  -- на какую сдвинулось вскрытие, - разрыв между ними сохраняется, и
  -- строка ни на миг не получает `opening_at > trading_at`
  moved_trading := CASE
    WHEN before.trading_at IS NULL OR before.opening_at IS NULL
         OR moved_opening IS NULL
      THEN before.trading_at
    WHEN moved_opening <= before.opening_at
      THEN before.trading_at
    ELSE before.trading_at + (moved_opening - before.opening_at)
  END;

  UPDATE core.tenders
  SET submission_deadline = NEW.new_deadline,
      opening_at          = moved_opening,
      trading_at          = moved_trading
  WHERE id = NEW.tender_id;

  RETURN NULL;  -- AFTER-триггер
END $$;

COMMENT ON FUNCTION core.apply_amendment_effects() IS
  'FR-304: редакция продлевает прием и переносит вскрытие и торги той же разницей (п. 27, 50, 59)';
