-- Аудит жизненного цикла учетной записи (W-07, FR-1503, регламент А.5).
--
-- До сих пор под аудитом были роли (`core.role_grants`), а сама учетная
-- запись — нет, и это сходило с рук ровно потому, что менять в ней через API
-- было нечего: пароль не сбрасывался, `is_active` не переключался. Теперь
-- и то и другое делает админ, и оба действия обязаны оставлять доказательство
-- «кто, кого и когда» — иначе отключение сотрудника и сброс его пароля
-- существуют только как факт в самой строке, без следа о том, чьё это решение.
--
-- Генерический `audit.record()` к `core.users` неприменим по одной причине:
-- он кладёт в payload `to_jsonb(NEW)` целиком, то есть и `password_hash`.
-- Журнал append-only и не чистится никем, а читать его может вся роль
-- приложения — Argon2id-строка осела бы там навсегда и в общем доступе.
-- Поэтому хеш из payload вырезается, а на его место кладётся короткий
-- отпечаток sha256 от самой PHC-строки: подобрать по нему пароль нельзя
-- (соль внутри строки случайна и в отпечаток входит), но меняется он вместе
-- с паролем — и по журналу видно, что сменился именно пароль, а не только
-- `updated_at`.
--
-- email и ФИО остаются в payload как есть: в аудите фиксация полная (NFR-07),
-- а опознать субъекта события по одному uuid при разбирательстве нельзя —
-- строка к тому моменту может быть уже другой или удалённой.

-- Отпечаток вместо секрета. STRICT: на INSERT `OLD` (и на DELETE `NEW`) —
-- NULL, и обёртка обязана вернуть тот же NULL, что и прежний `to_jsonb`.
CREATE FUNCTION audit.redact_secrets(row_json jsonb) RETURNS jsonb
LANGUAGE sql IMMUTABLE STRICT SET search_path = pg_catalog AS $$
  SELECT jsonb_set(
    row_json - 'password_hash',
    '{password_fingerprint}',
    CASE
      WHEN row_json ->> 'password_hash' IS NULL THEN 'null'::jsonb
      ELSE to_jsonb(
        left(encode(sha256(convert_to(row_json ->> 'password_hash', 'UTF8')), 'hex'), 16))
    END,
    true)
$$;

-- Копия `audit.record()` с одной правкой — payload проходит через редакцию.
-- Тело повторено целиком осознанно: подменить `audit.record()` на версию
-- с редакцией нельзя (она применяется ко всем таблицам перечня и молча
-- переименовала бы им поля), а вызвать её из общей функции — значит завести
-- в горячем триггере ветвление по имени таблицы.
CREATE FUNCTION audit.record_user() RETURNS trigger
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
    'old',    audit.redact_secrets(to_jsonb(OLD)),
    'new',    audit.redact_secrets(to_jsonb(NEW))
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

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.users
  FOR EACH ROW EXECUTE FUNCTION audit.record_user();
