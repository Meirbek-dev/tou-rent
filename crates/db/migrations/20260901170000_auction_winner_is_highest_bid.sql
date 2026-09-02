-- Победителем торгов объявляется только наибольшая ставка (FR-606, INV-063).
--
-- `core.enforce_auction_results` проверяла лишь принадлежность пары
-- (заявка, сумма) этому аукциону, но не максимальность. Круг 2 гаунтлета
-- показал последствие в одноразовой БД: при ставках 168 000 и 176 000
--
--   UPDATE core.auctions SET status='finished',
--     winner_application_id = <автор 168 000>, winner_amount = 168000.00;
--
-- проходил без единого возражения - проигравший объявлялся победителем.
-- Домен считает иначе (`domain::auction::outcome`: лента монотонна, победа
-- у наибольшей суммы), но это был последний рубеж только в Rust; регламент
-- А.5 требует закреплять инвариант на самом нижнем достижимом уровне.
--
-- Второе место — наибольшая сумма среди прочих участников: это то же
-- правило вытеснения, что и в домене (прежний лидер уступает первое место
-- и претендует на второе).

CREATE OR REPLACE FUNCTION core.enforce_auction_results() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  top       numeric(14,2);
  best_rest numeric(14,2);
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

  IF NEW.winner_application_id IS NOT NULL THEN
    SELECT max(b.amount) INTO top FROM core.bids b WHERE b.auction_id = NEW.id;
    IF NEW.winner_amount IS DISTINCT FROM top THEN
      RAISE EXCEPTION
        'FR-606: победителем объявляется наибольшая ставка (% при максимуме %)',
        NEW.winner_amount, top
        USING ERRCODE = 'check_violation';
    END IF;
  END IF;

  IF NEW.runner_up_application_id IS NOT NULL THEN
    IF NEW.runner_up_application_id = NEW.winner_application_id THEN
      RAISE EXCEPTION 'FR-606: второе место не может принадлежать победителю'
        USING ERRCODE = 'check_violation';
    END IF;
    SELECT max(b.amount) INTO best_rest
    FROM core.bids b
    WHERE b.auction_id = NEW.id
      AND b.application_id IS DISTINCT FROM NEW.winner_application_id;
    IF NEW.runner_up_amount IS DISTINCT FROM best_rest THEN
      RAISE EXCEPTION
        'FR-606: второе место - наибольшая ставка среди прочих участников (% при максимуме %)',
        NEW.runner_up_amount, best_rest
        USING ERRCODE = 'check_violation';
    END IF;
  END IF;

  RETURN NEW;
END $$;
