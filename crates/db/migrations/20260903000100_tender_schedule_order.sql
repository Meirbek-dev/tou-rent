-- Порядок сроков тендера - ограничение таблицы, а не только домена
-- (FR-303, п. 5–6, 36, 50, 59).
--
-- Круг 2 гаунтлета (G-043): у `core.tenders` было ровно два CHECK -
-- `deadline_before_opening` и `opened_not_before_meeting`. Обе крайние
-- отметки процедуры - публикация и торги - не были связаны ни с чем:
--
--   UPDATE core.tenders SET announced_at = submission_deadline + '1 day';
--     -> UPDATE 1, объявление «вышло» после окончания приема заявок
--   UPDATE core.tenders SET trading_at = opening_at - '1 day';
--     -> UPDATE 1, торги «прошли» до вскрытия конвертов
--
-- Порядок стерег только домен (`tender::Schedule::validate`), то есть
-- ровно один путь - вкладка «Сроки тендеров» кабинета админа (М15).
-- Любая другая запись в таблицу - миграция данных, ручная правка
-- дежурного, будущий обработчик - проходила мимо. Инвариант, у которого
-- нижний рубеж пустой, держится до первого нового вызова.
--
-- Ограничения объявлены NOT VALID: на прод-дампе уже лежат строки, и
-- разошедшиеся сроки прежних тендеров - это факт истории, а не повод
-- уронить накат. Новые и изменяемые строки проверяются с этой миграции.
--
-- Чего здесь намеренно НЕТ: окна в десять календарных дней между
-- публикацией и вскрытием (FR-303, п. 5). Оно проверяется при переходе
-- в `announced` (триггер `enforce_tender_transition`) и обязано остаться
-- условием публикации, а не свойством строки:
--
--   * § 9.1 стенда. `seed_demo_tender` доводит демо-тендер до вскрытия
--     сдвигом отметок (`crates/db/src/seed.rs`): публикация остается
--     на `core.now() - 2 days`, а прием и вскрытие переносятся в
--     `core.now() - 1 hour` / `- 30 minutes`. Разрыв - двое суток;
--     CHECK на десять дней сделал бы демо-стенд неподнимаемым.
--   * Правка записи под вышедшее объявление (М15) обязана уметь
--     отразить сроки такими, как они опубликованы, - в том числе те,
--     что университет объявил с меньшим разрывом.
--
-- Имена ограничений разбирает `RuleViolation::from_constraint`
-- (crates/domain/src/rule.rs): отказ приезжает наверх причиной перечня
-- `tender_schedule_order`, а не безымянным 23514.

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint
    WHERE conname = 'announced_before_deadline'
      AND conrelid = 'core.tenders'::regclass
  ) THEN
    ALTER TABLE core.tenders
      ADD CONSTRAINT announced_before_deadline
      CHECK (announced_at IS NULL OR submission_deadline IS NULL
             OR announced_at < submission_deadline)
      NOT VALID;
  END IF;

  IF NOT EXISTS (
    SELECT 1 FROM pg_constraint
    WHERE conname = 'opening_before_trading'
      AND conrelid = 'core.tenders'::regclass
  ) THEN
    ALTER TABLE core.tenders
      ADD CONSTRAINT opening_before_trading
      CHECK (opening_at IS NULL OR trading_at IS NULL
             OR opening_at <= trading_at)
      NOT VALID;
  END IF;
END $$;

COMMENT ON CONSTRAINT announced_before_deadline ON core.tenders IS
  'FR-303: объявление публикуется раньше окончания приема заявок (п. 5–6, 36)';
COMMENT ON CONSTRAINT opening_before_trading ON core.tenders IS
  'FR-303: торги идут не раньше вскрытия конвертов (п. 50, 59)';
