-- Роль приложения (A-011): API подключается под tou_rent_app (SET ROLE после
-- connect) — без superuser и BYPASSRLS, иначе RLS INV-040 не действует.
-- Superuser-подключение используется только для миграций.

DO $$
BEGIN
  CREATE ROLE tou_rent_app NOLOGIN;
EXCEPTION WHEN duplicate_object THEN
  NULL;  -- роль кластерная: уже существует при повторном накате на тот же кластер
END $$;

GRANT tou_rent_app TO current_user;

GRANT USAGE ON SCHEMA core, refdata, audit TO tou_rent_app;

-- refdata: чтение всем; правку справочников (МРП, коэффициенты, календарь) выполняет
-- admin через API. Таблица переходов — только SELECT: INV-021 меняется миграцией.
GRANT SELECT ON ALL TABLES IN SCHEMA refdata TO tou_rent_app;
GRANT INSERT, UPDATE ON refdata.mrp, refdata.rate_coefficients, refdata.holidays TO tou_rent_app;
GRANT DELETE ON refdata.holidays TO tou_rent_app;

-- core: полный DML, затем точечные вычеты append-only (первый рубеж — REVOKE,
-- второй — триггеры forbid_mutation, работающие и для владельца)
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA core TO tou_rent_app;
GRANT USAGE ON ALL SEQUENCES IN SCHEMA core TO tou_rent_app;
REVOKE UPDATE, DELETE ON core.journal_entries FROM tou_rent_app;  -- INV-037
REVOKE UPDATE, DELETE ON core.bids            FROM tou_rent_app;  -- append-only ставок
REVOKE UPDATE, DELETE ON core.ledger_entries  FROM tou_rent_app;  -- INV-DB-05

-- audit: только чтение (лента аудита). INSERT не выдается — записи создает
-- исключительно триггер audit.record() (SECURITY DEFINER): подделка события невозможна.
GRANT SELECT ON audit.log TO tou_rent_app;

-- Будущие таблицы core/refdata из следующих миграций наследуют права;
-- append-only вычеты для них задаются явно в тех же миграциях.
ALTER DEFAULT PRIVILEGES IN SCHEMA core
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO tou_rent_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA core
  GRANT USAGE ON SEQUENCES TO tou_rent_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA refdata
  GRANT SELECT ON TABLES TO tou_rent_app;
