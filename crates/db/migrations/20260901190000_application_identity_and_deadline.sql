-- Личность заявки неизменна, прием закрывается дедлайном (INV-037, FR-401).
--
-- Круг 2 гаунтлета, пробы в одноразовой БД:
--
-- 1. У поданной заявки подменяется заявитель:
--      UPDATE core.applications SET participant_id = <другой> WHERE id = ...;
--      -> UPDATE 1
--    Запись журнала регистрации при этом append-only и продолжает
--    утверждать «заявка X подана участником Y в момент Z», а цепочка
--    аудита остается целой. То есть журнал после подмены врет, и по нему
--    этого не видно. У `core.applications` не было ни одного сторожа на
--    UPDATE.
--
-- 2. Заявка вставляется после истечения срока приема:
--      UPDATE core.tenders SET submission_deadline = core.now() - '10 days';
--      INSERT INTO core.applications (...) -> INSERT 0 1
--    При этом на журнал тот же дедлайн действует
--    (`core.journal_before_insert`, INV-037). Инвариант держался только
--    тем, что код всегда пишет обе строки в одной транзакции - то есть
--    жил в приложении, а не в схеме.
--
-- 3. `ON DELETE CASCADE` от заявки уносил запечатанное ценовое
--    предложение и вложения:
--      DELETE FROM core.applications WHERE id = ...;  -> DELETE 1
--      SELECT count(*) FROM core.price_proposals ...  -> 0
--    Ценовое предложение защищено RLS и шифрованием (INV-040), но
--    удалялось молча вместе с родителем, минуя этот запрет.

-- --- 1. Личность заявки -----------------------------------------------------
CREATE FUNCTION core.freeze_application_identity() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.tender_id IS DISTINCT FROM OLD.tender_id
     OR NEW.lot_id IS DISTINCT FROM OLD.lot_id
     OR NEW.participant_id IS DISTINCT FROM OLD.participant_id
     OR NEW.applicant_kind IS DISTINCT FROM OLD.applicant_kind
     OR NEW.submitted_at IS DISTINCT FROM OLD.submitted_at THEN
    RAISE EXCEPTION
      'INV-037: заявитель, лот, тендер и момент подачи не переписываются - запись журнала регистрации ссылается на них (п. 37-39)'
      USING ERRCODE = 'check_violation';
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER freeze_application_identity BEFORE UPDATE ON core.applications
  FOR EACH ROW EXECUTE FUNCTION core.freeze_application_identity();
ALTER TABLE core.applications ENABLE ALWAYS TRIGGER freeze_application_identity;

-- --- 2. Прием закрыт - значит закрыт ----------------------------------------
CREATE FUNCTION core.check_application_deadline() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  deadline timestamptz;
BEGIN
  SELECT submission_deadline INTO deadline FROM core.tenders WHERE id = NEW.tender_id;

  IF deadline IS NOT NULL AND core.now() > deadline THEN
    RAISE EXCEPTION 'INV-037: прием закрыт - дедлайн % истек (п. 37-39)', deadline
      USING ERRCODE = 'check_violation';
  END IF;

  RETURN NEW;
END $$;

CREATE TRIGGER check_application_deadline BEFORE INSERT ON core.applications
  FOR EACH ROW EXECUTE FUNCTION core.check_application_deadline();

-- --- 3. Запечатанная цена и вложения не уносятся каскадом -------------------
ALTER TABLE core.price_proposals
  DROP CONSTRAINT price_proposals_application_id_fkey,
  ADD  CONSTRAINT price_proposals_application_id_fkey
    FOREIGN KEY (application_id) REFERENCES core.applications (id) ON DELETE RESTRICT;

ALTER TABLE core.application_files
  DROP CONSTRAINT application_files_application_id_fkey,
  ADD  CONSTRAINT application_files_application_id_fkey
    FOREIGN KEY (application_id) REFERENCES core.applications (id) ON DELETE RESTRICT;
