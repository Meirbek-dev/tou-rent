-- Полный регламент торгов (М6, FR-604–605, п. 65, 70–71): очередность по
-- кругу, выбытие не повысившего, объявление первоначальных предложений
-- отсутствующих. Правила дублируют домен (`domain::turn`) — БД последний рубеж.

CREATE TYPE core.auction_participant_status AS ENUM ('active', 'passed', 'absent');

-- Круг торгов: допущенные заявки лота в порядке журнала регистрации.
-- Порядок — единственный законный (заявки нумеруются журналом, п. 37–39).
CREATE TABLE core.auction_participants (
  id             uuid                            PRIMARY KEY DEFAULT uuidv7(),
  auction_id     uuid                            NOT NULL REFERENCES core.auctions (id) ON DELETE CASCADE,
  application_id uuid                            NOT NULL REFERENCES core.applications (id),
  -- Место в очередности: seq журнала регистрации этой заявки
  turn_order     integer                         NOT NULL,
  status         core.auction_participant_status NOT NULL DEFAULT 'active',
  -- Первоначальное предложение (Прил. 9): объявляется при неявке (п. 70)
  initial_price  numeric(14,2)                   NOT NULL CHECK (initial_price > 0),
  changed_at     timestamptz                     NOT NULL DEFAULT now(),
  UNIQUE (auction_id, application_id)
);

CREATE INDEX auction_participants_turn_idx
  ON core.auction_participants (auction_id, turn_order);

CREATE TRIGGER audit_record AFTER INSERT OR UPDATE OR DELETE ON core.auction_participants
  FOR EACH ROW EXECUTE FUNCTION audit.record();

-- Чей сейчас ход (FR-604). NULL — круг не начат либо торги окончены.
ALTER TABLE core.auctions
  ADD COLUMN current_turn_application_id uuid REFERENCES core.applications (id);

COMMENT ON COLUMN core.auctions.current_turn_application_id IS
  'FR-604: участник, чей ход сейчас (п. 65); ставка вне очереди отклоняется';

-- Объявленное первоначальное предложение отсутствующего (п. 70): в ленте
-- оно есть, но шагу торгов не подчиняется — это не повышение, а оглашение.
ALTER TABLE core.bids
  ADD COLUMN announced boolean NOT NULL DEFAULT false;

COMMENT ON COLUMN core.bids.announced IS
  'FR-605: оглашенное первоначальное предложение (п. 70), а не ставка круга';

-- Правила ставки с учетом очередности и оглашений (FR-604, FR-605).
-- Замена функции T2: прежние проверки статуса, времени и шага сохранены.
CREATE OR REPLACE FUNCTION core.enforce_bid_rules() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  a           core.auctions%ROWTYPE;
  current_max numeric(14,2);
  circle      core.auction_participants%ROWTYPE;
BEGIN
  SELECT * INTO a FROM core.auctions WHERE id = NEW.auction_id FOR UPDATE;

  -- Оглашение первоначального предложения (п. 70) идет при старте торгов
  -- и правилам круга не подчиняется: это не ставка участника
  IF NEW.announced THEN
    IF a.status NOT IN ('scheduled', 'running') THEN
      RAISE EXCEPTION 'FR-605: огласить предложение можно только до конца торгов (статус %)', a.status
        USING ERRCODE = 'check_violation';
    END IF;
    NEW.placed_at := now();
    RETURN NEW;
  END IF;

  IF a.status <> 'running' THEN
    RAISE EXCEPTION 'ставка отклонена: аукцион не в статусе running (текущий: %)', a.status
      USING ERRCODE = 'check_violation';
  END IF;
  IF a.ends_at IS NOT NULL AND now() > a.ends_at THEN
    RAISE EXCEPTION 'INV-066: время торгов истекло (%)', a.ends_at
      USING ERRCODE = 'check_violation';
  END IF;

  -- Очередность по кругу (FR-604, п. 65): ходит тот, чья очередь
  SELECT * INTO circle FROM core.auction_participants
  WHERE auction_id = NEW.auction_id AND application_id = NEW.application_id;

  IF circle.id IS NOT NULL THEN
    IF circle.status = 'passed' THEN
      RAISE EXCEPTION 'FR-604: участник выбыл из торгов и больше не повышает (п. 65)'
        USING ERRCODE = 'check_violation';
    END IF;
    IF circle.status = 'absent' THEN
      RAISE EXCEPTION 'FR-605: участник не явился — объявлено его первоначальное предложение (п. 70)'
        USING ERRCODE = 'check_violation';
    END IF;
    IF a.current_turn_application_id IS NOT NULL
       AND a.current_turn_application_id <> NEW.application_id THEN
      RAISE EXCEPTION 'FR-604: сейчас ход другого участника (п. 65)'
        USING ERRCODE = 'check_violation';
    END IF;
  END IF;

  -- Оглашенные предложения (п. 70) ниже стартовой ставки: планка не может
  -- опуститься ниже старта, поэтому берется наибольшее из двух (INV-062–063)
  SELECT greatest(max(amount), a.starting_bid) INTO current_max
  FROM core.bids WHERE auction_id = NEW.auction_id;
  current_max := coalesce(current_max, a.starting_bid);

  IF NEW.amount < current_max + a.bid_step THEN
    RAISE EXCEPTION 'INV-063: ставка % ниже минимально допустимой % (максимум % + шаг %)',
      NEW.amount, current_max + a.bid_step, current_max, a.bid_step
      USING ERRCODE = 'check_violation';
  END IF;

  NEW.placed_at := now();  -- время сервера (NFR-03)
  RETURN NEW;
END $$;
