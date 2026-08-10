-- Схемы и расширения (арх. § 6). PostgreSQL 18+ (dev — 19beta, ADR-0002).
-- Все ID — uuid v7 (встроенный uuidv7()), деньги — numeric(14,2) KZT, времена — timestamptz (UTC, NFR-03).

CREATE SCHEMA IF NOT EXISTS core;    -- рабочие данные процессов
CREATE SCHEMA IF NOT EXISTS refdata; -- справочники (версионируемые, редактирует admin)
CREATE SCHEMA IF NOT EXISTS audit;   -- append-only журнал с hash-цепочкой (INV-A01)

CREATE EXTENSION IF NOT EXISTS citext;     -- email без учета регистра
CREATE EXTENSION IF NOT EXISTS btree_gist; -- EXCLUDE (object_id WITH =, lease_period WITH &&), INV-DB-02

-- Текущий пользователь приложения: API выставляет `SET LOCAL app.user_id = '<uuid>'`
-- в каждой транзакции; NULL — системные/анонимные операции.
CREATE FUNCTION core.current_app_user() RETURNS uuid
LANGUAGE sql STABLE PARALLEL SAFE
RETURN nullif(current_setting('app.user_id', true), '')::uuid;

-- Единый триггер updated_at для изменяемых таблиц
CREATE FUNCTION core.touch_updated_at() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  NEW.updated_at := now();
  RETURN NEW;
END $$;

-- Универсальный запрет UPDATE/DELETE для append-only таблиц (второй рубеж после REVOKE:
-- срабатывает и для владельца БД, которого privileges не ограничивают)
CREATE FUNCTION core.forbid_mutation() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION '%: таблица % append-only (изменение и удаление запрещены)',
    coalesce(TG_ARGV[0], 'append-only'), TG_TABLE_NAME
    USING ERRCODE = 'insufficient_privilege';
END $$;
