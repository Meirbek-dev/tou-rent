-- Уклонение победителя и участника № 2 (М9, FR-903, FR-505, п. 116–120).
-- Уклонение — юридический факт со своими следствиями: взнос удерживается,
-- договор прекращается, право на договор переходит к участнику № 2, а сам
-- уклонившийся попадает в реестр, который отклоняет его будущие заявки
-- (п. 52.4, 120). Следствия применяет БД — они наступают и мимо приложения.

-- Место в итогах торгов (п. 74): третьего места Правила не знают
CREATE TYPE core.auction_place AS ENUM ('winner', 'runner_up');

-- Чей это договор: победителя или участника № 2 (FR-903)
ALTER TABLE core.contracts
  ADD COLUMN place core.auction_place NOT NULL DEFAULT 'winner';

COMMENT ON COLUMN core.contracts.place IS
  'FR-903: договор победителя либо участника № 2 после уклонения (п. 117)';

-- По лоту действует один договор: прекращенный уклонением освобождает место
-- следующему (п. 117), но два живых договора на лот невозможны.
CREATE UNIQUE INDEX contracts_live_lot_idx ON core.contracts (lot_id)
  WHERE lot_id IS NOT NULL AND status NOT IN ('terminated', 'cancelled');

-- Основания уклонения (п. 116) — справочник + FK, как основания отклонения
-- заявки (INV-052). TODO-ENGINEER: формулировки сверяются по Правилам (Q-006).
CREATE TABLE refdata.evasion_grounds (
  code     text PRIMARY KEY,
  label_ru text NOT NULL,
  label_kk text,
  label_en text,
  rule_ref text NOT NULL
);

INSERT INTO refdata.evasion_grounds (code, label_ru, label_kk, label_en, rule_ref) VALUES
  ('signing_deadline_missed', 'Подписанный договор не возвращен в установленный срок',
   'Қол қойылған шарт белгіленген мерзімде қайтарылмады',
   'Signed contract not returned within the term', 'п. 111, 116'),
  ('documents_deadline_missed', 'Документы для сверки не представлены в установленный срок',
   'Салыстыру үшін құжаттар белгіленген мерзімде ұсынылмады',
   'Documents for the check not submitted within the term', 'п. 112, 116'),
  ('refused', 'Письменный отказ от подписания договора',
   'Шартқа қол қоюдан жазбаша бас тарту',
   'Written refusal to sign the contract', 'п. 116')
ON CONFLICT DO NOTHING;

-- Факт уклонения (FR-903): по договору — не более одного
CREATE TABLE core.evasions (
  id             uuid              PRIMARY KEY DEFAULT uuidv7(),
  contract_id    uuid              NOT NULL UNIQUE REFERENCES core.contracts (id),
  tender_id      uuid              REFERENCES core.tenders (id),
  lot_id         uuid              REFERENCES core.lots (id),
  application_id uuid              REFERENCES core.applications (id),
  user_id        uuid              NOT NULL REFERENCES core.users (id),
  place          core.auction_place NOT NULL,
  ground         text              NOT NULL REFERENCES refdata.evasion_grounds (code),
  note           text,
  declared_at    timestamptz       NOT NULL DEFAULT now(),
  declared_by    uuid              REFERENCES core.users (id)
);

CREATE INDEX evasions_user_idx ON core.evasions (user_id);

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.evasions
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Уклонение — юридический факт: переписать и стереть его нельзя (как акт)
CREATE TRIGGER evasions_append_only BEFORE UPDATE OR DELETE ON core.evasions
  FOR EACH ROW EXECUTE FUNCTION core.forbid_mutation('FR-903');

REVOKE UPDATE, DELETE ON core.evasions FROM tou_rent_app;

-- Условия признания уклонения (п. 110–111, 116): экземпляр договора передан,
-- подписанный не возвращен. Те же правила выражены типом в `domain::evasion`.
CREATE FUNCTION core.check_evasion() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  contract core.contracts%ROWTYPE;
BEGIN
  SELECT * INTO contract FROM core.contracts WHERE id = NEW.contract_id FOR UPDATE;

  IF contract.handed_to_tenant_at IS NULL THEN
    RAISE EXCEPTION
      'FR-903: экземпляр договора не передавался — уклоняться не от чего (п. 110)';
  END IF;
  IF contract.tenant_signed_at IS NOT NULL THEN
    RAISE EXCEPTION 'FR-903: договор подписан нанимателем — уклонения нет (п. 111)';
  END IF;
  IF NEW.place IS DISTINCT FROM contract.place THEN
    RAISE EXCEPTION 'FR-903: место уклонившегося не совпадает с местом стороны договора (п. 74)';
  END IF;

  NEW.tender_id      := coalesce(NEW.tender_id, contract.tender_id);
  NEW.lot_id         := coalesce(NEW.lot_id, contract.lot_id);
  NEW.application_id := coalesce(NEW.application_id, contract.winner_application_id);
  NEW.user_id        := coalesce(NEW.user_id, contract.tenant_id);
  RETURN NEW;
END $$;

CREATE TRIGGER check_evasion BEFORE INSERT ON core.evasions
  FOR EACH ROW EXECUTE FUNCTION core.check_evasion();

-- Следствия уклонения (п. 116): договор прекращается, гарантийный взнос
-- уклонившегося удерживается. Удержание — проводка книги (INV-DB-05),
-- поэтому остаток счета уходит в дебет целиком, а не «списывается» полем.
CREATE FUNCTION core.apply_evasion_effects() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  account uuid;
  balance numeric(14,2);
BEGIN
  UPDATE core.contracts SET status = 'terminated' WHERE id = NEW.contract_id;

  SELECT acc.id, coalesce(sum(e.credit - e.debit), 0)::numeric(14,2)
  INTO account, balance
  FROM core.ledger_accounts acc
  LEFT JOIN core.ledger_entries e ON e.account_id = acc.id
  WHERE acc.kind = 'participant_fee' AND acc.application_id = NEW.application_id
  GROUP BY acc.id;

  IF account IS NOT NULL AND balance > 0 THEN
    INSERT INTO core.ledger_entries (account_id, op, debit, rule_ref, recorded_by, note)
    VALUES (account, 'hold', balance, 'п. 116', NEW.declared_by,
            'удержание взноса при уклонении от подписания договора');
  END IF;

  RETURN NULL;  -- AFTER-триггер
END $$;

CREATE TRIGGER apply_evasion_effects AFTER INSERT ON core.evasions
  FOR EACH ROW EXECUTE FUNCTION core.apply_evasion_effects();

-- Договор с участником № 2 возможен только после уклонения победителя (п. 117)
-- и только один раз: третьего места Правила не знают — после уклонения № 2
-- тендер идет к признанию несостоявшимся (п. 81.4).
CREATE FUNCTION core.check_runner_up_contract() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.place <> 'runner_up' THEN
    RETURN NEW;
  END IF;

  IF NOT EXISTS (
       SELECT 1 FROM core.evasions e
       WHERE e.lot_id = NEW.lot_id AND e.place = 'winner'
     ) THEN
    RAISE EXCEPTION
      'FR-903: договор с участником № 2 составляется после уклонения победителя (п. 117)';
  END IF;

  IF EXISTS (
       SELECT 1 FROM core.evasions e
       WHERE e.lot_id = NEW.lot_id AND e.place = 'runner_up'
     ) THEN
    RAISE EXCEPTION
      'FR-903: участник № 2 уклонился — договариваться больше не с кем (п. 81.4)';
  END IF;

  RETURN NEW;
END $$;

CREATE TRIGGER check_runner_up_contract BEFORE INSERT ON core.contracts
  FOR EACH ROW EXECUTE FUNCTION core.check_runner_up_contract();

-- Реестр уклонистов (FR-505, п. 120): кто, сколько раз и когда уклонился
CREATE VIEW core.evader_registry AS
SELECT
  e.user_id,
  u.full_name,
  count(*)::integer                                        AS evasions,
  max(e.declared_at)                                       AS last_declared_at,
  (array_agg(e.ground     ORDER BY e.declared_at DESC))[1] AS last_ground,
  (array_agg(e.tender_id  ORDER BY e.declared_at DESC))[1] AS last_tender_id
FROM core.evasions e
JOIN core.users u ON u.id = e.user_id
GROUP BY e.user_id, u.full_name;

COMMENT ON VIEW core.evader_registry IS
  'FR-505: реестр уклонистов — их заявки отклоняются автоматически (п. 52.4, 120)';

-- FR-505: заявка уклониста отклоняется автоматически, решения комиссии для
-- этого не требуется — основание закрытого перечня п. 52.4 проставляется
-- при регистрации заявки. TODO-ENGINEER: срок нахождения в реестре Правилами
-- не задан, применяется бессрочно (A-054).
CREATE FUNCTION core.reject_evader_application() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF EXISTS (SELECT 1 FROM core.evasions e WHERE e.user_id = NEW.participant_id) THEN
    NEW.status           := 'rejected';
    NEW.rejection_reason := 'evader';
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER reject_evader_application BEFORE INSERT ON core.applications
  FOR EACH ROW EXECUTE FUNCTION core.reject_evader_application();
