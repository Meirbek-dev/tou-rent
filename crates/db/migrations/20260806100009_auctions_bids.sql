-- Онлайн-торги (М6). Сервер — единственный источник времени и порядка ставок
-- (FR-601, NFR-03); БД — последний рубеж правил шага и таймера.

CREATE TABLE core.auctions (
  id                       uuid                PRIMARY KEY DEFAULT uuidv7(),
  lot_id                   uuid                NOT NULL UNIQUE REFERENCES core.lots (id),
  status                   core.auction_status NOT NULL DEFAULT 'scheduled',
  -- INV-062: старт = максимум первоначальных предложений допущенных (п. 62)
  starting_bid             numeric(14,2)       NOT NULL CHECK (starting_bid > 0),
  -- INV-063: шаг = 5 % от стартовой ставки, фиксируется на старте (п. 63);
  -- точное значение (округление FR-204) вычисляет домен
  bid_step                 numeric(14,2)       NOT NULL CHECK (bid_step > 0),
  started_at               timestamptz,
  ends_at                  timestamptz,        -- server-authoritative таймер (FR-602)
  extended_once            boolean             NOT NULL DEFAULT false,  -- INV-066: продление <= 1 раза
  finished_at              timestamptz,
  finished_early           boolean             NOT NULL DEFAULT false,  -- досрочно при общем согласии (п. 67)
  winner_application_id    uuid                REFERENCES core.applications (id),
  winner_amount            numeric(14,2),
  runner_up_application_id uuid                REFERENCES core.applications (id),  -- № 2 (FR-606, п. 74)
  runner_up_amount         numeric(14,2),
  CHECK (started_at IS NULL OR ends_at IS NULL OR started_at < ends_at)
);

-- INV-066: продление таймера — один раз на 15 минут решением председателя (п. 66, 68)
CREATE FUNCTION core.enforce_auction_extension() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF OLD.status = 'running' AND NEW.status = 'running'
     AND OLD.ends_at IS NOT NULL AND NEW.ends_at > OLD.ends_at THEN
    IF OLD.extended_once THEN
      RAISE EXCEPTION 'INV-066: таймер уже продлевался — повторное продление запрещено (п. 68)'
        USING ERRCODE = 'check_violation';
    END IF;
    NEW.extended_once := true;
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER enforce_auction_extension BEFORE UPDATE ON core.auctions
  FOR EACH ROW EXECUTE FUNCTION core.enforce_auction_extension();

-- Ставки: append-only, id генерирует клиент (uuid v7) — идемпотентность реконнекта
-- по bid_id (NFR-05): повторная вставка того же id — ON CONFLICT DO NOTHING на слое API.
CREATE TABLE core.bids (
  id             uuid          PRIMARY KEY,
  auction_id     uuid          NOT NULL REFERENCES core.auctions (id),
  application_id uuid          NOT NULL REFERENCES core.applications (id),
  amount         numeric(14,2) NOT NULL CHECK (amount > 0),
  seq            bigint        GENERATED ALWAYS AS IDENTITY,  -- тотальный порядок ставок
  placed_at      timestamptz   NOT NULL DEFAULT now()
);

CREATE INDEX bids_auction_idx ON core.bids (auction_id, amount DESC);

-- INV-063: ставка принимается, если >= текущий максимум + шаг; торги должны идти,
-- время не истекло. FOR UPDATE на аукционе сериализует конкурентные ставки.
CREATE FUNCTION core.enforce_bid_rules() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  a core.auctions%ROWTYPE;
  current_max numeric(14,2);
BEGIN
  SELECT * INTO a FROM core.auctions WHERE id = NEW.auction_id FOR UPDATE;

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
    RAISE EXCEPTION 'INV-063: ставка % ниже минимально допустимой % (максимум % + шаг %)',
      NEW.amount, coalesce(current_max, a.starting_bid) + a.bid_step,
      coalesce(current_max, a.starting_bid), a.bid_step
      USING ERRCODE = 'check_violation';
  END IF;

  NEW.placed_at := now();  -- время сервера (NFR-03)
  RETURN NEW;
END $$;

CREATE TRIGGER enforce_bid_rules BEFORE INSERT ON core.bids
  FOR EACH ROW EXECUTE FUNCTION core.enforce_bid_rules();

CREATE TRIGGER bids_append_only BEFORE UPDATE OR DELETE ON core.bids
  FOR EACH ROW EXECUTE FUNCTION core.forbid_mutation('INV-063');
