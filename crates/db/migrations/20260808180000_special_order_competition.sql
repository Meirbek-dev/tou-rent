-- Конкуренция заявок особого порядка (М12, FR-1203, п. 86, 97).
--
-- Правила Правил: по категориям 4–5 две и более заявки переводят вопрос
-- в общий порядок (п. 86), а по инвестиционной категории приоритет у большей
-- суммы; при сопоставимых суммах Правление вправе направить в общий порядок
-- с обоснованием в решении (п. 97).
--
-- INV-086: заявка не удовлетворяется, пока есть конкурент с приоритетом —
-- либо перечень категории требует общего порядка, либо у конкурента
-- существенно большая сумма инвестиций. Закреплено типом домена и триггером.
--
-- Правило конкуренции и порог сопоставимости сумм объявляет категория
-- (тот же прием, что у срока проверки, FR-1201): значения — данные Правил,
-- а не код. TODO-ENGINEER: какая из тринадцати категорий инвестиционная и
-- каков порог «сопоставимых» сумм — по Правилам (Q-009, Q-013).

CREATE TYPE core.special_competition AS ENUM ('none', 'redirect', 'highest_amount');

ALTER TABLE refdata.special_categories
  ADD COLUMN competition core.special_competition NOT NULL DEFAULT 'none',
  ADD COLUMN comparable_margin_pct numeric(5,2) NOT NULL DEFAULT 5.00
    CHECK (comparable_margin_pct >= 0 AND comparable_margin_pct <= 100);

COMMENT ON COLUMN refdata.special_categories.competition IS
  'FR-1203 (п. 86): что делать при двух и более заявках по категории';
COMMENT ON COLUMN refdata.special_categories.comparable_margin_pct IS
  'FR-1203 (п. 97): суммы инвестиций считаются сопоставимыми, если различаются не более чем на столько процентов (TODO-ENGINEER: Q-013)';

-- Категории 4–5 названы номерами в самом ТЗ (FR-1203), поэтому правило
-- общего порядка ставится по номеру подпункта п. 87
UPDATE refdata.special_categories
   SET competition = 'redirect'
 WHERE ordinal IN (4, 5);

-- Сумма инвестиций (п. 91–94, FR-1204): ею ранжируются заявки
-- инвестиционной категории (п. 97).
ALTER TABLE core.special_requests
  ADD COLUMN investment_amount numeric(14,2) CHECK (investment_amount > 0),
  -- Тендер, созданный переводом заявки в общий порядок (п. 86)
  ADD COLUMN tender_id uuid REFERENCES core.tenders (id);

COMMENT ON COLUMN core.special_requests.investment_amount IS
  'FR-1203/FR-1204: объем инвестиций — приоритет большей суммы (п. 97)';
COMMENT ON COLUMN core.special_requests.tender_id IS
  'FR-1203: тендер общего порядка, созданный по решению Правления (п. 86)';

-- Заявка инвестиционной категории без суммы неранжируема (п. 97)
CREATE FUNCTION core.check_special_investment_amount() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  rule core.special_competition;
BEGIN
  SELECT c.competition INTO rule
  FROM refdata.special_categories c WHERE c.code = NEW.category;

  IF rule = 'highest_amount' AND NEW.investment_amount IS NULL THEN
    RAISE EXCEPTION
      'FR-1203: заявка инвестиционной категории подается с объемом инвестиций (п. 97)';
  END IF;

  RETURN NEW;
END $$;

CREATE TRIGGER check_special_investment_amount
  BEFORE INSERT OR UPDATE ON core.special_requests
  FOR EACH ROW EXECUTE FUNCTION core.check_special_investment_amount();

-- Конкурирующие заявки (п. 86): активные заявки той же категории на тот же
-- объект. Заявка без объекта ни с кем не конкурирует — предмет спора не задан.
CREATE FUNCTION core.special_competitors(p_request_id uuid)
RETURNS TABLE (id uuid, investment_amount numeric)
LANGUAGE sql STABLE AS $$
  SELECT other.id, other.investment_amount
  FROM core.special_requests self
  JOIN core.special_requests other
    ON other.id <> self.id
   AND other.category = self.category
   AND other.object_id = self.object_id
   AND other.status IN ('submitted', 'under_review')
  WHERE self.id = p_request_id AND self.object_id IS NOT NULL;
$$;

-- INV-086: пока есть конкурент с приоритетом, заявка не удовлетворяется.
-- Правление по-прежнему вправе отказать или направить в общий порядок —
-- запрещено именно «предоставить» в обход конкуренции.
CREATE FUNCTION core.check_special_competition() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  rule       core.special_competition;
  margin     numeric;
  own_amount numeric;
  rivals     int;
  best_rival numeric;
BEGIN
  IF NEW.decision <> 'grant' THEN
    RETURN NEW;  -- отказ и общий порядок конкуренцией не ограничены
  END IF;

  SELECT c.competition, c.comparable_margin_pct, r.investment_amount
    INTO rule, margin, own_amount
  FROM core.special_requests r
  JOIN refdata.special_categories c ON c.code = r.category
  WHERE r.id = NEW.special_request_id;

  SELECT count(*), max(k.investment_amount)
    INTO rivals, best_rival
  FROM core.special_competitors(NEW.special_request_id) k;

  IF rivals = 0 THEN
    RETURN NEW;
  END IF;

  IF rule = 'redirect' THEN
    RAISE EXCEPTION
      'INV-086: по категории подано % конкурирующих заявок — вопрос выносится в общий порядок (п. 86)',
      rivals + 1
      USING ERRCODE = 'raise_exception';
  END IF;

  -- Инвестиционная категория: приоритет большей суммы. Сопоставимые суммы
  -- приоритета не дают — там решает Правление (п. 97).
  IF rule = 'highest_amount'
     AND best_rival IS NOT NULL
     AND coalesce(own_amount, 0) < best_rival * (1 - margin / 100) THEN
    RAISE EXCEPTION
      'INV-086: конкурирующая заявка предлагает больший объем инвестиций (п. 97)'
      USING ERRCODE = 'raise_exception';
  END IF;

  RETURN NEW;
END $$;

CREATE TRIGGER check_special_competition BEFORE INSERT ON core.special_board_decisions
  FOR EACH ROW EXECUTE FUNCTION core.check_special_competition();
