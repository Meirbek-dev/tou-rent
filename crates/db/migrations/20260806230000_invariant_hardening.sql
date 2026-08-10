-- Усиление инвариантов, которые раньше обеспечивались только application-кодом.

-- FR-206: гарантийный взнос является снимком той же месячной ставки, а не просто
-- произвольным положительным числом.
ALTER TABLE core.lots
  ADD CONSTRAINT lots_guarantee_fee_equals_monthly_rate
  CHECK (guarantee_fee = base_rate_monthly) NOT VALID;
ALTER TABLE core.lots
  VALIDATE CONSTRAINT lots_guarantee_fee_equals_monthly_rate;

-- FR-303: объявление без лотов не может быть опубликовано даже прямым SQL.
CREATE OR REPLACE FUNCTION core.enforce_tender_transition() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.status IS DISTINCT FROM NEW.status THEN
    IF NOT EXISTS (
      SELECT 1 FROM refdata.tender_status_transitions t
      WHERE t.from_status = OLD.status AND t.to_status = NEW.status
    ) THEN
      RAISE EXCEPTION 'INV-021: переход статуса тендера % -> % запрещен', OLD.status, NEW.status
        USING ERRCODE = 'check_violation';
    END IF;

    IF NEW.status IN ('announced', 'repeat_announced') THEN
      IF NOT EXISTS (SELECT 1 FROM core.lots l WHERE l.tender_id = NEW.id) THEN
        RAISE EXCEPTION 'FR-303: публикация тендера без хотя бы одного лота невозможна'
          USING ERRCODE = 'check_violation';
      END IF;
      IF NEW.opening_at IS NULL OR NEW.submission_deadline IS NULL THEN
        RAISE EXCEPTION 'FR-303: публикация без дат вскрытия и дедлайна приема невозможна'
          USING ERRCODE = 'check_violation';
      END IF;
      IF NEW.opening_at < now() + interval '10 days' THEN
        RAISE EXCEPTION 'FR-303: между публикацией и вскрытием должно быть >= 10 календарных дней'
          USING ERRCODE = 'check_violation';
      END IF;
      NEW.announced_at := now();
    END IF;
  END IF;
  RETURN NEW;
END $$;

-- INV-066: после первоначального назначения ends_at допускается ровно одно
-- продление и строго на 15 минут. Одновременная смена статуса не обходит правило.
CREATE OR REPLACE FUNCTION core.enforce_auction_extension() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.ends_at IS NULL THEN
    IF NEW.extended_once THEN
      RAISE EXCEPTION 'INV-066: первоначальное назначение времени не является продлением'
        USING ERRCODE = 'check_violation';
    END IF;
    RETURN NEW;
  END IF;

  IF NEW.ends_at IS DISTINCT FROM OLD.ends_at THEN
    IF OLD.status <> 'running' OR NEW.status <> 'running' THEN
      RAISE EXCEPTION 'INV-066: время окончания меняется только у идущего аукциона'
        USING ERRCODE = 'check_violation';
    END IF;
    IF OLD.extended_once THEN
      RAISE EXCEPTION 'INV-066: повторное продление запрещено'
        USING ERRCODE = 'check_violation';
    END IF;
    IF NEW.ends_at <> OLD.ends_at + interval '15 minutes' THEN
      RAISE EXCEPTION 'INV-066: аукцион продлевается ровно на 15 минут'
        USING ERRCODE = 'check_violation';
    END IF;
    NEW.extended_once := true;
  ELSIF NEW.extended_once IS DISTINCT FROM OLD.extended_once THEN
    RAISE EXCEPTION 'INV-066: признак продления меняется только вместе с ends_at'
      USING ERRCODE = 'check_violation';
  END IF;

  RETURN NEW;
END $$;

-- Участник ставки обязан подаваться на лот именно этого аукциона.
CREATE OR REPLACE FUNCTION core.enforce_bid_rules() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  a core.auctions%ROWTYPE;
  application_lot uuid;
  current_max numeric(14,2);
BEGIN
  SELECT * INTO a FROM core.auctions WHERE id = NEW.auction_id FOR UPDATE;
  SELECT lot_id INTO application_lot FROM core.applications WHERE id = NEW.application_id;

  IF application_lot IS DISTINCT FROM a.lot_id THEN
    RAISE EXCEPTION 'INV-063: заявка относится не к лоту аукциона'
      USING ERRCODE = 'foreign_key_violation';
  END IF;
  IF a.status <> 'running' THEN
    RAISE EXCEPTION 'ставка отклонена: аукцион не в статусе running (текущий: %)', a.status
      USING ERRCODE = 'check_violation';
  END IF;
  IF a.ends_at IS NOT NULL AND now() > a.ends_at THEN
    RAISE EXCEPTION 'INV-066: время торгов истекло (%)', a.ends_at
      USING ERRCODE = 'check_violation';
  END IF;

  SELECT max(amount) INTO current_max FROM core.bids WHERE auction_id = NEW.auction_id;
  IF NEW.amount < coalesce(current_max, a.starting_bid) + a.bid_step THEN
    RAISE EXCEPTION 'INV-063: ставка ниже максимума плюс шаг'
      USING ERRCODE = 'check_violation';
  END IF;

  NEW.placed_at := now();
  RETURN NEW;
END $$;

-- Победитель и второе место должны принадлежать лоту и иметь соответствующую
-- ставку именно в этом аукционе.
ALTER TABLE core.applications
  ADD CONSTRAINT applications_id_lot_key UNIQUE (id, lot_id);

ALTER TABLE core.auctions
  DROP CONSTRAINT auctions_winner_application_id_fkey,
  DROP CONSTRAINT auctions_runner_up_application_id_fkey,
  ADD CONSTRAINT auctions_winner_application_lot_fkey
    FOREIGN KEY (winner_application_id, lot_id)
    REFERENCES core.applications (id, lot_id),
  ADD CONSTRAINT auctions_runner_up_application_lot_fkey
    FOREIGN KEY (runner_up_application_id, lot_id)
    REFERENCES core.applications (id, lot_id),
  ADD CONSTRAINT auctions_distinct_results
    CHECK (winner_application_id IS NULL OR runner_up_application_id IS NULL
           OR winner_application_id <> runner_up_application_id),
  ADD CONSTRAINT auctions_winner_result_complete
    CHECK ((winner_application_id IS NULL) = (winner_amount IS NULL)),
  ADD CONSTRAINT auctions_runner_up_result_complete
    CHECK ((runner_up_application_id IS NULL) = (runner_up_amount IS NULL));

CREATE FUNCTION core.enforce_auction_results() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.winner_application_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM core.bids b
    WHERE b.auction_id = NEW.id
      AND b.application_id = NEW.winner_application_id
      AND b.amount = NEW.winner_amount
  ) THEN
    RAISE EXCEPTION 'FR-606: победитель и его сумма не относятся к этому аукциону'
      USING ERRCODE = 'foreign_key_violation';
  END IF;
  IF NEW.runner_up_application_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM core.bids b
    WHERE b.auction_id = NEW.id
      AND b.application_id = NEW.runner_up_application_id
      AND b.amount = NEW.runner_up_amount
  ) THEN
    RAISE EXCEPTION 'FR-606: второе место и его сумма не относятся к этому аукциону'
      USING ERRCODE = 'foreign_key_violation';
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER enforce_auction_results
  BEFORE INSERT OR UPDATE OF winner_application_id, winner_amount,
    runner_up_application_id, runner_up_amount, lot_id
  ON core.auctions
  FOR EACH ROW EXECUTE FUNCTION core.enforce_auction_results();

-- FR-1302: одно приглашение участнику на один тендер. Запрос по JSONB остается
-- оптимизацией чтения, но однократность теперь защищена UNIQUE-индексом.
CREATE UNIQUE INDEX notifications_auction_invitation_once_idx
  ON core.notifications (user_id, ((payload ->> 'tender_id')))
  WHERE kind = 'auction_invitation' AND payload ? 'tender_id';

-- INV-AUDIT: первоначальная цена является юридически значимым неизменяемым фактом.
CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.price_proposals
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Стабильный каталог option_code отделяет идентичность опции от ее версий по датам.
CREATE TABLE refdata.rate_options (
  coefficient text NOT NULL,
  option_code text NOT NULL,
  PRIMARY KEY (coefficient, option_code)
);

INSERT INTO refdata.rate_options (coefficient, option_code)
SELECT DISTINCT coefficient, option_code FROM refdata.rate_coefficients;

ALTER TABLE refdata.rate_coefficients
  ADD CONSTRAINT rate_coefficients_option_fkey
  FOREIGN KEY (coefficient, option_code)
  REFERENCES refdata.rate_options (coefficient, option_code);

ALTER TABLE core.objects
  ADD COLUMN premises_type_coefficient text GENERATED ALWAYS AS ('kt') STORED,
  ADD COLUMN premises_kind_coefficient text GENERATED ALWAYS AS ('ksk') STORED,
  ADD COLUMN comfort_coefficient text GENERATED ALWAYS AS ('kk') STORED,
  ADD COLUMN location_coefficient text GENERATED ALWAYS AS ('kr') STORED,
  ADD CONSTRAINT objects_premises_type_option_fkey
    FOREIGN KEY (premises_type_coefficient, premises_type_code)
    REFERENCES refdata.rate_options (coefficient, option_code),
  ADD CONSTRAINT objects_premises_kind_option_fkey
    FOREIGN KEY (premises_kind_coefficient, premises_kind_code)
    REFERENCES refdata.rate_options (coefficient, option_code),
  ADD CONSTRAINT objects_comfort_option_fkey
    FOREIGN KEY (comfort_coefficient, comfort_code)
    REFERENCES refdata.rate_options (coefficient, option_code),
  ADD CONSTRAINT objects_location_option_fkey
    FOREIGN KEY (location_coefficient, location_code)
    REFERENCES refdata.rate_options (coefficient, option_code);

-- Поддерживающие индексы FK, не покрытые PRIMARY KEY/UNIQUE или существующими
-- индексами с тем же первым столбцом.
CREATE INDEX tenders_organizer_idx ON core.tenders (organizer_id);
CREATE INDEX tenders_repeat_of_idx ON core.tenders (repeat_of) WHERE repeat_of IS NOT NULL;
CREATE INDEX lots_object_idx ON core.lots (object_id);
CREATE INDEX applications_participant_idx ON core.applications (participant_id);
CREATE INDEX applications_fee_confirmed_by_idx ON core.applications (fee_confirmed_by)
  WHERE fee_confirmed_by IS NOT NULL;
CREATE INDEX application_files_application_idx ON core.application_files (application_id);
CREATE INDEX journal_entries_application_idx ON core.journal_entries (application_id)
  WHERE application_id IS NOT NULL;
CREATE INDEX journal_entries_actor_idx ON core.journal_entries (actor_id)
  WHERE actor_id IS NOT NULL;
CREATE INDEX commission_members_user_idx ON core.commission_members (user_id);
CREATE INDEX coi_declarations_tender_idx ON core.coi_declarations (tender_id);
CREATE INDEX sessions_meetings_commission_idx ON core.sessions_meetings (commission_id);
CREATE INDEX votes_application_idx ON core.votes (application_id);
CREATE INDEX votes_member_idx ON core.votes (member_id);
CREATE INDEX protocols_meeting_idx ON core.protocols (meeting_id) WHERE meeting_id IS NOT NULL;
CREATE INDEX bids_application_idx ON core.bids (application_id);
CREATE INDEX contracts_tender_idx ON core.contracts (tender_id) WHERE tender_id IS NOT NULL;
CREATE INDEX contracts_lot_idx ON core.contracts (lot_id) WHERE lot_id IS NOT NULL;
CREATE INDEX contracts_object_idx ON core.contracts (object_id);
CREATE INDEX contracts_tenant_idx ON core.contracts (tenant_id);
CREATE INDEX contracts_protocol_idx ON core.contracts (protocol_id) WHERE protocol_id IS NOT NULL;
CREATE INDEX contract_checklists_checked_by_idx ON core.contract_checklists (checked_by)
  WHERE checked_by IS NOT NULL;
CREATE INDEX ledger_accounts_owner_idx ON core.ledger_accounts (owner_user_id);
CREATE INDEX ledger_entries_account_idx ON core.ledger_entries (account_id);
CREATE INDEX ledger_entries_recorded_by_idx ON core.ledger_entries (recorded_by)
  WHERE recorded_by IS NOT NULL;
CREATE INDEX obligations_tender_idx ON core.obligations (tender_id) WHERE tender_id IS NOT NULL;
CREATE INDEX obligations_contract_idx ON core.obligations (contract_id) WHERE contract_id IS NOT NULL;
CREATE INDEX obligations_application_idx ON core.obligations (application_id)
  WHERE application_id IS NOT NULL;
CREATE INDEX notifications_user_idx ON core.notifications (user_id);
CREATE INDEX special_requests_applicant_idx ON core.special_requests (applicant_id);
CREATE INDEX dossier_items_tender_idx ON core.dossier_items (tender_id) WHERE tender_id IS NOT NULL;
CREATE INDEX dossier_items_special_request_idx ON core.dossier_items (special_request_id)
  WHERE special_request_id IS NOT NULL;
