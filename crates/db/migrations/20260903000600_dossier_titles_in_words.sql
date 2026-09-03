-- Материал досье называется словами, а не кодом перечня (FR-1602, п. 16, 42).
--
-- Круг 4 гаунтлета (G-118). Подписи материалов собирались в SQL прямо из
-- значения столбца-перечисления:
--
--   «Уклонение от подписания договора: documents_deadline_missed»
--   «Протокол admission №Д-4f2a1c»
--   «Публикация протокола results (п. 75)»
--
-- Досье - делопроизводственный документ (NFR-01): его читает секретарь,
-- организатор и проверяющий, и английский код перечня в русской строке
-- не читается никем. У основания уклонения формулировка уже есть -
-- `refdata.evasion_grounds.label_ru` (п. 116), и подпись обязана брать ее,
-- а не код: тогда правка справочника доходит до досье сама.
--
-- У вида протокола своего справочника нет - перечисление
-- `core.protocol_kind` заведено типом. Слова для него те же, что у
-- каталога интерфейса (`protocol_kind_*` в apps/web/messages): «допуска»,
-- «итогов», «о несостоявшемся тендере», «о победителе № 2». Родительный
-- падеж выбран потому, что подпись строится двумя способами - «Протокол
-- итогов» и «Публикация протокола итогов», - и обе формы требуют его.
--
-- Триггеры переопределяются целиком (`CREATE OR REPLACE`): переписать
-- примененную миграцию нельзя (контрольная сумма sqlx).

-- Вид протокола по-русски: одна формулировка на все подписи досье
CREATE OR REPLACE FUNCTION core.protocol_kind_ru(p_kind core.protocol_kind) RETURNS text
LANGUAGE sql IMMUTABLE AS $$
  SELECT CASE p_kind
    WHEN 'admission' THEN 'допуска'
    WHEN 'results'   THEN 'итогов'
    WHEN 'failed'    THEN 'о несостоявшемся тендере'
    WHEN 'winner2'   THEN 'о победителе № 2'
  END;
$$;

COMMENT ON FUNCTION core.protocol_kind_ru(core.protocol_kind) IS
  'FR-1602: вид протокола словами делопроизводства (п. 55, 73–76)';

-- Протокол комиссии и факт его публикации (п. 55, 73–76)
CREATE OR REPLACE FUNCTION core.dossier_on_protocol() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  PERFORM core.record_dossier_item(
    NEW.tender_id, 'protocol',
    'Протокол ' || core.protocol_kind_ru(NEW.kind) || coalesce(' №' || NEW.number, ''),
    NEW.pdf_key, 'core.protocols', NEW.id);

  IF NEW.published_at IS NOT NULL THEN
    PERFORM core.record_dossier_item(
      NEW.tender_id, 'publication',
      CASE
        WHEN NEW.unpublished_at IS NOT NULL
          THEN 'Публикация протокола ' || core.protocol_kind_ru(NEW.kind)
               || ' снята по истечении 6 месяцев (п. 76)'
        ELSE 'Публикация протокола ' || core.protocol_kind_ru(NEW.kind) || ' (п. 75)'
      END,
      NEW.pdf_key, 'core.protocols', NEW.id);
  END IF;

  RETURN NULL;
END $$;

-- Уклонение от подписания договора (п. 116): основание - из справочника
CREATE OR REPLACE FUNCTION core.dossier_on_evasion() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  ground_ru text;
BEGIN
  -- FK на refdata.evasion_grounds гарантирует строку справочника; coalesce
  -- оставлен на случай основания, заведенного без формулировки
  SELECT g.label_ru INTO ground_ru
  FROM refdata.evasion_grounds g WHERE g.code = NEW.ground;

  PERFORM core.record_dossier_item(
    NEW.tender_id, 'evasion',
    'Уклонение от подписания договора: ' || coalesce(ground_ru, NEW.ground),
    NULL, 'core.evasions', NEW.id);
  RETURN NULL;
END $$;

-- --- Разовый перенос уже собранных подписей --------------------------------
--
-- Материал досье не изымается и не переписывается (INV-042), но подпись
-- в перечень запрещенных изменений не входит: это название документа,
-- а не сам документ. Условие каждого UPDATE воспроизводит прежнюю
-- подпись целиком - повторный накат миграции не находит таких строк
-- и не трогает ничего.

UPDATE core.dossier_items d
   SET title = 'Протокол ' || core.protocol_kind_ru(p.kind) || coalesce(' №' || p.number, '')
  FROM core.protocols p
 WHERE d.source_table = 'core.protocols' AND d.source_id = p.id AND d.kind = 'protocol'
   AND d.title = 'Протокол ' || p.kind::text || coalesce(' №' || p.number, '');

UPDATE core.dossier_items d
   SET title = CASE
         WHEN p.unpublished_at IS NOT NULL
           THEN 'Публикация протокола ' || core.protocol_kind_ru(p.kind)
                || ' снята по истечении 6 месяцев (п. 76)'
         ELSE 'Публикация протокола ' || core.protocol_kind_ru(p.kind) || ' (п. 75)'
       END
  FROM core.protocols p
 WHERE d.source_table = 'core.protocols' AND d.source_id = p.id AND d.kind = 'publication'
   AND d.title IN ('Публикация протокола ' || p.kind::text || ' (п. 75)',
                   'Публикация протокола ' || p.kind::text
                     || ' снята по истечении 6 месяцев (п. 76)');

UPDATE core.dossier_items d
   SET title = 'Уклонение от подписания договора: ' || g.label_ru
  FROM core.evasions e
  JOIN refdata.evasion_grounds g ON g.code = e.ground
 WHERE d.source_table = 'core.evasions' AND d.source_id = e.id AND d.kind = 'evasion'
   AND d.title = 'Уклонение от подписания договора: ' || e.ground;
