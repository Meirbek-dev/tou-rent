-- Досье тендера получает объявление и тендерную документацию (FR-1602, п. 16, 42).
--
-- Круг 3 гаунтлета (G-103). Досье собирается триггерами в момент события -
-- заявка, протокол, публикация протокола, договор, акт, уклонение, редакция
-- документации. Двух событий в этом перечне не было:
--
--   * публикации объявления (draft -> announced): `core.dossier_items` знал
--     вид `announcement` только по строкам `core.tender_amendments`, то есть
--     по РЕДАКЦИЯМ документации. У тендера без редакций объявления в досье
--     не было вовсе;
--   * загрузки тендерной документации (`core.tender_docs`): у таблицы был
--     аудит и индекс, но не было триггера досье.
--
-- Отсюда наблюдаемое: `GET /api/v1/tenders/{id}/dossier` -> `[]`, а
-- `dossier.zip` - архив из одного манифеста. Досье - доказательная база
-- (п. 16, 42), и первые два документа процедуры в нее не попадали.
--
-- Оба события закрываются тем же механизмом, что и остальные: триггер
-- вызывает `core.record_dossier_item`, ключ идемпотентности -
-- (tender_id, kind, source_table, source_id), повторное событие дубля не
-- порождает. Источники разные (`core.tenders` и `core.tender_docs`),
-- поэтому объявление и каждый файл документации - отдельные материалы
-- одного раздела `announcement` («объявление и редакции документации»).
--
-- Файл у объявления не записывается: печатная форма Прил. 1 собирается на
-- лету (`GET /api/v1/tenders/{id}/announcement.pdf`) и в хранилище при
-- публикации не кладется - в отличие от редакции, которая сохраняет свой
-- снимок. Материал без файла досье знает: так же лежат заявки и уклонения.

-- --- 1. Объявление о тендере ----------------------------------------------
CREATE OR REPLACE FUNCTION core.dossier_on_tender_announcement() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  -- Черновик не опубликован (п. 5–6): в досье попадать нечему
  IF NEW.status = 'draft' OR NEW.announced_at IS NULL THEN
    RETURN NULL;
  END IF;

  -- Обычная правка опубликованного тендера (сроки, ссылка на запись, статус
  -- дальше по процедуре) объявления не переиздает - материал уже записан
  IF TG_OP = 'UPDATE'
     AND OLD.status <> 'draft'
     AND OLD.announced_at IS NOT DISTINCT FROM NEW.announced_at
     AND OLD.title IS NOT DISTINCT FROM NEW.title THEN
    RETURN NULL;
  END IF;

  PERFORM core.record_dossier_item(
    NEW.id, 'announcement',
    'Объявление о тендере: ' || NEW.title,
    NULL, 'core.tenders', NEW.id);
  RETURN NULL;
END $$;

COMMENT ON FUNCTION core.dossier_on_tender_announcement() IS
  'FR-1602: объявление ложится в досье в момент публикации (п. 5–6, 16)';

DROP TRIGGER IF EXISTS dossier_on_tender_announcement ON core.tenders;
CREATE TRIGGER dossier_on_tender_announcement AFTER INSERT OR UPDATE ON core.tenders
  FOR EACH ROW EXECUTE FUNCTION core.dossier_on_tender_announcement();

-- --- 2. Тендерная документация --------------------------------------------
CREATE OR REPLACE FUNCTION core.dossier_on_tender_doc() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  PERFORM core.record_dossier_item(
    NEW.tender_id, 'announcement',
    'Тендерная документация (ред. ' || NEW.version || '): ' || NEW.title,
    NEW.file_key, 'core.tender_docs', NEW.id);
  RETURN NULL;
END $$;

COMMENT ON FUNCTION core.dossier_on_tender_doc() IS
  'FR-1602: тендерная документация ложится в досье при загрузке (п. 16, 42)';

DROP TRIGGER IF EXISTS dossier_on_tender_doc ON core.tender_docs;
CREATE TRIGGER dossier_on_tender_doc AFTER INSERT OR UPDATE ON core.tender_docs
  FOR EACH ROW EXECUTE FUNCTION core.dossier_on_tender_doc();

-- --- 3. Досье уже опубликованных тендеров ---------------------------------
-- События прошлого триггеры не видели: материалы переносятся один раз
-- теми же правилами (как это делали 20260808080000 и 20260809000000).
INSERT INTO core.dossier_items
  (tender_id, kind, title, file_key, source_table, source_id, occurred_at)
SELECT t.id, 'announcement',
       'Объявление о тендере: ' || t.title,
       NULL, 'core.tenders', t.id, coalesce(t.announced_at, t.created_at)
FROM core.tenders t
WHERE t.status <> 'draft'
ON CONFLICT (tender_id, kind, source_table, source_id)
  WHERE tender_id IS NOT NULL AND source_id IS NOT NULL
DO NOTHING;

INSERT INTO core.dossier_items
  (tender_id, kind, title, file_key, source_table, source_id, occurred_at)
SELECT d.tender_id, 'announcement',
       'Тендерная документация (ред. ' || d.version || '): ' || d.title,
       d.file_key, 'core.tender_docs', d.id, d.published_at
FROM core.tender_docs d
ON CONFLICT (tender_id, kind, source_table, source_id)
  WHERE tender_id IS NOT NULL AND source_id IS NOT NULL
DO NOTHING;
