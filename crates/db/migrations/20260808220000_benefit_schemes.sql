-- Льготные схемы особого порядка (М12, FR-1205, п. 95–96, Прил. 4).
--
-- Льгота — расписание платы по годам найма, а не скидка по договоренности:
-- первый год наниматель возмещает коммунальные расходы, со второго платит
-- долю ставки Прил. 4 (п. 95–96). Социальному арендатору льгота приходит
-- коэффициентом Ксоц внутри самого расчета (FR-201) — расписания у нее нет.
--
-- INV-095: льгота образовательного оборудования применяется по согласованию
-- Ученого совета (п. 95). INV-096: спин-офф обучает не менее пяти кредитов
-- в семестр (п. 96). Оба закреплены типом домена и триггером.
--
-- TODO-ENGINEER: доля второго года (50 % из ТЗ), величина квоты стажировок
-- и порядок согласования Ученого совета сверяются по Правилам (Q-010).

ALTER TABLE refdata.benefit_schemes
  ADD COLUMN has_schedule       boolean      NOT NULL DEFAULT false,
  ADD COLUMN later_share_pct    numeric(5,2) NOT NULL DEFAULT 50.00
    CHECK (later_share_pct > 0 AND later_share_pct <= 100),
  ADD COLUMN requires_council   boolean      NOT NULL DEFAULT false,
  ADD COLUMN min_study_credits  int          NOT NULL DEFAULT 0 CHECK (min_study_credits >= 0),
  ADD COLUMN internship_quota   int          NOT NULL DEFAULT 0 CHECK (internship_quota >= 0);

COMMENT ON COLUMN refdata.benefit_schemes.later_share_pct IS
  'FR-1205 (п. 95–96): доля ставки Прил. 4 со второго года найма';
COMMENT ON COLUMN refdata.benefit_schemes.internship_quota IS
  'FR-1205 (п. 95): квота стажировок; величина — из Правил (TODO-ENGINEER, Q-010)';

-- Условия схем — из ТЗ FR-1205: у образовательного оборудования согласование
-- Ученого совета, у спин-оффа обучение не менее пяти кредитов в семестр.
UPDATE refdata.benefit_schemes
   SET has_schedule = true, requires_council = true
 WHERE code = 'educational_equipment';

UPDATE refdata.benefit_schemes
   SET has_schedule = true, min_study_credits = 5
 WHERE code = 'spin_off';

-- Применение льготы к договору (п. 95–96): одна схема на договор.
CREATE TABLE core.benefit_grants (
  id               uuid          PRIMARY KEY DEFAULT uuidv7(),
  contract_id      uuid          NOT NULL UNIQUE REFERENCES core.contracts (id) ON DELETE CASCADE,
  scheme           text          NOT NULL REFERENCES refdata.benefit_schemes (code),
  -- Коммунальные расходы за месяц: плата первого года найма (п. 95–96)
  communal_monthly numeric(14,2) NOT NULL CHECK (communal_monthly >= 0),
  -- Согласование Ученого совета (п. 95): реквизиты решения
  council_decision text,
  council_date     date,
  -- Обязательства нанимателя (п. 95–96)
  study_credits    int           NOT NULL DEFAULT 0 CHECK (study_credits >= 0),
  internships      int           NOT NULL DEFAULT 0 CHECK (internships >= 0),
  granted_by       uuid          NOT NULL REFERENCES core.users (id),
  granted_at       timestamptz   NOT NULL DEFAULT now(),
  updated_at       timestamptz   NOT NULL DEFAULT now(),
  CONSTRAINT council_decision_has_date CHECK ((council_decision IS NULL) = (council_date IS NULL))
);

COMMENT ON TABLE core.benefit_grants IS
  'FR-1205 (п. 95–96): льготная схема договора и подтверждение ее условий';

CREATE TRIGGER touch_updated_at BEFORE UPDATE ON core.benefit_grants
  FOR EACH ROW EXECUTE FUNCTION core.touch_updated_at();

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.benefit_grants
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- INV-095 и INV-096: условия льготы — правило, а не пожелание.
CREATE FUNCTION core.check_benefit_conditions() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  scheme refdata.benefit_schemes%ROWTYPE;
BEGIN
  SELECT * INTO scheme FROM refdata.benefit_schemes s WHERE s.code = NEW.scheme;

  IF scheme.requires_council AND NEW.council_decision IS NULL THEN
    RAISE EXCEPTION
      'INV-095: льгота применяется по согласованию Ученого совета (п. 95)'
      USING ERRCODE = 'raise_exception';
  END IF;

  IF NEW.study_credits < scheme.min_study_credits THEN
    RAISE EXCEPTION
      'INV-096: спин-офф обучает не менее % кредитов в семестр (п. 96)',
      scheme.min_study_credits
      USING ERRCODE = 'raise_exception';
  END IF;

  IF NEW.internships < scheme.internship_quota THEN
    RAISE EXCEPTION
      'FR-1205: не закрыта квота стажировок — требуется % (п. 95)',
      scheme.internship_quota
      USING ERRCODE = 'raise_exception';
  END IF;

  RETURN NEW;
END $$;

CREATE TRIGGER check_benefit_conditions
  BEFORE INSERT OR UPDATE ON core.benefit_grants
  FOR EACH ROW EXECUTE FUNCTION core.check_benefit_conditions();

-- Плата за год найма (п. 95–96): первый год — коммунальные расходы,
-- дальше доля ставки Прил. 4. Паритет с domain::benefit проверяет тест.
CREATE FUNCTION core.benefit_monthly(p_contract_id uuid, p_year int)
RETURNS numeric
LANGUAGE plpgsql STABLE AS $$
DECLARE
  grant_row core.benefit_grants%ROWTYPE;
  scheme    refdata.benefit_schemes%ROWTYPE;
  base      numeric;
BEGIN
  IF p_year < 1 THEN
    RAISE EXCEPTION 'FR-1205: год найма считается с первого, получено %', p_year;
  END IF;

  SELECT c.monthly_rate INTO base FROM core.contracts c WHERE c.id = p_contract_id;
  IF base IS NULL THEN
    RETURN NULL;
  END IF;

  SELECT * INTO grant_row FROM core.benefit_grants g WHERE g.contract_id = p_contract_id;
  IF grant_row.id IS NULL THEN
    RETURN base;  -- льгота не применяется
  END IF;

  SELECT * INTO scheme FROM refdata.benefit_schemes s WHERE s.code = grant_row.scheme;
  IF NOT scheme.has_schedule THEN
    RETURN base;  -- Ксоц уже внутри ставки (FR-201)
  END IF;

  IF p_year = 1 THEN
    RETURN grant_row.communal_monthly;
  END IF;

  RETURN round(base * scheme.later_share_pct / 100, 2);
END $$;
