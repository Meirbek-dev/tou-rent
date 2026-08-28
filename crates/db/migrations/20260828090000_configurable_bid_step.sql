-- Q-019: секретарь выбирает процент шага до открытия комнаты; минимум 5 %.
-- Денежный шаг остается фиксированным снимком, чтобы последующие ставки не
-- зависели от изменения настроек или правил округления.
ALTER TABLE core.auctions
  ADD COLUMN bid_step_percent numeric NOT NULL DEFAULT 5,
  ADD CONSTRAINT auctions_bid_step_percent_minimum
    CHECK (bid_step_percent >= 5),
  ADD CONSTRAINT auctions_bid_step_amount_minimum
    CHECK (bid_step >= GREATEST(round(starting_bid * 0.05), 1));

COMMENT ON COLUMN core.auctions.bid_step_percent IS
  'Процент от стартовой ставки, выбранный секретарем до открытия комнаты (Q-019)';
