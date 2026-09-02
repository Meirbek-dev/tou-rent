-- Точечная очистка любых записей (М15, FR-1503; аудит - FR-1601).
--
-- Вторая версия (20260902110000) знала пять областей - корни графа процедур.
-- Ввод стенда в работу потребовал удалять что угодно по одной записи:
-- заявку, протокол, проводку, уведомление. Поэтому областью становится
-- каждый вид данных, который кабинет показывает на вкладке «Данные», а
-- перечень `ids` по-прежнему необязателен: без него уходят все записи вида.
--
-- Правило то же: удаляемая запись уносит все, что на ней держится по
-- внешним ключам. Заявка - свои файлы, цену, журнал, голоса, участие в
-- торгах, торги, где она победитель или очередной ход, договор по ней,
-- счет и проводки; лот - заявки, торги и договоры по нему; протокол -
-- договоры по нему; договор - акты, сверку, допсоглашения, льготу,
-- инвестиционный и земельный договоры, счет и проводки. Листья графа
-- (акты, проводки, материалы досье, публикации, обязательства,
-- уведомления) уходят сами по себе.
--
-- Сигнатура прежняя, тело заменяется целиком. Сторожа, след в аудите и
-- порядок обхода таблиц не меняются.

CREATE OR REPLACE FUNCTION core.purge_data(scope text, ids uuid[]) RETURNS jsonb
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
  -- Наборы идентификаторов: заполняются до удаления, пока строки на месте,
  -- от корней графа к листьям - каждый набор знает своих родителей
  o   uuid[];  -- объекты
  t   uuid[];  -- тендеры
  l   uuid[];  -- лоты
  a   uuid[];  -- заявки
  s   uuid[];  -- заявки особого порядка
  lp  uuid[];  -- земельные участки
  la  uuid[];  -- заявки на участки
  p   uuid[];  -- протоколы
  au  uuid[];  -- торги
  c   uuid[];  -- договоры
  ic  uuid[];  -- инвестиционные договоры
  m   uuid[];  -- заседания
  acc uuid[];  -- лицевые счета
  le  uuid[];  -- проводки
  ia  uuid[];  -- приемки инвестиционных договоров
  ac  uuid[];  -- акты
  d   uuid[];  -- материалы досье
  pr  uuid[];  -- публикации решений
  ob  uuid[];  -- обязательства
  n   uuid[];  -- уведомления
  -- Порядок удаления: от листьев графа внешних ключей к корням. Условие
  -- каждой таблицы - ее связь с наборами выше.
  -- $1 t, $2 l, $3 a, $4 c, $5 s, $6 au, $7 m, $8 acc, $9 ia, $10 o,
  -- $11 lp, $12 la, $13 p, $14 ic, $15 le, $16 ac, $17 d, $18 pr,
  -- $19 ob, $20 n
  steps constant text[][] := ARRAY[
    ['public_records',            'id = ANY($18)'],
    ['investment_acceptances',    'id = ANY($9)'],
    ['investment_contract_files', 'contract_id = ANY($4)'],
    ['investment_contracts',      'id = ANY($14)'],
    ['benefit_grants',            'contract_id = ANY($4)'],
    ['contract_checklists',       'contract_id = ANY($4)'],
    ['contract_amendment_changes','amendment_id IN (SELECT id FROM core.contract_amendments WHERE contract_id = ANY($4))'],
    ['contract_amendments',       'contract_id = ANY($4)'],
    ['acts',                      'id = ANY($16)'],
    ['land_contract_covenants',   'contract_id = ANY($4)'],
    ['land_contracts',            'contract_id = ANY($4) OR land_application_id = ANY($12)'],
    ['evasions',                  'tender_id = ANY($1) OR lot_id = ANY($2) OR application_id = ANY($3) OR contract_id = ANY($4)'],
    ['ledger_entries',            'id = ANY($15)'],
    ['ledger_accounts',           'id = ANY($8)'],
    ['obligations',               'id = ANY($19)'],
    ['contracts',                 'id = ANY($4)'],
    ['land_decisions',            'land_application_id = ANY($12)'],
    ['land_applications',         'id = ANY($12)'],
    ['land_plots',                'id = ANY($11)'],
    ['special_reviews',           'special_request_id = ANY($5)'],
    ['special_board_decisions',   'special_request_id = ANY($5)'],
    ['special_request_files',     'special_request_id = ANY($5)'],
    ['dossier_items',             'id = ANY($17)'],
    ['special_requests',          'id = ANY($5)'],
    ['bids',                      'auction_id = ANY($6) OR application_id = ANY($3)'],
    ['auction_participants',      'auction_id = ANY($6) OR application_id = ANY($3)'],
    ['auctions',                  'id = ANY($6)'],
    ['votes',                     'application_id = ANY($3) OR meeting_id = ANY($7)'],
    ['meeting_attendance',        'meeting_id = ANY($7)'],
    ['member_recusals',           'tender_id = ANY($1) OR lot_id = ANY($2)'],
    ['coi_declarations',          'tender_id = ANY($1)'],
    ['protocols',                 'id = ANY($13)'],
    ['sessions_meetings',         'id = ANY($7)'],
    ['journal_entries',           'tender_id = ANY($1) OR application_id = ANY($3)'],
    ['journal_counters',          'tender_id = ANY($1)'],
    ['application_files',         'application_id = ANY($3)'],
    ['price_proposals',           'application_id = ANY($3)'],
    ['applications',              'id = ANY($3)'],
    ['tender_amendments',         'tender_id = ANY($1)'],
    ['tender_docs',               'tender_id = ANY($1)'],
    ['lots',                      'id = ANY($2)'],
    ['tenders',                   'id = ANY($1)'],
    ['objects',                   'id = ANY($10)'],
    ['notifications',             'id = ANY($20)']
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
  IF scope NOT IN ('everything', 'objects', 'tenders', 'lots', 'applications',
                   'protocols', 'auctions', 'contracts', 'acts', 'ledger_entries',
                   'special_requests', 'land_plots', 'investment_contracts',
                   'dossier_items', 'public_records', 'obligations',
                   'notifications') THEN
    RAISE EXCEPTION 'очистка данных: неизвестная область «%»', scope
      USING ERRCODE = 'invalid_parameter_value';
  END IF;

  -- Каждый набор: все записи (полная очистка), записи своей области
  -- (перечень либо все) и записи, которые держатся на уже выбранных
  SELECT coalesce(array_agg(id), '{}') INTO o
    FROM core.objects
    WHERE everything OR (scope = 'objects' AND (ids IS NULL OR id = ANY(ids)));
  SELECT coalesce(array_agg(id), '{}') INTO t
    FROM core.tenders
    WHERE everything OR (scope = 'tenders' AND (ids IS NULL OR id = ANY(ids)))
       OR id IN (SELECT lo.tender_id FROM core.lots lo WHERE lo.object_id = ANY(o));
  SELECT coalesce(array_agg(id), '{}') INTO l
    FROM core.lots
    WHERE everything OR (scope = 'lots' AND (ids IS NULL OR id = ANY(ids)))
       OR tender_id = ANY(t);
  SELECT coalesce(array_agg(id), '{}') INTO a
    FROM core.applications
    WHERE everything OR (scope = 'applications' AND (ids IS NULL OR id = ANY(ids)))
       OR tender_id = ANY(t) OR lot_id = ANY(l);
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
  SELECT coalesce(array_agg(id), '{}') INTO p
    FROM core.protocols
    WHERE everything OR (scope = 'protocols' AND (ids IS NULL OR id = ANY(ids)))
       OR tender_id = ANY(t);
  -- Торги: по лоту и те, где удаляемая заявка - победитель, второе место
  -- или очередной ход (внешние ключи из торгов в заявки)
  SELECT coalesce(array_agg(id), '{}') INTO au
    FROM core.auctions
    WHERE everything OR (scope = 'auctions' AND (ids IS NULL OR id = ANY(ids)))
       OR lot_id = ANY(l)
       OR winner_application_id = ANY(a) OR runner_up_application_id = ANY(a)
       OR current_turn_application_id = ANY(a);
  -- Договоры: по тендеру, лоту, объекту, заявке-победителю, протоколу,
  -- инвестиционные по заявке особого порядка, земельные по заявке на участок
  SELECT coalesce(array_agg(id), '{}') INTO c
    FROM core.contracts
    WHERE everything OR (scope = 'contracts' AND (ids IS NULL OR id = ANY(ids)))
       OR tender_id = ANY(t) OR lot_id = ANY(l) OR object_id = ANY(o)
       OR winner_application_id = ANY(a) OR protocol_id = ANY(p)
       OR id IN (SELECT x.contract_id FROM core.investment_contracts x
                 WHERE x.special_request_id = ANY(s))
       OR id IN (SELECT x.contract_id FROM core.land_contracts x
                 WHERE x.land_application_id = ANY(la));
  SELECT coalesce(array_agg(id), '{}') INTO ic
    FROM core.investment_contracts
    WHERE everything OR (scope = 'investment_contracts' AND (ids IS NULL OR id = ANY(ids)))
       OR contract_id = ANY(c) OR special_request_id = ANY(s);
  SELECT coalesce(array_agg(id), '{}') INTO m
    FROM core.sessions_meetings WHERE everything OR tender_id = ANY(t);
  SELECT coalesce(array_agg(id), '{}') INTO acc
    FROM core.ledger_accounts
    WHERE everything OR application_id = ANY(a) OR contract_id = ANY(c);
  SELECT coalesce(array_agg(id), '{}') INTO le
    FROM core.ledger_entries
    WHERE everything OR (scope = 'ledger_entries' AND (ids IS NULL OR id = ANY(ids)))
       OR account_id = ANY(acc);
  SELECT coalesce(array_agg(id), '{}') INTO ia
    FROM core.investment_acceptances WHERE everything OR contract_id = ANY(c);
  SELECT coalesce(array_agg(id), '{}') INTO ac
    FROM core.acts
    WHERE everything OR (scope = 'acts' AND (ids IS NULL OR id = ANY(ids)))
       OR contract_id = ANY(c);
  SELECT coalesce(array_agg(id), '{}') INTO d
    FROM core.dossier_items
    WHERE everything OR (scope = 'dossier_items' AND (ids IS NULL OR id = ANY(ids)))
       OR tender_id = ANY(t) OR special_request_id = ANY(s);
  SELECT coalesce(array_agg(id), '{}') INTO pr
    FROM core.public_records
    WHERE everything OR (scope = 'public_records' AND (ids IS NULL OR id = ANY(ids)))
       OR contract_id = ANY(c) OR acceptance_id = ANY(ia) OR special_request_id = ANY(s);
  SELECT coalesce(array_agg(id), '{}') INTO ob
    FROM core.obligations
    WHERE everything OR (scope = 'obligations' AND (ids IS NULL OR id = ANY(ids)))
       OR tender_id = ANY(t) OR contract_id = ANY(c) OR application_id = ANY(a)
       OR special_request_id = ANY(s);
  -- Уведомления не привязаны к процедуре внешним ключом: с тендером
  -- уходят те, чей payload его называет
  SELECT coalesce(array_agg(id), '{}') INTO n
    FROM core.notifications
    WHERE everything OR (scope = 'notifications' AND (ids IS NULL OR id = ANY(ids)))
       OR (payload ->> 'tender_id') IN (SELECT x::text FROM unnest(t) AS x);

  SELECT array_agg(steps[k][1]) INTO tables
    FROM generate_subscripts(steps, 1) AS k;

  -- Сторожа append-only снимаются только с таблиц этого перечня и только
  -- на транзакцию: режим запоминается и возвращается тем же, каким был.
  -- Аудит-триггеры остаются - каждая удаленная строка попадает в журнал.
  FOR guard IN
    SELECT cl.relname AS table_name, tg.tgname AS trigger_name, tg.tgenabled AS mode
    FROM pg_trigger tg
    JOIN pg_class cl     ON cl.oid = tg.tgrelid
    JOIN pg_namespace ns ON ns.oid = cl.relnamespace
    JOIN pg_proc pr_     ON pr_.oid = tg.tgfoid
    WHERE ns.nspname = 'core'
      AND NOT tg.tgisinternal
      AND pr_.proname = 'forbid_mutation'
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
    EXECUTE format('DELETE FROM core.%I WHERE %s', steps[i][1], steps[i][2])
      USING t, l, a, c, s, au, m, acc, ia, o, lp, la, p, ic, le, ac, d, pr, ob, n;
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
  'Очистка данных стенда администратором: область - everything либо любой вид данных вкладки «Данные» (objects, tenders, lots, applications, protocols, auctions, contracts, acts, ledger_entries, special_requests, land_plots, investment_contracts, dossier_items, public_records, obligations, notifications) - и необязательный перечень записей области. Сторожа append-only снимаются на транзакцию, аудит остается';
