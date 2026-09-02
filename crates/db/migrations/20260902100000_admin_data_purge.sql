-- Очистка данных стенда администратором (М15, FR-1503; аудит - FR-1601).
--
-- Стенд, наполненный `api seed` под демонстрацию (Прил. Б), становится
-- рабочим: демо-объекты, тендеры во всех статусах, заявки трех
-- демо-участников, протоколы и торги должны исчезнуть до того, как сюда
-- придут настоящие процедуры. Штатных средств для этого нет и быть не
-- должно: объявленный тендер отменяется, а не удаляется (FR-305), протокол
-- и материалы досье append-only (FR-702, INV-042), журнал и книга проводок
-- тоже (INV-037, INV-DB-05). Роль приложения права DELETE на эти таблицы
-- лишена, а сторожа `core.forbid_mutation` стоят в режиме ALWAYS и отбивают
-- даже владельца.
--
-- Поэтому очистка идет не мимо рубежей, а через один явный проход под
-- владельцем схемы (SECURITY DEFINER), устроенный по образцу сдвига часов
-- (ADR-0005): право - только у роли приложения и только через эту функцию;
-- намерение - переменная `ALLOW_DATA_PURGE` на стороне api, без нее маршрут
-- отказывает; след - каждая удаленная строка аудируемой таблицы уходит в
-- `audit.log` штатным триггером (сторожа `forbid_mutation` отключаются на
-- время транзакции, аудит - нет), а сверху ложится сводное событие
-- `core.data_purge` с числом удаленных строк по таблицам.
--
-- Что не трогается: учетные записи и роли (к ним привязаны сессии, а
-- удаления пользователя в системе нет и не будет), состав комиссии,
-- объявление на главной, справочники и сам журнал аудита (INV-A01).
-- Файлы в бакете `dossiers` остаются: он под Object Lock (INV-042), и права
-- удалять там нет ни у кого; без строки метаданных объект недостижим (A-095).
--
-- `tender_ids IS NULL` - стереть все процедуры и объекты; массив - только
-- перечисленные тендеры со всем, что на них висит (объекты остаются).

CREATE FUNCTION core.purge_data(tender_ids uuid[]) RETURNS jsonb
LANGUAGE plpgsql SECURITY DEFINER
SET search_path = pg_catalog, core, audit
-- Владелец схемы - superuser стенда, RLS его не касается. Если владельцем
-- станет обычная роль, FORCE ROW LEVEL SECURITY на ценах (INV-040) молча
-- отфильтровал бы строки, и удаление заявок упало бы на RESTRICT: с
-- `row_security = off` такой запрос отказывает вслух, а не втихую
SET row_security = off
AS $$
DECLARE
  actor      uuid    := core.current_app_user();
  everything boolean := tender_ids IS NULL;
  -- Наборы идентификаторов: заполняются до удаления, пока строки на месте
  t   uuid[];  -- тендеры
  l   uuid[];  -- лоты
  a   uuid[];  -- заявки
  c   uuid[];  -- договоры
  s   uuid[];  -- заявки особого порядка
  au  uuid[];  -- торги
  m   uuid[];  -- заседания
  acc uuid[];  -- лицевые счета
  ia  uuid[];  -- приемки инвестиционных договоров
  -- Порядок удаления: от листьев графа внешних ключей к корням. Условие
  -- каждой таблицы описывает ее связь с наборами выше; `false` - таблица
  -- не привязана к тендеру и стирается только при полной очистке.
  -- $1 - everything, $2 - t, $3 - l, $4 - a, $5 - c, $6 - s, $7 - au,
  -- $8 - m, $9 - acc, $10 - ia
  steps constant text[][] := ARRAY[
    ['public_records',            'contract_id = ANY($5) OR acceptance_id = ANY($10) OR special_request_id = ANY($6)'],
    ['investment_acceptances',    'contract_id = ANY($5)'],
    ['investment_contract_files', 'contract_id = ANY($5)'],
    ['investment_contracts',      'contract_id = ANY($5) OR special_request_id = ANY($6)'],
    ['benefit_grants',            'contract_id = ANY($5)'],
    ['contract_checklists',       'contract_id = ANY($5)'],
    ['contract_amendment_changes','amendment_id IN (SELECT id FROM core.contract_amendments WHERE contract_id = ANY($5))'],
    ['contract_amendments',       'contract_id = ANY($5)'],
    ['acts',                      'contract_id = ANY($5)'],
    ['land_contract_covenants',   'contract_id = ANY($5)'],
    ['land_contracts',            'contract_id = ANY($5)'],
    ['evasions',                  'tender_id = ANY($2) OR contract_id = ANY($5) OR application_id = ANY($4)'],
    ['ledger_entries',            'account_id = ANY($9)'],
    ['ledger_accounts',           'id = ANY($9)'],
    ['obligations',               'tender_id = ANY($2) OR contract_id = ANY($5) OR application_id = ANY($4) OR special_request_id = ANY($6)'],
    ['contracts',                 'id = ANY($5)'],
    ['land_decisions',            'false'],
    ['land_applications',         'false'],
    ['land_plots',                'false'],
    ['special_reviews',           'special_request_id = ANY($6)'],
    ['special_board_decisions',   'special_request_id = ANY($6)'],
    ['special_request_files',     'special_request_id = ANY($6)'],
    ['dossier_items',             'tender_id = ANY($2) OR special_request_id = ANY($6)'],
    ['special_requests',          'id = ANY($6)'],
    ['bids',                      'auction_id = ANY($7)'],
    ['auction_participants',      'auction_id = ANY($7)'],
    ['auctions',                  'id = ANY($7)'],
    ['votes',                     'application_id = ANY($4) OR meeting_id = ANY($8)'],
    ['meeting_attendance',        'meeting_id = ANY($8)'],
    ['member_recusals',           'tender_id = ANY($2)'],
    ['coi_declarations',          'tender_id = ANY($2)'],
    ['protocols',                 'tender_id = ANY($2)'],
    ['sessions_meetings',         'id = ANY($8)'],
    ['journal_entries',           'tender_id = ANY($2)'],
    ['journal_counters',          'tender_id = ANY($2)'],
    ['application_files',         'application_id = ANY($4)'],
    ['price_proposals',           'application_id = ANY($4)'],
    ['applications',              'id = ANY($4)'],
    ['tender_amendments',         'tender_id = ANY($2)'],
    ['tender_docs',               'tender_id = ANY($2)'],
    ['lots',                      'id = ANY($3)'],
    ['tenders',                   'id = ANY($2)'],
    ['objects',                   'false'],
    ['notifications',             '(payload ->> ''tender_id'') IN (SELECT x::text FROM unnest($2) AS x)']
  ];
  tables  text[];
  guard   record;
  -- Снятые сторожа: таблица, триггер и прежний режим (tgenabled) - три
  -- параллельных массива, потому что переменной типа record[] в plpgsql нет
  guard_tables   text[] := '{}';
  guard_triggers text[] := '{}';
  guard_modes    text[] := '{}';
  i       int;
  removed bigint;
  counts  jsonb := '{}'::jsonb;
  prev    bytea;
  body    jsonb;
BEGIN
  -- Актор обязателен: событие без автора в аудите очистки недопустимо
  IF actor IS NULL THEN
    RAISE EXCEPTION 'очистка данных без актора (app.user_id) запрещена (FR-1601)'
      USING ERRCODE = 'insufficient_privilege';
  END IF;

  SELECT coalesce(array_agg(id), '{}') INTO t
    FROM core.tenders WHERE everything OR id = ANY(tender_ids);
  SELECT coalesce(array_agg(id), '{}') INTO l
    FROM core.lots WHERE everything OR tender_id = ANY(t);
  SELECT coalesce(array_agg(id), '{}') INTO a
    FROM core.applications WHERE everything OR tender_id = ANY(t);
  SELECT coalesce(array_agg(id), '{}') INTO c
    FROM core.contracts WHERE everything OR tender_id = ANY(t);
  SELECT coalesce(array_agg(id), '{}') INTO s
    FROM core.special_requests WHERE everything OR tender_id = ANY(t);
  SELECT coalesce(array_agg(id), '{}') INTO au
    FROM core.auctions WHERE everything OR lot_id = ANY(l);
  SELECT coalesce(array_agg(id), '{}') INTO m
    FROM core.sessions_meetings WHERE everything OR tender_id = ANY(t);
  SELECT coalesce(array_agg(id), '{}') INTO acc
    FROM core.ledger_accounts
    WHERE everything OR application_id = ANY(a) OR contract_id = ANY(c);
  SELECT coalesce(array_agg(id), '{}') INTO ia
    FROM core.investment_acceptances WHERE everything OR contract_id = ANY(c);

  SELECT array_agg(steps[n][1]) INTO tables
    FROM generate_subscripts(steps, 1) AS n;

  -- Сторожа append-only снимаются только с таблиц этого перечня и только
  -- на транзакцию: режим запоминается и возвращается тем же, каким был.
  -- Аудит-триггеры остаются - каждая удаленная строка попадает в журнал.
  FOR guard IN
    SELECT cl.relname AS table_name, tg.tgname AS trigger_name, tg.tgenabled AS mode
    FROM pg_trigger tg
    JOIN pg_class cl     ON cl.oid = tg.tgrelid
    JOIN pg_namespace ns ON ns.oid = cl.relnamespace
    JOIN pg_proc pr      ON pr.oid = tg.tgfoid
    WHERE ns.nspname = 'core'
      AND NOT tg.tgisinternal
      AND pr.proname = 'forbid_mutation'
      AND (tg.tgtype & 8) = 8          -- BEFORE DELETE
      AND cl.relname = ANY(tables)
    ORDER BY 1, 2
  LOOP
    EXECUTE format('ALTER TABLE core.%I DISABLE TRIGGER %I',
                   guard.table_name, guard.trigger_name);
    guard_tables   := guard_tables   || guard.table_name::text;
    guard_triggers := guard_triggers || guard.trigger_name::text;
    guard_modes    := guard_modes    || guard.mode::text;
  END LOOP;

  -- Повторный тендер вне перечня не должен указывать на удаленный (п. 82)
  IF NOT everything THEN
    UPDATE core.tenders SET repeat_of = NULL
    WHERE repeat_of = ANY(t) AND NOT (id = ANY(t));
  END IF;

  FOR i IN 1 .. array_length(steps, 1) LOOP
    EXECUTE format('DELETE FROM core.%I WHERE $1 OR (%s)', steps[i][1], steps[i][2])
      USING everything, t, l, a, c, s, au, m, acc, ia;
    GET DIAGNOSTICS removed = ROW_COUNT;
    IF removed > 0 THEN
      counts := counts || jsonb_build_object(steps[i][1], removed);
    END IF;
  END LOOP;

  FOR i IN 1 .. coalesce(array_length(guard_tables, 1), 0) LOOP
    EXECUTE format('ALTER TABLE core.%I ENABLE %s TRIGGER %I',
                   guard_tables[i],
                   CASE guard_modes[i] WHEN 'A' THEN 'ALWAYS' WHEN 'R' THEN 'REPLICA' ELSE '' END,
                   guard_triggers[i]);
  END LOOP;

  -- Сводное событие в той же hash-цепочке, что и построчные (INV-A01):
  -- тот же замок и та же формула, что в audit.record()
  PERFORM pg_advisory_xact_lock(hashtext('audit.log'));
  SELECT lg.row_hash INTO prev FROM audit.log lg ORDER BY lg.id DESC LIMIT 1;
  body := jsonb_build_object(
    'table',  'core.data_purge',
    'action', 'DELETE',
    'old',    jsonb_build_object(
                'scope',      CASE WHEN everything THEN 'everything' ELSE 'tenders' END,
                'tender_ids', to_jsonb(t),
                'deleted',    counts),
    'new',    NULL
  );
  INSERT INTO audit.log (actor_id, table_name, action, row_id, payload, prev_hash, row_hash)
  VALUES (actor, 'core.data_purge', 'DELETE', NULL, body, prev,
          sha256(coalesce(prev, ''::bytea) || convert_to(body::text, 'UTF8')));

  RETURN counts;
END $$;

COMMENT ON FUNCTION core.purge_data(uuid[]) IS
  'Очистка данных стенда администратором: NULL - все процедуры и объекты, массив - перечисленные тендеры. Сторожа append-only снимаются на транзакцию, аудит остается';

REVOKE ALL ON FUNCTION core.purge_data(uuid[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION core.purge_data(uuid[]) TO tou_rent_app;
