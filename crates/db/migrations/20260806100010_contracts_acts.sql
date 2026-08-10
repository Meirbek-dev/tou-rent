-- Договоры и акты (М9). Существенные условия (ставка из торгов) — read-only
-- в шаблоне (FR-901); здесь — периоды аренды и защита от пересечений.

CREATE TABLE core.contracts (
  id              uuid                 PRIMARY KEY DEFAULT uuidv7(),
  tender_id       uuid                 REFERENCES core.tenders (id),
  lot_id          uuid                 REFERENCES core.lots (id),
  object_id       uuid                 NOT NULL REFERENCES core.objects (id),
  tenant_id       uuid                 NOT NULL REFERENCES core.users (id),
  protocol_id     uuid                 REFERENCES core.protocols (id),
  status          core.contract_status NOT NULL DEFAULT 'draft',
  monthly_rate    numeric(14,2)        NOT NULL CHECK (monthly_rate > 0),  -- ставка из торгов
  lease_period    tstzrange,           -- период найма; аренда начисляется с даты акта (FR-904)
  reg_number      text,                -- Журнал регистрации договоров (FR-905)
  registered_at   timestamptz,         -- дата регистрации = дата заключения (п. 126)
  signed_scan_key text,                -- скан подписанного экземпляра (без ЭЦП)
  created_at      timestamptz          NOT NULL DEFAULT now(),
  updated_at      timestamptz          NOT NULL DEFAULT now(),
  -- В signing/active период обязан быть задан — иначе EXCLUDE ниже не защищает
  CONSTRAINT active_needs_period
    CHECK (status NOT IN ('signing', 'active') OR lease_period IS NOT NULL),
  -- INV-DB-02: один объект не сдается на пересекающиеся периоды (FR-103)
  CONSTRAINT no_overlapping_lease
    EXCLUDE USING gist (object_id WITH =, lease_period WITH &&)
    WHERE (status IN ('signing', 'active'))
);

CREATE TRIGGER touch_updated_at BEFORE UPDATE ON core.contracts
  FOR EACH ROW EXECUTE FUNCTION core.touch_updated_at();

-- Чек-лист сверки документов (п. 113): INV-115 — договор не подписывается
-- наймодателем без завершенной сверки (блокирующий триггер — контур 2, FR-902)
CREATE TABLE core.contract_checklists (
  id          uuid        PRIMARY KEY DEFAULT uuidv7(),
  contract_id uuid        NOT NULL REFERENCES core.contracts (id) ON DELETE CASCADE,
  item_code   text        NOT NULL,  -- перечни п. 113 для физ/юрлиц
  checked_by  uuid        REFERENCES core.users (id),
  checked_at  timestamptz,
  UNIQUE (contract_id, item_code)
);

-- Акты приема-передачи и возврата (Прил. 7, 8; FR-904)
CREATE TABLE core.acts (
  id              uuid          PRIMARY KEY DEFAULT uuidv7(),
  contract_id     uuid          NOT NULL REFERENCES core.contracts (id),
  kind            core.act_kind NOT NULL,
  act_date        date          NOT NULL,
  pdf_key         text,
  signed_scan_key text,
  created_at      timestamptz   NOT NULL DEFAULT now(),
  UNIQUE (contract_id, kind)
);

-- FR-103: статус объекта вычисляется, а не хранится
CREATE VIEW core.object_statuses AS
SELECT
  o.id AS object_id,
  CASE
    WHEN EXISTS (
      SELECT 1 FROM core.contracts c
      WHERE c.object_id = o.id AND c.status = 'active'
    ) THEN 'leased'
    WHEN EXISTS (
      SELECT 1
      FROM core.lots l
      JOIN core.tenders t ON t.id = l.tender_id
      WHERE l.object_id = o.id
        AND t.status IN ('announced', 'accepting', 'qualification', 'trading', 'summed_up')
    ) THEN 'in_tender'
    ELSE 'free'
  END AS status
FROM core.objects o;
