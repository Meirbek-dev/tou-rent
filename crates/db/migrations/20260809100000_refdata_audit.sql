-- Аудит правок справочников (T53, FR-1901, регламент А.5).
--
-- Справочники не «просто данные»: МРП и коэффициенты Прил. 4 определяют все
-- будущие ставки, а календарь - все процессуальные сроки. Правка справочника
-- админом обязана оставлять след ровно так же, как мутация домена (FR-1601).
--
-- У `refdata.mrp` и `refdata.holidays` естественные ключи (год и дата), а не
-- `id uuid`, поэтому генерический `audit.record()` к ним неприменим: он берет
-- `NEW.id`. Для них - вариант с `row_id = NULL`; ключ виден в payload, который
-- в обоих случаях содержит полные old/new.

CREATE FUNCTION audit.record_natural_key() RETURNS trigger
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, audit, core AS $$
DECLARE
  prev  bytea;
  body  jsonb;
BEGIN
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
    NULL,  -- строка опознается естественным ключом внутри payload
    body,
    prev,
    sha256(coalesce(prev, ''::bytea) || convert_to(body::text, 'UTF8'))
  );

  RETURN NULL;  -- AFTER-триггер
END $$;

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON refdata.mrp
  FOR EACH ROW EXECUTE FUNCTION audit.record_natural_key();

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON refdata.holidays
  FOR EACH ROW EXECUTE FUNCTION audit.record_natural_key();

-- У коэффициентов есть `id uuid` - работает общий триггер, и запись аудита
-- ссылается на конкретную версию множителя
CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON refdata.rate_coefficients
  FOR EACH ROW EXECUTE FUNCTION audit.record();
