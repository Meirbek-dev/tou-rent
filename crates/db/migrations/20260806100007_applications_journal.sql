-- Заявки, ценовые предложения (запечатанные), журнал регистрации (М4).

CREATE TABLE core.applications (
  id                uuid                    PRIMARY KEY DEFAULT uuidv7(),
  tender_id         uuid                    NOT NULL REFERENCES core.tenders (id),
  lot_id            uuid                    NOT NULL REFERENCES core.lots (id),
  participant_id    uuid                    NOT NULL REFERENCES core.users (id),
  status            core.application_status NOT NULL DEFAULT 'submitted',
  applicant_kind    core.applicant_kind     NOT NULL,
  applicant_details jsonb                   NOT NULL,  -- Прил. 2 (перс. данные: NFR-07 — в логи не выводить)
  qualification     jsonb,                             -- Прил. 11
  fee_confirmed_at  timestamptz,                       -- FR-405: подтверждает finance вручную
  fee_confirmed_by  uuid                    REFERENCES core.users (id),
  rejection_reason  text                    REFERENCES refdata.rejection_reasons (code),  -- INV-052
  submitted_at      timestamptz             NOT NULL DEFAULT now(),
  withdrawn_at      timestamptz,
  updated_at        timestamptz             NOT NULL DEFAULT now(),
  -- Один участник — одна заявка на лот; один взнос — один лот (п. 22)
  UNIQUE (lot_id, participant_id),
  CONSTRAINT rejection_needs_reason
    CHECK (status <> 'rejected' OR rejection_reason IS NOT NULL),
  CONSTRAINT withdrawal_has_timestamp
    CHECK (status <> 'withdrawn' OR withdrawn_at IS NOT NULL)
);

CREATE INDEX applications_tender_idx ON core.applications (tender_id);

CREATE TRIGGER touch_updated_at BEFORE UPDATE ON core.applications
  FOR EACH ROW EXECUTE FUNCTION core.touch_updated_at();

CREATE TABLE core.application_files (
  id             uuid        PRIMARY KEY DEFAULT uuidv7(),
  application_id uuid        NOT NULL REFERENCES core.applications (id) ON DELETE CASCADE,
  file_key       text        NOT NULL,  -- RustFS
  filename       text        NOT NULL,
  content_type   text        NOT NULL,
  size_bytes     bigint      NOT NULL CHECK (size_bytes >= 0),
  uploaded_at    timestamptz NOT NULL DEFAULT now()
);

-- Ценовое предложение (Прил. 9) — отдельная таблица под RLS (INV-040, п. 40–41):
-- до события «вскрытие» (tenders.opened_at) строки не видит НИКТО, включая
-- organizer и admin; участник видит только свое. Роль приложения tou_rent_app
-- не имеет BYPASSRLS; superuser используется только для миграций (A-011).
CREATE TABLE core.price_proposals (
  id             uuid          PRIMARY KEY DEFAULT uuidv7(),
  application_id uuid          NOT NULL UNIQUE REFERENCES core.applications (id) ON DELETE CASCADE,
  amount         numeric(14,2) NOT NULL CHECK (amount > 0),
  created_at     timestamptz   NOT NULL DEFAULT now()
);

ALTER TABLE core.price_proposals ENABLE ROW LEVEL SECURITY;
ALTER TABLE core.price_proposals FORCE ROW LEVEL SECURITY;

CREATE POLICY sealed_until_opening ON core.price_proposals FOR SELECT
  USING (
    EXISTS (
      SELECT 1
      FROM core.applications a
      JOIN core.tenders t ON t.id = a.tender_id
      WHERE a.id = price_proposals.application_id
        AND (t.opened_at IS NOT NULL                          -- вскрытие состоялось (FR-403)
             OR a.participant_id = core.current_app_user())   -- участник видит свое предложение
    )
  );

CREATE POLICY insert_own_proposal ON core.price_proposals FOR INSERT
  WITH CHECK (
    EXISTS (
      SELECT 1 FROM core.applications a
      WHERE a.id = price_proposals.application_id
        AND a.participant_id = core.current_app_user()
    )
  );
-- UPDATE/DELETE-политик нет: предложение неизменяемо (default deny под RLS)

-- Журнал регистрации заявок (Прил. 12, INV-037): append-only, seq монотонен
-- в рамках тендера, время сервера, вставка после дедлайна отклоняется на уровне БД.
CREATE TABLE core.journal_entries (
  id             uuid                    PRIMARY KEY DEFAULT uuidv7(),
  tender_id      uuid                    NOT NULL REFERENCES core.tenders (id),
  seq            int                     NOT NULL,
  entry_kind     core.journal_entry_kind NOT NULL,
  application_id uuid                    REFERENCES core.applications (id),
  actor_id       uuid                    REFERENCES core.users (id),
  occurred_at    timestamptz             NOT NULL DEFAULT now(),
  note           text,
  UNIQUE (tender_id, seq)
);

-- Счетчик seq на тендер: UPSERT под блокировкой строки сериализует конкурентные
-- вставки (NFR-02: 100 одновременных подач без потери seq)
CREATE TABLE core.journal_counters (
  tender_id uuid PRIMARY KEY REFERENCES core.tenders (id),
  last_seq  int  NOT NULL DEFAULT 0
);

CREATE FUNCTION core.journal_before_insert() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  deadline timestamptz;
BEGIN
  SELECT submission_deadline INTO deadline FROM core.tenders WHERE id = NEW.tender_id;

  IF deadline IS NOT NULL AND now() > deadline THEN
    RAISE EXCEPTION 'INV-037: прием закрыт — дедлайн % истек (п. 37–39)', deadline
      USING ERRCODE = 'check_violation';
  END IF;

  -- Сервер — единственный источник времени и порядка (NFR-03)
  NEW.occurred_at := now();
  INSERT INTO core.journal_counters AS c (tender_id, last_seq)
  VALUES (NEW.tender_id, 1)
  ON CONFLICT (tender_id) DO UPDATE SET last_seq = c.last_seq + 1
  RETURNING last_seq INTO NEW.seq;

  RETURN NEW;
END $$;

CREATE TRIGGER journal_before_insert BEFORE INSERT ON core.journal_entries
  FOR EACH ROW EXECUTE FUNCTION core.journal_before_insert();

CREATE TRIGGER journal_append_only BEFORE UPDATE OR DELETE ON core.journal_entries
  FOR EACH ROW EXECUTE FUNCTION core.forbid_mutation('INV-037');
