-- Результат сверки hash-цепочки аудита (INV-A01, FR-1601).
--
-- Цепочка сверяется фоновым проходом, но единственным следом сверки была
-- строка в stdout контейнера. Наблюдаемости у бэкенда нет (арх. v3 § 8,
-- Q-018): ни метрик, ни алертов, ни экспорта логов наружу, а журнал
-- контейнера ротируется (json-file, 20m × 5). То есть событие, ради
-- обнаружения которого построена вся конструкция append-only, обнаруживалось
-- и тут же терялось: ни даты последней успешной сверки, ни счетчика.
-- Результат теперь лежит там же, где и остальные доказательства, - в БД.

-- Разбор цепочки за один проход: целостность, размер журнала и место
-- первого расхождения. audit.verify_chain() возвращал только boolean,
-- поэтому «разорвана» было нечем уточнить - разбирательство начиналось
-- с полного пересчета вручную.
--
-- entries считается отдельно от цикла: на разорванной цепочке цикл
-- прерывается на первом расхождении, а размер журнала нужен полный -
-- иначе число записей значило бы разное у целой и у разорванной цепочки.
CREATE FUNCTION audit.verify_chain_report(
  OUT intact    boolean,
  OUT entries   bigint,
  OUT broken_at bigint   -- audit.log.id первой разошедшейся записи
)
LANGUAGE plpgsql STABLE AS $$
DECLARE
  rec  record;
  prev bytea := NULL;
BEGIN
  intact    := true;
  broken_at := NULL;
  SELECT count(*) INTO entries FROM audit.log;

  FOR rec IN SELECT * FROM audit.log ORDER BY id LOOP
    IF rec.prev_hash IS DISTINCT FROM prev
       OR rec.row_hash <> sha256(coalesce(prev, ''::bytea) || convert_to(rec.payload::text, 'UTF8'))
    THEN
      intact    := false;
      broken_at := rec.id;
      RETURN;  -- после первого расхождения остаток цепочки недостоверен
    END IF;
    prev := rec.row_hash;
  END LOOP;
END $$;

-- Прежняя проверка остается точкой входа гейта G15 и тестов, но своей копии
-- логики цепочки больше не держит: две реализации одного инварианта
-- разъезжаются молча.
CREATE OR REPLACE FUNCTION audit.verify_chain() RETURNS boolean
LANGUAGE sql STABLE AS $$
  SELECT intact FROM audit.verify_chain_report();
$$;

-- Журнал сверок: только дописывается, как и сам audit.log.
CREATE TABLE audit.chain_checks (
  id         bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  checked_at timestamptz NOT NULL DEFAULT core.now(),
  intact     boolean     NOT NULL,
  entries    bigint      NOT NULL CHECK (entries >= 0),
  broken_at  bigint,
  -- Место расхождения есть тогда и только тогда, когда цепочка разорвана:
  -- «разорвана, но неизвестно где» - это не результат, а потерянный результат
  CONSTRAINT chain_checks_break_located CHECK (intact = (broken_at IS NULL))
);

COMMENT ON TABLE audit.chain_checks IS
  'Результаты сверки hash-цепочки аудита (INV-A01); только дописывается';

-- Кабинет админа спрашивает две вещи: последнюю сверку и последнюю успешную.
-- Первую отдает первичный ключ (id монотонен порядку записи), второй нужен
-- частичный индекс - иначе на разорванной базе вопрос «когда цепочка
-- последний раз сходилась» перебирал бы весь журнал сверок.
CREATE INDEX chain_checks_intact_idx ON audit.chain_checks (id DESC) WHERE intact;

-- INV-A01: результат сверки не переписывается и не удаляется
CREATE TRIGGER chain_checks_append_only BEFORE UPDATE OR DELETE ON audit.chain_checks
  FOR EACH ROW EXECUTE FUNCTION core.forbid_mutation('INV-A01');

-- Audit-триггера (audit.record) здесь нет, и это не пропуск:
--   1) перечень INV-AUDIT (specs/INVENTORY.md) закрывает мутации домена
--      в схеме core; сверка домен не меняет - это наблюдение за журналом,
--      а не событие процесса;
--   2) триггер писал бы в audit.log запись на каждую сверку, то есть
--      проверка удлиняла бы ту самую цепочку, которую проверяет, и каждый
--      следующий проход стоил бы дороже предыдущего - рост без предела;
--   3) от правки задним числом эта таблица защищена тем же способом, что
--      и audit.log: append-only триггер плюс отсутствие права INSERT у роли
--      приложения - записи создает только функция ниже.

-- Сверка и запись ее результата - одно действие (SECURITY DEFINER, как
-- audit.record()). Роль приложения получает EXECUTE, но не INSERT: «целая»
-- в этой таблице не может появиться иначе, чем от настоящего пересчета.
CREATE FUNCTION audit.run_chain_check(
  OUT checked_at timestamptz,
  OUT intact     boolean,
  OUT entries    bigint,
  OUT broken_at  bigint
)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, audit, core AS $$
DECLARE
  report record;
  -- Результат читается через алиас и переносится в OUT-параметры присваиванием:
  -- в RETURNING имена столбцов совпали бы с именами OUT-параметров, и plpgsql
  -- назвал бы такую ссылку неоднозначной
  saved  audit.chain_checks;
BEGIN
  SELECT * INTO report FROM audit.verify_chain_report();

  INSERT INTO audit.chain_checks AS c (intact, entries, broken_at)
  VALUES (report.intact, report.entries, report.broken_at)
  RETURNING c.* INTO saved;

  checked_at := saved.checked_at;
  intact     := saved.intact;
  entries    := saved.entries;
  broken_at  := saved.broken_at;
END $$;

GRANT SELECT ON audit.chain_checks TO tou_rent_app;
GRANT EXECUTE ON FUNCTION audit.run_chain_check() TO tou_rent_app;
GRANT EXECUTE ON FUNCTION audit.verify_chain_report() TO tou_rent_app;
