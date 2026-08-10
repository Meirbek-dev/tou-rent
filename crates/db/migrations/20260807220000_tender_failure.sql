-- Несостоявшийся тендер (М8, FR-801–802, п. 81–83): закрытый перечень
-- оснований, обязательность основания при переходе в `failed` и следствие
-- (повтор, договор из одного источника, вопрос Правлению).

-- Основания п. 81 — справочник + FK, как у оснований отклонения (INV-052).
-- TODO-ENGINEER: нумерация подпунктов п. 81 сверяется по Правилам (Q-004).
CREATE TABLE refdata.failure_grounds (
  code     text PRIMARY KEY,
  label_ru text NOT NULL,
  label_kk text,
  label_en text,
  rule_ref text NOT NULL
);

INSERT INTO refdata.failure_grounds (code, label_ru, label_kk, label_en, rule_ref) VALUES
  ('no_applications', 'Не подано ни одной заявки',
   'Бірде-бір өтінім берілмеді', 'No applications submitted', 'п. 81.1'),
  ('single_application', 'Подана единственная заявка',
   'Жалғыз өтінім берілді', 'Only one application submitted', 'п. 81.2'),
  ('fewer_than_two_admitted', 'К торгам допущено менее двух участников',
   'Саудаға екеуден аз қатысушы жіберілді', 'Fewer than two participants qualified', 'п. 81.3'),
  ('winners_evaded', 'Победитель и участник № 2 уклонились от подписания договора',
   'Жеңімпаз бен №2 қатысушы шартқа қол қоюдан жалтарды',
   'Both the winner and the runner-up evaded signing', 'п. 81.4')
ON CONFLICT DO NOTHING;

-- Следствие несостоявшегося тендера (п. 82–83)
CREATE TYPE core.failure_consequence AS ENUM ('repeat', 'single_source', 'board_referral');

ALTER TABLE core.tenders
  ADD COLUMN failure_ground text REFERENCES refdata.failure_grounds (code),
  ADD COLUMN failed_at      timestamptz,
  ADD COLUMN consequence    core.failure_consequence;

COMMENT ON COLUMN core.tenders.failure_ground IS
  'FR-801: основание признания несостоявшимся из закрытого перечня п. 81';
COMMENT ON COLUMN core.tenders.consequence IS
  'FR-802: следствие — повторный тендер, договор из одного источника либо вопрос Правлению (п. 82–83)';

-- Переход в `failed` — только с основанием (FR-801): «не состоялся»
-- по усмотрению объявить нельзя. Обратный переход основание не стирает:
-- оно остается в истории тендера и в протоколе.
CREATE FUNCTION core.check_failure_ground() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.status = 'failed' AND OLD.status IS DISTINCT FROM 'failed'
     AND NEW.failure_ground IS NULL THEN
    RAISE EXCEPTION 'FR-801: тендер признается несостоявшимся только по основанию п. 81';
  END IF;
  IF NEW.status = 'failed' AND OLD.status IS DISTINCT FROM 'failed' THEN
    NEW.failed_at := coalesce(NEW.failed_at, now());
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER check_failure_ground BEFORE UPDATE ON core.tenders
  FOR EACH ROW EXECUTE FUNCTION core.check_failure_ground();

-- Следствие фиксируется только у несостоявшегося тендера
ALTER TABLE core.tenders
  ADD CONSTRAINT consequence_needs_failure
  CHECK (consequence IS NULL OR failure_ground IS NOT NULL);
