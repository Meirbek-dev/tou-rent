-- Досье решений особого порядка и WORM-хранение (М12, М16: FR-1206, FR-1602,
-- INV-042, п. 97, 16.15, 42).
--
-- «Единое досье по каждому решению» ведется тем же механизмом, что и досье
-- тендера (T28): материал попадает в него в момент события — заявка, ее
-- документы, заключение подразделения, решение Правления. Публикации особого
-- порядка ложатся в то же досье задачей T39.
--
-- INV-042: материал досье хранится не менее срока своего предмета —
-- тендерные материалы пять лет, решения особого порядка три года (п. 16.15,
-- 42, FR-1206). Срок считается от момента события (A-075) и живет тремя
-- уровнями: тип домена (`publication::DossierSubject::retention_years`),
-- колонка `retain_until` с триггером (уменьшить срок нельзя, файл у материала
-- не отвязывается) и Object Lock бакета `dossiers` в режиме compliance
-- (infra/compose) — удалить объект до истечения срока не может и администратор.

-- Срок хранения материала: его считает БД, а не вызывающий код (INV-042)
ALTER TABLE core.dossier_items ADD COLUMN retain_until timestamptz;

COMMENT ON COLUMN core.dossier_items.retain_until IS
  'INV-042: материал хранится не менее 5 лет (тендер) или 3 лет (решение особого порядка), п. 16.15, 42';

-- Материалы, попавшие в досье до ввода срока, — тендерные: своего досье
-- у особого порядка не было
UPDATE core.dossier_items
   SET retain_until = occurred_at + interval '5 years'
 WHERE retain_until IS NULL;

ALTER TABLE core.dossier_items ALTER COLUMN retain_until SET NOT NULL;

-- INV-042: срок задается вставкой и после нее только продлевается; материал
-- вместе со своим файлом остается в досье до его истечения.
CREATE FUNCTION core.check_dossier_retention() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF TG_OP = 'INSERT' THEN
    NEW.retain_until := NEW.occurred_at + make_interval(years =>
      CASE WHEN NEW.tender_id IS NOT NULL THEN 5 ELSE 3 END);
    RETURN NEW;
  END IF;

  -- Предмет досье задает срок: подменив его, срок можно было бы сократить
  IF NEW.tender_id IS DISTINCT FROM OLD.tender_id
     OR NEW.special_request_id IS DISTINCT FROM OLD.special_request_id THEN
    RAISE EXCEPTION 'INV-042: предмет досье не переписывается (п. 16.15, 42)';
  END IF;

  IF NEW.retain_until < OLD.retain_until THEN
    RAISE EXCEPTION
      'INV-042: срок хранения материала досье не сокращается (хранится до %), п. 16.15, 42',
      OLD.retain_until;
  END IF;

  -- Обнулить ссылку на файл — то же изъятие материала, только другим
  -- способом; замена одного документа другим (подписанный скан вместо
  -- проекта) — обычный ход дела и запретом не считается
  IF OLD.file_key IS NOT NULL AND NEW.file_key IS NULL THEN
    RAISE EXCEPTION 'INV-042: файл материала досье не отвязывается (FR-1602)';
  END IF;

  RETURN NEW;
END $$;

CREATE TRIGGER check_dossier_retention BEFORE INSERT OR UPDATE ON core.dossier_items
  FOR EACH ROW EXECUTE FUNCTION core.check_dossier_retention();

-- Досье решения собирается так же идемпотентно, как досье тендера (T28)
CREATE UNIQUE INDEX dossier_items_special_source_idx
  ON core.dossier_items (special_request_id, kind, source_table, source_id)
  WHERE special_request_id IS NOT NULL AND source_id IS NOT NULL;

-- Одна точка входа расширяется вторым предметом: у материала либо тендер,
-- либо заявка особого порядка — ключ идемпотентности у каждого свой.
DROP FUNCTION core.record_dossier_item(uuid, text, text, text, text, uuid);

CREATE FUNCTION core.record_dossier_item(
  p_tender_id          uuid,
  p_kind               text,
  p_title              text,
  p_file_key           text,
  p_source_table       text,
  p_source_id          uuid,
  p_special_request_id uuid DEFAULT NULL
) RETURNS void
LANGUAGE plpgsql AS $$
BEGIN
  IF p_tender_id IS NOT NULL THEN
    INSERT INTO core.dossier_items
      (tender_id, kind, title, file_key, source_table, source_id, occurred_at)
    VALUES (p_tender_id, p_kind, p_title, p_file_key, p_source_table, p_source_id, now())
    ON CONFLICT (tender_id, kind, source_table, source_id)
      WHERE tender_id IS NOT NULL AND source_id IS NOT NULL
    DO UPDATE SET file_key = coalesce(EXCLUDED.file_key, core.dossier_items.file_key),
                  title    = coalesce(EXCLUDED.title, core.dossier_items.title);
    RETURN;
  END IF;

  IF p_special_request_id IS NULL THEN
    RETURN;  -- материал вне досье: договор из одного источника вне особого порядка
  END IF;

  INSERT INTO core.dossier_items
    (special_request_id, kind, title, file_key, source_table, source_id, occurred_at)
  VALUES (p_special_request_id, p_kind, p_title, p_file_key, p_source_table, p_source_id, now())
  ON CONFLICT (special_request_id, kind, source_table, source_id)
    WHERE special_request_id IS NOT NULL AND source_id IS NOT NULL
  DO UPDATE SET file_key = coalesce(EXCLUDED.file_key, core.dossier_items.file_key),
                title    = coalesce(EXCLUDED.title, core.dossier_items.title);
END $$;

-- Досье ведется по-русски (делопроизводство, NFR-01), поэтому вывод
-- заключения и решение подписываются словами Правил, а не кодом перечня.
CREATE FUNCTION core.special_decision_ru(p_decision core.special_decision) RETURNS text
LANGUAGE sql IMMUTABLE AS $$
  SELECT CASE p_decision
    WHEN 'grant'    THEN 'предоставить'
    WHEN 'refuse'   THEN 'отказать'
    WHEN 'redirect' THEN 'направить в общий порядок'
  END;
$$;

-- Подпись заявки особого порядка: категория названа так же, как в каталоге
CREATE FUNCTION core.special_request_title(p_request_id uuid) RETURNS text
LANGUAGE sql STABLE AS $$
  SELECT 'Заявка особого порядка: ' || c.label_ru || ' (' || c.rule_ref || ')'
  FROM core.special_requests r
  JOIN refdata.special_categories c ON c.code = r.category
  WHERE r.id = p_request_id;
$$;

-- Заявка (Прил. 3, п. 88)
CREATE FUNCTION core.dossier_on_special_request() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  PERFORM core.record_dossier_item(
    NULL, 'application', core.special_request_title(NEW.id),
    NULL, 'core.special_requests', NEW.id, NEW.id);
  RETURN NULL;
END $$;

CREATE TRIGGER dossier_on_special_request AFTER INSERT ON core.special_requests
  FOR EACH ROW EXECUTE FUNCTION core.dossier_on_special_request();

-- Документы заявки (п. 88): каждый лежит в досье вместе со своим файлом
CREATE FUNCTION core.dossier_on_special_file() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  PERFORM core.record_dossier_item(
    NULL, 'application', 'Документ заявки: ' || NEW.filename,
    NEW.file_key, 'core.special_request_files', NEW.id, NEW.special_request_id);
  RETURN NULL;
END $$;

CREATE TRIGGER dossier_on_special_file AFTER INSERT ON core.special_request_files
  FOR EACH ROW EXECUTE FUNCTION core.dossier_on_special_file();

-- Заключение уполномоченного подразделения (п. 89)
CREATE FUNCTION core.dossier_on_special_review() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  PERFORM core.record_dossier_item(
    NULL, 'review',
    'Заключение подразделения (вывод: ' || core.special_decision_ru(NEW.recommendation) || ')',
    NULL, 'core.special_reviews', NEW.id, NEW.special_request_id);
  RETURN NULL;
END $$;

CREATE TRIGGER dossier_on_special_review AFTER INSERT ON core.special_reviews
  FOR EACH ROW EXECUTE FUNCTION core.dossier_on_special_review();

-- Решение Правления и его протокол (п. 90, 97): печатная форма догружается
-- после решения, поэтому триггер стоит и на обновлении.
CREATE FUNCTION core.dossier_on_special_decision() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  PERFORM core.record_dossier_item(
    NULL, 'decision',
    'Решение Правления: ' || core.special_decision_ru(NEW.decision),
    NEW.pdf_key, 'core.special_board_decisions', NEW.id, NEW.special_request_id);
  RETURN NULL;
END $$;

CREATE TRIGGER dossier_on_special_decision
  AFTER INSERT OR UPDATE ON core.special_board_decisions
  FOR EACH ROW EXECUTE FUNCTION core.dossier_on_special_decision();

-- Заявки, рассмотренные до ввода досье решений: события прошлого триггеры
-- не видели, поэтому материалы переносятся один раз — теми же правилами.
INSERT INTO core.dossier_items
  (special_request_id, kind, title, file_key, source_table, source_id, occurred_at)
SELECT r.id, 'application', core.special_request_title(r.id),
       NULL, 'core.special_requests', r.id, r.submitted_at
FROM core.special_requests r
ON CONFLICT (special_request_id, kind, source_table, source_id)
  WHERE special_request_id IS NOT NULL AND source_id IS NOT NULL
DO NOTHING;

INSERT INTO core.dossier_items
  (special_request_id, kind, title, file_key, source_table, source_id, occurred_at)
SELECT f.special_request_id, 'application', 'Документ заявки: ' || f.filename,
       f.file_key, 'core.special_request_files', f.id, f.uploaded_at
FROM core.special_request_files f
ON CONFLICT (special_request_id, kind, source_table, source_id)
  WHERE special_request_id IS NOT NULL AND source_id IS NOT NULL
DO NOTHING;

INSERT INTO core.dossier_items
  (special_request_id, kind, title, file_key, source_table, source_id, occurred_at)
SELECT v.special_request_id, 'review',
       'Заключение подразделения (вывод: ' || core.special_decision_ru(v.recommendation) || ')',
       NULL, 'core.special_reviews', v.id, v.created_at
FROM core.special_reviews v
ON CONFLICT (special_request_id, kind, source_table, source_id)
  WHERE special_request_id IS NOT NULL AND source_id IS NOT NULL
DO NOTHING;

INSERT INTO core.dossier_items
  (special_request_id, kind, title, file_key, source_table, source_id, occurred_at)
SELECT d.special_request_id, 'decision',
       'Решение Правления: ' || core.special_decision_ru(d.decision),
       d.pdf_key, 'core.special_board_decisions', d.id, d.decided_at
FROM core.special_board_decisions d
ON CONFLICT (special_request_id, kind, source_table, source_id)
  WHERE special_request_id IS NOT NULL AND source_id IS NOT NULL
DO NOTHING;
