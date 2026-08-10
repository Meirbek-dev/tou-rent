-- Депозитная книга двойной записи (М10) и двигатель обязательств-сроков (М17).

-- Счет: «взнос участника по лоту» либо «депозит по договору» (FR-1001)
CREATE TABLE core.ledger_accounts (
  id             uuid                     PRIMARY KEY DEFAULT uuidv7(),
  kind           core.ledger_account_kind NOT NULL,
  application_id uuid                     UNIQUE REFERENCES core.applications (id),
  contract_id    uuid                     UNIQUE REFERENCES core.contracts (id),
  owner_user_id  uuid                     NOT NULL REFERENCES core.users (id),
  created_at     timestamptz              NOT NULL DEFAULT now(),
  -- Счет привязан ровно к одному основанию, соответствующему его типу
  CONSTRAINT account_binding CHECK (
    (kind = 'participant_fee'  AND application_id IS NOT NULL AND contract_id IS NULL) OR
    (kind = 'contract_deposit' AND contract_id IS NOT NULL AND application_id IS NULL)
  )
);

-- Проводка: debit XOR credit (INV-DB-05). credit — поступление/восполнение,
-- debit — удержание/зачет/возврат/списание. Подтверждает поступления finance вручную (FR-405).
CREATE TABLE core.ledger_entries (
  id          uuid           PRIMARY KEY DEFAULT uuidv7(),
  account_id  uuid           NOT NULL REFERENCES core.ledger_accounts (id),
  op          core.ledger_op NOT NULL,
  debit       numeric(14,2)  NOT NULL DEFAULT 0 CHECK (debit >= 0),
  credit      numeric(14,2)  NOT NULL DEFAULT 0 CHECK (credit >= 0),
  rule_ref    text,          -- пункт Правил-основание (п. 26, 132–136)
  note        text,
  recorded_by uuid           REFERENCES core.users (id),
  occurred_at timestamptz    NOT NULL DEFAULT now(),
  CONSTRAINT debit_xor_credit CHECK ((debit = 0) <> (credit = 0)),
  -- Направление операции задано ее типом
  CONSTRAINT op_direction CHECK (
    CASE op
      WHEN 'receipt_confirmed' THEN credit > 0
      WHEN 'replenish'         THEN credit > 0
      ELSE debit > 0
    END
  )
);

-- INV-DB-05: баланс счета не уходит в минус. FOR UPDATE на счете сериализует проводки.
CREATE FUNCTION core.enforce_ledger_balance() RETURNS trigger
LANGUAGE plpgsql AS $$
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

  NEW.occurred_at := now();
  RETURN NEW;
END $$;

CREATE TRIGGER enforce_ledger_balance BEFORE INSERT ON core.ledger_entries
  FOR EACH ROW EXECUTE FUNCTION core.enforce_ledger_balance();

CREATE TRIGGER ledger_append_only BEFORE UPDATE OR DELETE ON core.ledger_entries
  FOR EACH ROW EXECUTE FUNCTION core.forbid_mutation('INV-DB-05');

-- Обязательства-сроки (FR-1702): каждое событие процесса порождает записи
-- с due_at по производственному календарю (refdata.add_business_days).
CREATE TABLE core.obligations (
  id             uuid                   PRIMARY KEY DEFAULT uuidv7(),
  rule_ref       text                   NOT NULL,  -- пункт Правил (п. 54, 57–59, 111–118, ...)
  action         text                   NOT NULL,  -- машинный код действия
  assignee_role  core.role              NOT NULL,
  tender_id      uuid                   REFERENCES core.tenders (id),
  contract_id    uuid                   REFERENCES core.contracts (id),
  application_id uuid                   REFERENCES core.applications (id),
  due_at         timestamptz            NOT NULL,
  status         core.obligation_status NOT NULL DEFAULT 'pending',
  completed_at   timestamptz,
  created_at     timestamptz            NOT NULL DEFAULT now(),
  updated_at     timestamptz            NOT NULL DEFAULT now()
);

CREATE INDEX obligations_due_idx ON core.obligations (status, due_at);

CREATE TRIGGER touch_updated_at BEFORE UPDATE ON core.obligations
  FOR EACH ROW EXECUTE FUNCTION core.touch_updated_at();
