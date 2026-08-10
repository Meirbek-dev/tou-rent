-- Особый порядок: категории и заявка (М12, FR-1201, п. 87–88).
--
-- Категория — не свободный текст, а закрытый перечень из 13 позиций (п. 87):
-- справочник + FK (INV-087), паритет с enum домена проверяет тест — тот же
-- прием, что у оснований отклонения (INV-052). Каждая категория декларирует
-- то, что Правила связывают именно с ней: требуемые документы, срок проверки,
-- льготную схему и публикуемость. Значения деклараций — данные, а не код:
-- заполняются по Правилам без правки приложения.
--
-- TODO-ENGINEER: наименования 13 категорий и их требования агенту недоступны
-- (первоисточник — PDF Правил, вопрос Q-009). Категории заведены по номерам
-- п. 87.1–87.13 — тем же способом, каким на них ссылается само ТЗ
-- («категории 4–5», FR-1203); подписи и требования ниже заведомо черновые.

-- Состояние заявки особого порядка (п. 88–90). Решение Правления из закрытого
-- перечня «предоставить / отказать / направить в общий порядок» (FR-1202)
-- ложится на три терминальных состояния; сами решения — задача T34.
CREATE TYPE core.special_request_status AS ENUM
  ('submitted', 'under_review', 'granted', 'refused', 'redirected', 'withdrawn');

-- Вид срока Правил: рабочие или календарные дни (паритет с domain::obligation::Term)
CREATE TYPE core.term_kind AS ENUM ('business', 'calendar');

-- Льготные схемы (FR-1205, п. 95–96, Прил. 4): категория объявляет, какая
-- схема к ней применяется; сам расчет льготы — задача T37.
CREATE TABLE refdata.benefit_schemes (
  code     text PRIMARY KEY,
  label_ru text NOT NULL,
  label_kk text,
  label_en text,
  rule_ref text NOT NULL
);

INSERT INTO refdata.benefit_schemes (code, label_ru, label_kk, label_en, rule_ref) VALUES
  ('none', 'Льгота не применяется', 'Жеңілдік қолданылмайды', 'No benefit', 'п. 87'),
  ('educational_equipment', 'Оборудование в образовательном процессе',
   'Білім беру процесіндегі жабдық', 'Equipment used in the educational process', 'п. 95'),
  ('spin_off', 'Спин-офф компании университета', 'Университеттің спин-офф компаниясы',
   'University spin-off company', 'п. 96'),
  ('social', 'Социальный арендатор (Ксоц)', 'Әлеуметтік жалдаушы (Кәлеум)',
   'Social tenant (social factor)', 'Прил. 4')
ON CONFLICT DO NOTHING;

-- INV-087: категория особого порядка — только из перечня п. 87, без catch-all.
CREATE TABLE refdata.special_categories (
  code           text           PRIMARY KEY,
  ordinal        int            NOT NULL UNIQUE CHECK (ordinal BETWEEN 1 AND 13),
  label_ru       text           NOT NULL,
  label_kk       text,
  label_en       text,
  rule_ref       text           NOT NULL,  -- подпункт п. 87
  -- Срок проверки уполномоченным подразделением (FR-1202, п. 89)
  review_days    int            NOT NULL CHECK (review_days > 0),
  review_term    core.term_kind NOT NULL,
  benefit_scheme text           NOT NULL REFERENCES refdata.benefit_schemes (code),
  -- Публикуются ли результаты по категории (FR-1403, п. 90, 97)
  publishable    boolean        NOT NULL
);

COMMENT ON TABLE refdata.special_categories IS
  'INV-087 (FR-1201, п. 87): закрытый перечень 13 категорий особого порядка; '
  'каждая объявляет срок проверки, льготную схему и публикуемость';

-- Требуемые документы категории (п. 88): перечень объявляется данными,
-- заявитель закрывает позиции вложениями.
CREATE TABLE refdata.special_category_documents (
  category_code text    NOT NULL REFERENCES refdata.special_categories (code),
  code          text    NOT NULL,
  ordinal       int     NOT NULL CHECK (ordinal > 0),
  label_ru      text    NOT NULL,
  label_kk      text,
  label_en      text,
  required      boolean NOT NULL DEFAULT true,
  PRIMARY KEY (category_code, code)
);

-- Тринадцать категорий п. 87. Срок проверки по умолчанию — 15 календарных
-- дней (FR-1202, п. 89); сокращенный срок 10 рабочих дней для малых помещений
-- и сервисной инфраструктуры выставляется тем категориям, к которым он
-- относится по Правилам (TODO-ENGINEER, Q-009). Публикуемость взята из п. 97
-- (результаты особого порядка публикуются) и уточняется по категориям.
INSERT INTO refdata.special_categories
  (code, ordinal, label_ru, label_kk, label_en, rule_ref,
   review_days, review_term, benefit_scheme, publishable)
SELECT
  'category_' || n,
  n,
  'Категория № ' || n || ' (наименование уточняется по Правилам)',
  'Санат № ' || n || ' (атауы Ережелер бойынша нақтыланады)',
  'Category no. ' || n || ' (name to be confirmed against the Rules)',
  'п. 87.' || n,
  15,
  'calendar',
  'none',
  true
FROM generate_series(1, 13) AS n
ON CONFLICT DO NOTHING;

-- Перечень документов каждой категории — из Правил (п. 88). До ответа Q-009
-- у категории одна заведомо черновая позиция: механизм работает, состав пуст.
INSERT INTO refdata.special_category_documents
  (category_code, code, ordinal, label_ru, label_kk, label_en, required)
SELECT
  'category_' || n,
  'documents_pending',
  1,
  'Перечень документов категории уточняется (п. 88)',
  'Санат құжаттарының тізбесі нақтыланады (88-т.)',
  'The document list for this category is to be confirmed (cl. 88)',
  false
FROM generate_series(1, 13) AS n
ON CONFLICT DO NOTHING;

-- Заявка особого порядка (Прил. 3). Таблица-каркас заведена миграцией
-- notifications_special_dossier и до сих пор не наполнялась: приложение
-- не имело ни одного пути записи в нее, поэтому строк нет ни на чистой БД,
-- ни в дампе — колонки получают NOT NULL после разовой подстановки.
ALTER TABLE core.special_requests RENAME COLUMN payload TO applicant_details;

COMMENT ON COLUMN core.special_requests.applicant_details IS
  'Сведения заявителя Прил. 3 (персональные данные — NFR-07: в логи не выводить)';

ALTER TABLE core.special_requests
  ADD COLUMN applicant_kind   core.applicant_kind,
  ADD COLUMN object_id        uuid REFERENCES core.objects (id),
  ADD COLUMN purpose          text,
  ADD COLUMN requested_months int CHECK (requested_months > 0),
  ADD COLUMN submitted_at     timestamptz NOT NULL DEFAULT now(),
  ADD COLUMN withdrawn_at     timestamptz;

UPDATE core.special_requests
   SET applicant_kind = coalesce(applicant_kind, 'legal_entity'),
       purpose        = coalesce(purpose, '(не указано)');

ALTER TABLE core.special_requests
  ALTER COLUMN applicant_kind SET NOT NULL,
  ALTER COLUMN purpose        SET NOT NULL;

-- INV-087: категория заявки — только из закрытого перечня п. 87
ALTER TABLE core.special_requests
  ADD CONSTRAINT special_requests_category_fkey
  FOREIGN KEY (category) REFERENCES refdata.special_categories (code);

-- Статус — закрытый перечень вместо свободного текста каркаса
ALTER TABLE core.special_requests ALTER COLUMN status DROP DEFAULT;
ALTER TABLE core.special_requests
  ALTER COLUMN status TYPE core.special_request_status
  USING status::core.special_request_status;
ALTER TABLE core.special_requests ALTER COLUMN status SET DEFAULT 'submitted';

ALTER TABLE core.special_requests
  ADD CONSTRAINT special_request_purpose_not_empty CHECK (length(btrim(purpose)) > 0),
  ADD CONSTRAINT special_request_withdrawal_has_timestamp
    CHECK (status <> 'withdrawn' OR withdrawn_at IS NOT NULL);

-- Список кабинета: заявки заявителя свежими сверху (индекс по applicant_id
-- уже есть с миграции invariant_hardening — здесь добавляется порядок)
CREATE INDEX special_requests_applicant_submitted_idx
  ON core.special_requests (applicant_id, submitted_at DESC);

-- Порядок состояний заявки (п. 88–90): решение принимается по результатам
-- проверки, а принятое решение и отзыв заявителя окончательны. Правило живет
-- в БД, потому что оно относится к фактам, а не к экрану (паритет с доменом).
CREATE FUNCTION core.check_special_request_transition() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.status = OLD.status THEN
    RETURN NEW;
  END IF;

  IF NOT (
    (OLD.status = 'submitted'    AND NEW.status IN ('under_review', 'withdrawn')) OR
    (OLD.status = 'under_review' AND NEW.status IN ('granted', 'refused', 'redirected', 'withdrawn'))
  ) THEN
    RAISE EXCEPTION 'FR-1201: переход заявки особого порядка % → % запрещен (п. 88–90)',
      OLD.status, NEW.status;
  END IF;

  IF NEW.status = 'withdrawn' AND NEW.withdrawn_at IS NULL THEN
    NEW.withdrawn_at := now();  -- время отзыва задает сервер (NFR-03)
  END IF;

  RETURN NEW;
END $$;

CREATE TRIGGER check_special_request_transition BEFORE UPDATE ON core.special_requests
  FOR EACH ROW EXECUTE FUNCTION core.check_special_request_transition();

-- Мутация домена — в аудит (регламент А.5, перечень INV-AUDIT)
CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.special_requests
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Документы заявки (п. 88): вложение закрывает позицию перечня своей категории
CREATE TABLE core.special_request_files (
  id                 uuid        PRIMARY KEY DEFAULT uuidv7(),
  special_request_id uuid        NOT NULL REFERENCES core.special_requests (id) ON DELETE CASCADE,
  document_code      text,       -- позиция перечня категории; NULL — прочий документ
  file_key           text        NOT NULL,  -- RustFS (бакет dossiers)
  filename           text        NOT NULL,
  content_type       text        NOT NULL,
  size_bytes         bigint      NOT NULL CHECK (size_bytes >= 0),
  uploaded_at        timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX special_request_files_request_idx
  ON core.special_request_files (special_request_id);

-- Позиция перечня принадлежит категории заявки: составного FK на пару
-- (категория заявки, код документа) в схеме нет, поэтому проверяет триггер.
CREATE FUNCTION core.check_special_request_document() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  request_category text;
BEGIN
  IF NEW.document_code IS NULL THEN
    RETURN NEW;
  END IF;

  SELECT category INTO request_category
  FROM core.special_requests WHERE id = NEW.special_request_id;

  IF NOT EXISTS (
    SELECT 1 FROM refdata.special_category_documents d
    WHERE d.category_code = request_category AND d.code = NEW.document_code
  ) THEN
    RAISE EXCEPTION
      'FR-1201: документ % не объявлен категорией % (п. 87–88)',
      NEW.document_code, request_category;
  END IF;

  RETURN NEW;
END $$;

CREATE TRIGGER check_special_request_document
  BEFORE INSERT OR UPDATE ON core.special_request_files
  FOR EACH ROW EXECUTE FUNCTION core.check_special_request_document();

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.special_request_files
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Права на новые таблицы приходят из ALTER DEFAULT PRIVILEGES (миграция
-- app_role_grants): справочники — чтение, core — полный DML.
