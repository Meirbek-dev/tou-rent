-- Двигатель обязательств (М17, FR-1702): сроки процесса как данные.
-- Таблица создана в миграции ledger_obligations; здесь она получает
-- идемпотентность, аудит и связь с уведомлением об эскалации.

-- Одно обязательство на «действие + предмет»: повторное событие процесса
-- (перегенерация протокола, повторная рассылка) не плодит дубли сроков.
-- NULLS NOT DISTINCT: у обязательства заполнен ровно один из предметов.
ALTER TABLE core.obligations
  ADD CONSTRAINT obligations_unique_per_subject
  UNIQUE NULLS NOT DISTINCT (action, tender_id, contract_id, application_id);

-- Момент, от которого отсчитан срок (событие Правил): видно, почему due_at
-- именно такой, и можно пересчитать при изменении календаря
ALTER TABLE core.obligations
  ADD COLUMN started_at timestamptz NOT NULL DEFAULT now();

COMMENT ON COLUMN core.obligations.started_at IS
  'Событие, от которого отсчитан срок (п. 54, 57, 73, 75) — основание due_at';

-- Просрочка уведомляется один раз: повторные проходы воркера молчат
ALTER TABLE core.obligations
  ADD COLUMN escalated_at timestamptz;

-- Мутация домена — в аудит (регламент А.5, перечень INV-AUDIT)
CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.obligations
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Дашборд «мои сроки» (FR-1702) выбирает по роли и близости срока
CREATE INDEX obligations_assignee_idx
  ON core.obligations (assignee_role, status, due_at);
