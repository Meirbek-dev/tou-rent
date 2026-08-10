-- Договорный конвейер (М9, FR-901–902, FR-905, INV-115, п. 108–115, 126).
-- Шаги п. 110–115 фиксируются отметками времени, существенные условия
-- неизменяемы (FR-901), а подписание наймодателя блокируется без сверки
-- документов (INV-115) — последним рубежом, а не только формой.

ALTER TABLE core.contracts
  ADD COLUMN winner_application_id uuid REFERENCES core.applications (id),
  ADD COLUMN lease_months          integer CHECK (lease_months > 0),
  ADD COLUMN drafted_at            timestamptz,
  ADD COLUMN handed_to_tenant_at   timestamptz,
  ADD COLUMN tenant_signed_at      timestamptz,
  ADD COLUMN documents_received_at timestamptz,
  ADD COLUMN checklist_done_at     timestamptz,
  ADD COLUMN landlord_signed_at    timestamptz,
  ADD COLUMN copy_sent_at          timestamptz,
  ADD COLUMN pdf_key               text;

COMMENT ON COLUMN core.contracts.landlord_signed_at IS
  'INV-115: проставляется только при завершенной сверке документов (п. 113, 115)';

-- Перечень документов для сверки (п. 113): свой для физического и своего
-- для юридического лица. TODO-ENGINEER: состав сверяется по Правилам (Q-005).
CREATE TABLE refdata.checklist_items (
  code           text                 PRIMARY KEY,
  applicant_kind core.applicant_kind,  -- NULL — требуется для обоих
  label_ru       text                 NOT NULL,
  label_kk       text,
  label_en       text,
  rule_ref       text                 NOT NULL,
  seq            integer              NOT NULL
);

INSERT INTO refdata.checklist_items (code, applicant_kind, label_ru, label_kk, label_en, rule_ref, seq) VALUES
  ('identity', 'individual', 'Документ, удостоверяющий личность',
   'Жеке басын куәландыратын құжат', 'Identity document', 'п. 113.1', 1),
  ('tax_individual', 'individual', 'Справка о регистрации в налоговом органе',
   'Салық органында тіркелгені туралы анықтама', 'Tax registration certificate', 'п. 113.2', 2),
  ('charter', 'legal_entity', 'Устав юридического лица',
   'Заңды тұлғаның жарғысы', 'Charter of the legal entity', 'п. 113.3', 3),
  ('state_registration', 'legal_entity', 'Справка о государственной регистрации',
   'Мемлекеттік тіркеу туралы анықтама', 'State registration certificate', 'п. 113.4', 4),
  ('signatory_authority', 'legal_entity', 'Документ о полномочиях подписанта',
   'Қол қоюшының өкілеттігі туралы құжат', 'Proof of signatory authority', 'п. 113.5', 5),
  ('bank_details', NULL, 'Банковские реквизиты',
   'Банк деректемелері', 'Bank details', 'п. 113.6', 6),
  ('fee_receipt', NULL, 'Подтверждение внесения гарантийного взноса',
   'Кепілдік жарнаның енгізілгенін растау', 'Proof of the guarantee fee payment', 'п. 113.7', 7)
ON CONFLICT DO NOTHING;

-- Позиция чек-листа ссылается на перечень (закрытый список, как INV-052)
ALTER TABLE core.contract_checklists
  ADD CONSTRAINT checklist_item_known
  FOREIGN KEY (item_code) REFERENCES refdata.checklist_items (code);

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.contract_checklists
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- FR-901: существенные условия — снимок итогов торгов, менять их нельзя
-- ни через приложение, ни мимо него.
CREATE FUNCTION core.freeze_contract_terms() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.monthly_rate IS DISTINCT FROM OLD.monthly_rate
     OR NEW.object_id IS DISTINCT FROM OLD.object_id
     OR NEW.lot_id IS DISTINCT FROM OLD.lot_id
     OR NEW.tenant_id IS DISTINCT FROM OLD.tenant_id
     OR NEW.lease_months IS DISTINCT FROM OLD.lease_months THEN
    RAISE EXCEPTION
      'FR-901: существенные условия договора (ставка, объект, лот, наниматель, срок) неизменяемы';
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER freeze_terms BEFORE UPDATE ON core.contracts
  FOR EACH ROW EXECUTE FUNCTION core.freeze_contract_terms();

-- INV-115: подпись наймодателя невозможна без завершенной сверки (п. 113, 115).
-- Проверяется по чек-листу, а не по отметке: «завершенность» — это факт,
-- что каждая позиция перечня отмечена проверившим.
CREATE FUNCTION core.enforce_checklist_before_signing() RETURNS trigger
LANGUAGE plpgsql AS $$
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

  NEW.checklist_done_at := coalesce(NEW.checklist_done_at, now());
  RETURN NEW;
END $$;

CREATE TRIGGER enforce_checklist_before_signing BEFORE UPDATE ON core.contracts
  FOR EACH ROW EXECUTE FUNCTION core.enforce_checklist_before_signing();

-- FR-905: журнал регистрации договоров — номер уникален, дата регистрации
-- равна дате заключения (п. 126); регистрация возможна после подписания
-- обеими сторонами.
CREATE UNIQUE INDEX contracts_reg_number_key ON core.contracts (reg_number)
  WHERE reg_number IS NOT NULL;

CREATE FUNCTION core.check_contract_registration() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.registered_at IS NOT NULL AND OLD.registered_at IS NULL THEN
    IF NEW.tenant_signed_at IS NULL OR NEW.landlord_signed_at IS NULL THEN
      RAISE EXCEPTION 'FR-905: регистрируется договор, подписанный обеими сторонами (п. 126)';
    END IF;
    IF NEW.reg_number IS NULL THEN
      RAISE EXCEPTION 'FR-905: регистрация без номера в журнале невозможна (п. 126)';
    END IF;
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER check_contract_registration BEFORE UPDATE ON core.contracts
  FOR EACH ROW EXECUTE FUNCTION core.check_contract_registration();
