-- Аудит (М16, FR-1601): append-only журнал с hash-цепочкой (INV-A01).
-- row_hash = sha256(prev_hash || payload::text); канонический текст jsonb
-- детерминирован (ключи отсортированы), цепочка проверяема audit.verify_chain().

CREATE TABLE audit.log (
  id          bigint      GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  occurred_at timestamptz NOT NULL DEFAULT now(),
  actor_id    uuid,                 -- core.current_app_user(); NULL — системная операция
  table_name  text        NOT NULL, -- schema.table
  action      text        NOT NULL CHECK (action IN ('INSERT', 'UPDATE', 'DELETE')),
  row_id      uuid,
  payload     jsonb       NOT NULL, -- {'old': ..., 'new': ...} (NFR-07: в аудите — полная фиксация)
  prev_hash   bytea,                -- NULL только у первой записи
  row_hash    bytea       NOT NULL
);

CREATE INDEX log_table_row_idx ON audit.log (table_name, row_id);

-- Генерический триггер для таблиц перечня INV-AUDIT. SECURITY DEFINER:
-- роль приложения не имеет прав на audit.log — подделать событие вне триггера нельзя.
CREATE FUNCTION audit.record() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, audit, core AS $$
DECLARE
  prev  bytea;
  body  jsonb;
BEGIN
  -- Сериализация хвоста цепочки: конкурентные записи не порвут hash-связность
  PERFORM pg_advisory_xact_lock(hashtext('audit.log'));
  SELECT l.row_hash INTO prev FROM audit.log l ORDER BY l.id DESC LIMIT 1;

  body := jsonb_build_object(
    'table',  TG_TABLE_SCHEMA || '.' || TG_TABLE_NAME,
    'action', TG_OP,
    'old',    to_jsonb(OLD),
    'new',    to_jsonb(NEW)
  );

  INSERT INTO audit.log (actor_id, table_name, action, row_id, payload, prev_hash, row_hash)
  VALUES (
    core.current_app_user(),
    TG_TABLE_SCHEMA || '.' || TG_TABLE_NAME,
    TG_OP,
    CASE TG_OP WHEN 'DELETE' THEN OLD.id ELSE NEW.id END,
    body,
    prev,
    sha256(coalesce(prev, ''::bytea) || convert_to(body::text, 'UTF8'))
  );

  RETURN NULL;  -- AFTER-триггер
END $$;

-- INV-A01: журнал append-only и для владельца БД
CREATE TRIGGER log_append_only BEFORE UPDATE OR DELETE ON audit.log
  FOR EACH ROW EXECUTE FUNCTION core.forbid_mutation('INV-A01');

-- Проверка непрерывности цепочки (гейт G15, лента аудита в демо § 9.1)
CREATE FUNCTION audit.verify_chain() RETURNS boolean
LANGUAGE plpgsql STABLE AS $$
DECLARE
  rec  record;
  prev bytea := NULL;
BEGIN
  FOR rec IN SELECT * FROM audit.log ORDER BY id LOOP
    IF rec.prev_hash IS DISTINCT FROM prev THEN
      RETURN false;
    END IF;
    IF rec.row_hash <> sha256(coalesce(prev, ''::bytea) || convert_to(rec.payload::text, 'UTF8')) THEN
      RETURN false;
    END IF;
    prev := rec.row_hash;
  END LOOP;
  RETURN true;
END $$;

-- Триггеры на каждой таблице перечня INV-AUDIT (FR-1601); полноту проверяет тест G15
CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.tenders
  FOR EACH ROW EXECUTE FUNCTION audit.record();
CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.lots
  FOR EACH ROW EXECUTE FUNCTION audit.record();
CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.applications
  FOR EACH ROW EXECUTE FUNCTION audit.record();
CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.journal_entries
  FOR EACH ROW EXECUTE FUNCTION audit.record();
CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.bids
  FOR EACH ROW EXECUTE FUNCTION audit.record();
CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.protocols
  FOR EACH ROW EXECUTE FUNCTION audit.record();
CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.contracts
  FOR EACH ROW EXECUTE FUNCTION audit.record();
CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.ledger_entries
  FOR EACH ROW EXECUTE FUNCTION audit.record();
CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.role_grants
  FOR EACH ROW EXECUTE FUNCTION audit.record();
CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.notifications
  FOR EACH ROW EXECUTE FUNCTION audit.record();
