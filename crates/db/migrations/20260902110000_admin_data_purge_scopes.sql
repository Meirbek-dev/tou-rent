-- Очистка данных по видам и по объектам (М15, FR-1503; аудит - FR-1601).
--
-- Первая версия (20260902100000) умела два режима: все процедуры разом и
-- перечисленные тендеры. Ввод стенда в работу оказался поштучным делом:
-- один демо-объект удалить, другой оставить, стереть только уведомления и
-- не трогать тендеры. Поэтому у функции появляется область - вид данных,
-- с которого начинается удаление, - и необязательный перечень записей
-- этого вида. Все, что держится на удаляемых записях, уходит с ними по тем
-- же внешним ключам, что и раньше: у объекта - тендеры, в которых он
-- выставлен лотом, участки и заявки особого порядка по нему; у тендера -
-- заявки, протоколы, торги, договоры; у участка - заявки на него, решения
-- и договор; у заявки особого порядка - заключения, решение, досье и
-- инвестиционный договор.
--
-- Сигнатура меняется, поэтому прежняя функция снимается: одноименная
-- перегрузка с другим набором аргументов оставила бы два входа в одну
-- операцию. Рубежи, след в аудите и порядок обхода таблиц - те же.
--
-- Области: everything, tenders, objects, special_requests, land_plots,
-- notifications. `ids IS NULL` - все записи области.

DROP FUNCTION core.purge_data(uuid[]);

CREATE FUNCTION core.purge_data(scope text, ids uuid[]) RETURNS jsonb
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
  everything boolean := scope = 'everything';
  -- Уведомления не привязаны к процедуре внешним ключом: полная очистка и
  -- область notifications стирают их все, область tenders - только те,
  -- чей payload называет удаляемый тендер
  all_notifications boolean := scope IN ('everything', 'notifications');
  -- Наборы идентификаторов: заполняются до удаления, пока строки на месте
  o   uuid[];  -- объекты
  t   uuid[];  -- тендеры
  l   uuid[];  -- лоты
  a   uuid[];  -- заявки
  s   uuid[];  -- заявки особого порядка
  lp  uuid[];  -- земельные участки
  la  uuid[];  -- заявки на участки
  c   uuid[];  -- договоры
  au  uuid[];  -- торги
  m   uuid[];  -- заседания
  acc uuid[];  -- лицевые счета
  ia  uuid[];  -- приемки инвестиционных договоров
  -- Порядок удаления: от листьев графа внешних ключей к корням. Условие
  -- каждой таблицы описывает ее связь с наборами выше.
  -- $1 everything, $2 t, $3 l, $4 a, $5 c, $6 s, $7 au, $8 m, $9 acc,
  -- $10 ia, $11 o, $12 lp, $13 la, $14 all_notifications
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
    ['land_contracts',            'contract_id = ANY($5) OR land_application_id = ANY($13)'],
    ['evasions',                  'tender_id = ANY($2) OR contract_id = ANY($5) OR application_id = ANY($4)'],
    ['ledger_entries',            'account_id = ANY($9)'],
    ['ledger_accounts',           'id = ANY($9)'],
    ['obligations',               'tender_id = ANY($2) OR contract_id = ANY($5) OR application_id = ANY($4) OR special_request_id = ANY($6)'],
    ['contracts',                 'id = ANY($5)'],
    ['land_decisions',            'land_application_id = ANY($13)'],
    ['land_applications',         'id = ANY($13)'],
    ['land_plots',                'id = ANY($12)'],
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
    ['objects',                   'id = ANY($11)'],
    ['notifications',             '$14 OR (payload ->> ''tender_id'') IN (SELECT x::text FROM unnest($2) AS x)']
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
  IF scope NOT IN ('everything', 'tenders', 'objects', 'special_requests',
                   'land_plots', 'notifications') THEN
    RAISE EXCEPTION 'очистка данных: неизвестная область «%»', scope
      USING ERRCODE = 'invalid_parameter_value';
  END IF;

  -- Корни: с чего начинается удаление в выбранной области
  SELECT coalesce(array_agg(id), '{}') INTO o
    FROM core.objects
    WHERE everything OR (scope = 'objects' AND (ids IS NULL OR id = ANY(ids)));
  -- Объект уходит вместе с тендерами, где он выставлен лотом
  SELECT coalesce(array_agg(id), '{}') INTO t
    FROM core.tenders
    WHERE everything OR (scope = 'tenders' AND (ids IS NULL OR id = ANY(ids)))
       OR id IN (SELECT lo.tender_id FROM core.lots lo WHERE lo.object_id = ANY(o));
  SELECT coalesce(array_agg(id), '{}') INTO l
    FROM core.lots WHERE everything OR tender_id = ANY(t);
  SELECT coalesce(array_agg(id), '{}') INTO a
    FROM core.applications WHERE everything OR tender_id = ANY(t);
  SELECT coalesce(array_agg(id), '{}') INTO s
    FROM core.special_requests
    WHERE everything OR (scope = 'special_requests' AND (ids IS NULL OR id = ANY(ids)))
       OR tender_id = ANY(t) OR object_id = ANY(o);
  SELECT coalesce(array_agg(id), '{}') INTO lp
    FROM core.land_plots
    WHERE everything OR (scope = 'land_plots' AND (ids IS NULL OR id = ANY(ids)))
       OR object_id = ANY(o);
  SELECT coalesce(array_agg(id), '{}') INTO la
    FROM core.land_applications WHERE everything OR plot_id = ANY(lp);
  -- Договоры: по тендеру, по объекту, инвестиционные по заявке особого
  -- порядка и договоры участков по заявке на участок
  SELECT coalesce(array_agg(id), '{}') INTO c
    FROM core.contracts
    WHERE everything OR tender_id = ANY(t) OR object_id = ANY(o)
       OR id IN (SELECT ic.contract_id FROM core.investment_contracts ic
                 WHERE ic.special_request_id = ANY(s))
       OR id IN (SELECT lc.contract_id FROM core.land_contracts lc
                 WHERE lc.land_application_id = ANY(la));
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
      USING everything, t, l, a, c, s, au, m, acc, ia, o, lp, la, all_notifications;
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
                'scope',      scope,
                'ids',        to_jsonb(ids),
                'tender_ids', to_jsonb(t),
                'object_ids', to_jsonb(o),
                'deleted',    counts),
    'new',    NULL
  );
  INSERT INTO audit.log (actor_id, table_name, action, row_id, payload, prev_hash, row_hash)
  VALUES (actor, 'core.data_purge', 'DELETE', NULL, body, prev,
          sha256(coalesce(prev, ''::bytea) || convert_to(body::text, 'UTF8')));

  RETURN counts;
END $$;

COMMENT ON FUNCTION core.purge_data(text, uuid[]) IS
  'Очистка данных стенда администратором: область (everything | tenders | objects | special_requests | land_plots | notifications) и необязательный перечень записей области. Сторожа append-only снимаются на транзакцию, аудит остается';

REVOKE ALL ON FUNCTION core.purge_data(text, uuid[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION core.purge_data(text, uuid[]) TO tou_rent_app;
