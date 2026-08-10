-- Единые часы для отметок времени (ADR-0005, арх. v3 § 6.2, NFR-03).
--
-- Миграция `20260809120000_controllable_time.sql` объявила `core.now()`
-- единственным источником времени, но перевела на него только правила -
-- триггерные функции. Отметки же ставятся умолчаниями колонок, и все 57
-- остались на `now()`: запись журнала получала сдвинутое время, а
-- `submitted_at` той же заявки, `cast_at` голоса, `generated_at` протокола
-- и `occurred_at` записи аудита - реальное. На проде сдвиг нулевой и разницы
-- нет; в сквозных сценариях с управляемым временем (T68) досье одного
-- тендера получало несогласованную хронологию.
--
-- `refdata.clock_offset.set_at` намеренно остается на `clock_timestamp()`:
-- отметка о самом сдвиге обязана быть реальной, иначе сдвиг прячет себя.
--
-- Операция затрагивает только каталог: значения строк не переписываются.

-- Служебная отметка изменения - тем же временем, что и все остальные:
-- иначе `updated_at` оказывается раньше `created_at` при сдвинутых часах
CREATE OR REPLACE FUNCTION core.touch_updated_at() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  NEW.updated_at := core.now();
  RETURN NEW;
END $$;

ALTER TABLE audit.log ALTER COLUMN occurred_at SET DEFAULT core.now();
ALTER TABLE core.acts ALTER COLUMN created_at SET DEFAULT core.now();
ALTER TABLE core.application_files ALTER COLUMN uploaded_at SET DEFAULT core.now();
ALTER TABLE core.applications ALTER COLUMN submitted_at SET DEFAULT core.now();
ALTER TABLE core.applications ALTER COLUMN updated_at SET DEFAULT core.now();
ALTER TABLE core.auction_participants ALTER COLUMN changed_at SET DEFAULT core.now();
ALTER TABLE core.benefit_grants ALTER COLUMN granted_at SET DEFAULT core.now();
ALTER TABLE core.benefit_grants ALTER COLUMN updated_at SET DEFAULT core.now();
ALTER TABLE core.bids ALTER COLUMN placed_at SET DEFAULT core.now();
ALTER TABLE core.coi_declarations ALTER COLUMN declared_at SET DEFAULT core.now();
ALTER TABLE core.commissions ALTER COLUMN created_at SET DEFAULT core.now();
ALTER TABLE core.contract_amendments ALTER COLUMN created_at SET DEFAULT core.now();
ALTER TABLE core.contracts ALTER COLUMN created_at SET DEFAULT core.now();
ALTER TABLE core.contracts ALTER COLUMN updated_at SET DEFAULT core.now();
ALTER TABLE core.dossier_items ALTER COLUMN created_at SET DEFAULT core.now();
ALTER TABLE core.dossier_items ALTER COLUMN occurred_at SET DEFAULT core.now();
ALTER TABLE core.evasions ALTER COLUMN declared_at SET DEFAULT core.now();
ALTER TABLE core.investment_acceptances ALTER COLUMN created_at SET DEFAULT core.now();
ALTER TABLE core.investment_contract_files ALTER COLUMN uploaded_at SET DEFAULT core.now();
ALTER TABLE core.investment_contracts ALTER COLUMN created_at SET DEFAULT core.now();
ALTER TABLE core.investment_contracts ALTER COLUMN updated_at SET DEFAULT core.now();
ALTER TABLE core.journal_entries ALTER COLUMN occurred_at SET DEFAULT core.now();
ALTER TABLE core.land_applications ALTER COLUMN submitted_at SET DEFAULT core.now();
ALTER TABLE core.land_contract_covenants ALTER COLUMN created_at SET DEFAULT core.now();
ALTER TABLE core.land_contracts ALTER COLUMN created_at SET DEFAULT core.now();
ALTER TABLE core.land_decisions ALTER COLUMN decided_at SET DEFAULT core.now();
ALTER TABLE core.land_plots ALTER COLUMN created_at SET DEFAULT core.now();
ALTER TABLE core.land_plots ALTER COLUMN updated_at SET DEFAULT core.now();
ALTER TABLE core.ledger_accounts ALTER COLUMN created_at SET DEFAULT core.now();
ALTER TABLE core.ledger_entries ALTER COLUMN occurred_at SET DEFAULT core.now();
ALTER TABLE core.meeting_attendance ALTER COLUMN recorded_at SET DEFAULT core.now();
ALTER TABLE core.member_recusals ALTER COLUMN decided_at SET DEFAULT core.now();
ALTER TABLE core.notifications ALTER COLUMN created_at SET DEFAULT core.now();
ALTER TABLE core.objects ALTER COLUMN created_at SET DEFAULT core.now();
ALTER TABLE core.objects ALTER COLUMN updated_at SET DEFAULT core.now();
ALTER TABLE core.obligations ALTER COLUMN created_at SET DEFAULT core.now();
ALTER TABLE core.obligations ALTER COLUMN started_at SET DEFAULT core.now();
ALTER TABLE core.obligations ALTER COLUMN updated_at SET DEFAULT core.now();
ALTER TABLE core.price_proposals ALTER COLUMN created_at SET DEFAULT core.now();
ALTER TABLE core.protocols ALTER COLUMN generated_at SET DEFAULT core.now();
ALTER TABLE core.public_records ALTER COLUMN published_at SET DEFAULT core.now();
ALTER TABLE core.public_records ALTER COLUMN unpublish_at SET DEFAULT core.now();
ALTER TABLE core.role_grants ALTER COLUMN granted_at SET DEFAULT core.now();
ALTER TABLE core.special_board_decisions ALTER COLUMN decided_at SET DEFAULT core.now();
ALTER TABLE core.special_request_files ALTER COLUMN uploaded_at SET DEFAULT core.now();
ALTER TABLE core.special_requests ALTER COLUMN created_at SET DEFAULT core.now();
ALTER TABLE core.special_requests ALTER COLUMN submitted_at SET DEFAULT core.now();
ALTER TABLE core.special_requests ALTER COLUMN updated_at SET DEFAULT core.now();
ALTER TABLE core.special_reviews ALTER COLUMN created_at SET DEFAULT core.now();
ALTER TABLE core.tender_amendments ALTER COLUMN created_at SET DEFAULT core.now();
ALTER TABLE core.tender_docs ALTER COLUMN published_at SET DEFAULT core.now();
ALTER TABLE core.tenders ALTER COLUMN created_at SET DEFAULT core.now();
ALTER TABLE core.tenders ALTER COLUMN updated_at SET DEFAULT core.now();
ALTER TABLE core.user_identities ALTER COLUMN linked_at SET DEFAULT core.now();
ALTER TABLE core.users ALTER COLUMN created_at SET DEFAULT core.now();
ALTER TABLE core.users ALTER COLUMN updated_at SET DEFAULT core.now();
ALTER TABLE core.votes ALTER COLUMN cast_at SET DEFAULT core.now();

-- Правила и отметки внутри триггерных функций. Миграция T68 перевела
-- на `core.now()` переходы тендера, журнал и ставки, но восемь функций
-- остались на часах процесса. Среди них не только отметки (`failed_at`,
-- `cancelled_at`, `withdrawn_at`, `checklist_done_at`, проводка книги,
-- запись досье), но и правило срока: окно правки документации (п. 27)
-- сравнивало `submission_deadline` со временем процесса, а сам дедлайн
-- живет в часах домена.
--
-- Тела приведены целиком: изменить одну строку в теле функции Postgres
-- не позволяет. Отличие от прежней версии - только `now()` -> `core.now()`.

CREATE OR REPLACE FUNCTION core.check_failure_ground()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
  IF NEW.status = 'failed' AND OLD.status IS DISTINCT FROM 'failed'
     AND NEW.failure_ground IS NULL THEN
    RAISE EXCEPTION 'FR-801: тендер признается несостоявшимся только по основанию п. 81';
  END IF;
  IF NEW.status = 'failed' AND OLD.status IS DISTINCT FROM 'failed' THEN
    NEW.failed_at := coalesce(NEW.failed_at, core.now());
  END IF;
  RETURN NEW;
END $function$
;

CREATE OR REPLACE FUNCTION core.check_land_application_transition()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
  IF NEW.status = OLD.status THEN
    RETURN NEW;
  END IF;

  IF OLD.status <> 'submitted' THEN
    RAISE EXCEPTION 'FR-1801: переход заявки на участок % → % запрещен (п. 105–106)',
      OLD.status, NEW.status;
  END IF;

  IF NEW.status = 'withdrawn' AND NEW.withdrawn_at IS NULL THEN
    NEW.withdrawn_at := core.now();  -- время отзыва задает сервер (NFR-03)
  END IF;

  RETURN NEW;
END $function$
;

CREATE OR REPLACE FUNCTION core.check_special_request_transition()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
  IF NEW.status = OLD.status THEN
    RETURN NEW;
  END IF;

  IF NOT (
    (OLD.status = 'submitted'    AND NEW.status IN ('under_review', 'withdrawn')) OR
    (OLD.status = 'under_review' AND NEW.status IN ('granted', 'refused', 'redirected', 'withdrawn'))
  ) THEN
    RAISE EXCEPTION 'FR-1201: переход заявки особого порядка % → % запрещен (п. 88–90)',
      OLD.status, NEW.status;
  END IF;

  IF NEW.status = 'withdrawn' AND NEW.withdrawn_at IS NULL THEN
    NEW.withdrawn_at := core.now();  -- время отзыва задает сервер (NFR-03)
  END IF;

  RETURN NEW;
END $function$
;

CREATE OR REPLACE FUNCTION core.check_tender_amendment()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE
  tender core.tenders%ROWTYPE;
BEGIN
  SELECT * INTO tender FROM core.tenders WHERE id = NEW.tender_id FOR UPDATE;

  IF tender.status NOT IN ('announced', 'accepting', 'repeat_announced') THEN
    RAISE EXCEPTION
      'FR-304: документация изменяется между публикацией и вскрытием (сейчас %)', tender.status;
  END IF;
  IF tender.opened_at IS NOT NULL THEN
    RAISE EXCEPTION 'FR-304: заявки вскрыты — условия тендера больше не меняются (п. 50)';
  END IF;
  IF tender.submission_deadline IS NULL THEN
    RAISE EXCEPTION 'FR-304: у тендера не назначен срок приема заявок';
  END IF;
  IF tender.submission_deadline < core.now() THEN
    RAISE EXCEPTION 'FR-304: срок приема заявок истек — изменение невозможно (п. 27)';
  END IF;
  IF tender.submission_deadline - core.now() < interval '2 days' THEN
    RAISE EXCEPTION
      'FR-304: до окончания приема меньше 2 календарных дней — документация не изменяется (п. 27)';
  END IF;
  IF NEW.new_deadline <= tender.submission_deadline THEN
    RAISE EXCEPTION 'FR-304: новая редакция обязана продлить срок приема заявок (п. 27)';
  END IF;
  IF NEW.new_deadline - core.now() < interval '10 days' THEN
    RAISE EXCEPTION
      'FR-304: срок приема продлевается не менее чем на 10 календарных дней (п. 27)';
  END IF;
  IF btrim(NEW.summary) = '' THEN
    RAISE EXCEPTION 'FR-304: редакция публикуется с описанием изменений (п. 27)';
  END IF;

  NEW.previous_deadline := tender.submission_deadline;
  NEW.version := coalesce(
    (SELECT max(a.version) + 1 FROM core.tender_amendments a WHERE a.tender_id = NEW.tender_id),
    1);
  RETURN NEW;
END $function$
;

CREATE OR REPLACE FUNCTION core.check_tender_cancellation()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
  IF NEW.status <> 'cancelled' OR OLD.status = 'cancelled' THEN
    RETURN NEW;
  END IF;

  IF btrim(coalesce(NEW.cancel_reason, '')) = '' THEN
    RAISE EXCEPTION 'FR-305: тендер отменяется с указанием нарушения (п. 78)';
  END IF;
  IF EXISTS (
       SELECT 1 FROM core.contracts c
       WHERE c.tender_id = NEW.id AND c.registered_at IS NOT NULL
     ) THEN
    RAISE EXCEPTION
      'FR-305: по тендеру заключен договор — отмена возможна только до его заключения (п. 78)';
  END IF;

  NEW.cancelled_at := coalesce(NEW.cancelled_at, core.now());
  RETURN NEW;
END $function$
;

CREATE OR REPLACE FUNCTION core.enforce_checklist_before_signing()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE
  total   integer;
  checked integer;
BEGIN
  IF NEW.landlord_signed_at IS NULL OR OLD.landlord_signed_at IS NOT NULL THEN
    RETURN NEW;
  END IF;

  SELECT count(*), count(*) FILTER (WHERE checked_at IS NOT NULL)
  INTO total, checked
  FROM core.contract_checklists WHERE contract_id = NEW.id;

  IF total = 0 THEN
    RAISE EXCEPTION 'INV-115: чек-лист сверки документов не сформирован (п. 113)';
  END IF;
  IF checked < total THEN
    RAISE EXCEPTION
      'INV-115: сверка документов не завершена (%/% позиций) — договор не подписывается (п. 113, 115)',
      checked, total;
  END IF;

  NEW.checklist_done_at := coalesce(NEW.checklist_done_at, core.now());
  RETURN NEW;
END $function$
;

CREATE OR REPLACE FUNCTION core.enforce_ledger_balance()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE
  balance numeric(14,2);
BEGIN
  PERFORM 1 FROM core.ledger_accounts WHERE id = NEW.account_id FOR UPDATE;

  SELECT coalesce(sum(credit - debit), 0) INTO balance
  FROM core.ledger_entries WHERE account_id = NEW.account_id;

  IF balance + NEW.credit - NEW.debit < 0 THEN
    RAISE EXCEPTION 'INV-DB-05: операция % уводит баланс счета % в минус (% - %)',
      NEW.op, NEW.account_id, balance + NEW.credit, NEW.debit
      USING ERRCODE = 'check_violation';
  END IF;

  NEW.occurred_at := core.now();
  RETURN NEW;
END $function$
;

CREATE OR REPLACE FUNCTION core.record_dossier_item(p_tender_id uuid, p_kind text, p_title text, p_file_key text, p_source_table text, p_source_id uuid, p_special_request_id uuid DEFAULT NULL::uuid)
 RETURNS void
 LANGUAGE plpgsql
AS $function$
BEGIN
  IF p_tender_id IS NOT NULL THEN
    INSERT INTO core.dossier_items
      (tender_id, kind, title, file_key, source_table, source_id, occurred_at)
    VALUES (p_tender_id, p_kind, p_title, p_file_key, p_source_table, p_source_id, core.now())
    ON CONFLICT (tender_id, kind, source_table, source_id)
      WHERE tender_id IS NOT NULL AND source_id IS NOT NULL
    DO UPDATE SET file_key = coalesce(EXCLUDED.file_key, core.dossier_items.file_key),
                  title    = coalesce(EXCLUDED.title, core.dossier_items.title);
    RETURN;
  END IF;

  IF p_special_request_id IS NULL THEN
    RETURN;  -- материал вне досье: договор из одного источника вне особого порядка
  END IF;

  INSERT INTO core.dossier_items
    (special_request_id, kind, title, file_key, source_table, source_id, occurred_at)
  VALUES (p_special_request_id, p_kind, p_title, p_file_key, p_source_table, p_source_id, core.now())
  ON CONFLICT (special_request_id, kind, source_table, source_id)
    WHERE special_request_id IS NOT NULL AND source_id IS NOT NULL
  DO UPDATE SET file_key = coalesce(EXCLUDED.file_key, core.dossier_items.file_key),
                title    = coalesce(EXCLUDED.title, core.dossier_items.title);
END $function$;
