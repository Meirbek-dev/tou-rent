//! Онлайн-торги (М6, FR-601–606).
//!
//! Чистые правила комнаты лота: шаг торгов (п. 63), минимально допустимая
//! ставка, определение победителя и второго места (п. 69, 74). Времени здесь
//! нет - таймер (п. 66, 68) ведет сервер (FR-602), а порядок ставок задает
//! БД (`core.bids.seq`), поэтому итог считается по уже упорядоченной ленте.
//!
//! Инварианты закреплены триплетом «тип → триггер БД → тест»: INV-063 (ставка
//! ≥ максимум + шаг) дублируется в `core.enforce_bid_rules`, INV-062 (старт =
//! максимум первоначальных предложений допущенных) - в слое данных.

use rust_decimal::{Decimal, dec};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ids::ApplicationId;
use crate::money::Money;

/// Шаг торгов - 5 % от стартовой ставки (п. 63).
pub const BID_STEP_PERCENT: Decimal = dec!(5);

/// Длительность торгов по умолчанию - 60 минут от объявления старта (п. 66).
pub const DEFAULT_DURATION_MINUTES: i64 = 60;

/// Единственное продление - ровно 15 минут (п. 68, INV-066).
pub const EXTENSION_MINUTES: i64 = 15;

/// Шаг торгов: 5 % от стартовой ставки, округленные по FR-204 (п. 140–143),
/// но не меньше 1 тенге - иначе на копеечном старте шаг выродится в ноль
/// и `core.auctions.bid_step > 0` отклонит аукцион.
pub fn bid_step(starting_bid: Money) -> Money {
    let raw = Money::new(starting_bid.amount() * BID_STEP_PERCENT / dec!(100)).round_to_tenge();
    if raw.amount() < dec!(1) {
        Money::new(dec!(1))
    } else {
        raw
    }
}

/// Минимально допустимая ставка (INV-063): текущий максимум (а до первой
/// ставки - стартовая ставка) плюс шаг.
pub fn min_next_bid(starting_bid: Money, current_max: Option<Money>, step: Money) -> Money {
    let base = current_max.unwrap_or(starting_bid);
    Money::new(base.amount() + step.amount())
}

/// Отказ в ставке - тот же перечень причин, что и у триггера БД.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum BidRejected {
    #[error("ставка ниже минимально допустимой (INV-063)")]
    BelowMinimum,
}

/// Проверка ставки до похода в БД: быстрый отказ участнику с понятной суммой.
/// Последний рубеж - триггер `core.enforce_bid_rules` (он же сериализует гонки).
pub fn accepts(
    starting_bid: Money,
    current_max: Option<Money>,
    step: Money,
    amount: Money,
) -> Result<(), BidRejected> {
    if amount < min_next_bid(starting_bid, current_max, step) {
        return Err(BidRejected::BelowMinimum);
    }
    Ok(())
}

/// Ставка в ленте комнаты; порядок задан сервером (`core.bids.seq`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bid {
    pub application_id: ApplicationId,
    pub amount: Money,
}

/// Место в итоге торгов (FR-606).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    pub application_id: ApplicationId,
    pub amount: Money,
}

/// Итог торгов по лоту (FR-606, п. 69, 74): победитель и второе место.
/// Второе место - лучшая ставка другого участника; если торговался один,
/// второго места нет (при отсутствии ставок нет и победителя - п. 71
/// разбирает такой случай отдельной процедурой, контур 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Outcome {
    pub winner: Option<Placement>,
    pub runner_up: Option<Placement>,
}

/// Итог по ленте ставок в серверном порядке.
pub fn outcome(bids: &[Bid]) -> Outcome {
    let mut winner: Option<Placement> = None;
    let mut runner_up: Option<Placement> = None;

    for bid in bids {
        let placement = Placement {
            application_id: bid.application_id,
            amount: bid.amount,
        };
        match winner {
            // Победитель растет только вверх: лента монотонна (INV-063)
            Some(best) if bid.amount <= best.amount => {
                if best.application_id != bid.application_id
                    && runner_up.is_none_or(|second| bid.amount > second.amount)
                {
                    runner_up = Some(placement);
                }
            }
            Some(best) => {
                // Прежний лидер уступает первое место и претендует на второе
                if best.application_id != bid.application_id {
                    runner_up = Some(best);
                }
                winner = Some(placement);
            }
            None => winner = Some(placement),
        }
    }

    // Вытеснение: второе место не может принадлежать победителю
    if let (Some(best), Some(second)) = (winner, runner_up)
        && best.application_id == second.application_id
    {
        runner_up = None;
    }

    Outcome { winner, runner_up }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn money(value: &str) -> Money {
        Money::new(value.parse().unwrap())
    }

    fn app(tag: u128) -> ApplicationId {
        ApplicationId::new(Uuid::from_u128(tag))
    }

    fn bid(tag: u128, amount: &str) -> Bid {
        Bid {
            application_id: app(tag),
            amount: money(amount),
        }
    }

    #[test]
    fn step_is_five_percent_of_start() {
        // п. 63: 55 000 → 2 750
        assert_eq!(bid_step(money("55000")), money("2750"));
    }

    #[test]
    fn step_rounds_to_whole_tenge_by_fr204() {
        // 5 % от 40 009 = 2 000,45 → 2 000; от 40 011 = 2 000,55 → 2 001
        assert_eq!(bid_step(money("40009")), money("2000"));
        assert_eq!(bid_step(money("40011")), money("2001"));
    }

    #[test]
    fn step_never_degenerates_to_zero() {
        assert_eq!(bid_step(money("5")), money("1"));
    }

    #[test]
    fn first_bid_must_exceed_start_by_step() {
        let start = money("55000");
        let step = bid_step(start);
        assert_eq!(min_next_bid(start, None, step), money("57750"));
        assert_eq!(
            accepts(start, None, step, money("57749")),
            Err(BidRejected::BelowMinimum)
        );
        assert_eq!(accepts(start, None, step, money("57750")), Ok(()));
    }

    #[test]
    fn next_bid_counts_from_current_maximum() {
        let start = money("55000");
        let step = bid_step(start);
        assert_eq!(
            min_next_bid(start, Some(money("57750")), step),
            money("60500")
        );
    }

    #[test]
    fn outcome_without_bids_has_neither_winner_nor_runner_up() {
        assert_eq!(outcome(&[]), Outcome::default());
    }

    #[test]
    fn single_bidder_has_no_runner_up() {
        // Одна заявка перебивает сама себя - второго места нет (п. 74)
        let result = outcome(&[bid(1, "57750"), bid(1, "60500")]);
        assert_eq!(result.winner.unwrap().amount, money("60500"));
        assert!(result.runner_up.is_none());
    }

    #[test]
    fn runner_up_is_best_bid_of_another_participant() {
        // FR-606: победитель - 66 000 (заявка 1), второе место - 63 250 (заявка 2)
        let result = outcome(&[
            bid(1, "57750"),
            bid(2, "60500"),
            bid(1, "63250"),
            bid(2, "66000"),
            bid(1, "68750"),
        ]);
        let winner = result.winner.unwrap();
        let runner_up = result.runner_up.unwrap();
        assert_eq!(
            (winner.application_id, winner.amount),
            (app(1), money("68750"))
        );
        assert_eq!(
            (runner_up.application_id, runner_up.amount),
            (app(2), money("66000"))
        );
    }

    /// FR-601 (property): при любой последовательности принятых ставок
    /// максимум монотонен, победитель единственен и это последняя ставка,
    /// второе место - другой участник с меньшей суммой.
    #[test]
    fn accepted_sequences_keep_maximum_monotonic_and_winner_unique() {
        let mut rng = Xorshift::new(0x2545_F491_4F6C_DD1D_u64);
        for case in 0..2_000_u32 {
            let start = money(&format!("{}", 1_000 + u64::from(case) * 37));
            let step = bid_step(start);
            let participants = 1 + rng.next_below(4); // 1..4 заявок
            let mut ledger: Vec<Bid> = Vec::new();
            let mut current_max: Option<Money> = None;

            for _ in 0..rng.next_below(12) {
                let application_id = app(u128::from(rng.next_below(participants)));
                let minimum = min_next_bid(start, current_max, step);
                // Половина попыток - заведомо низкие: их обязан отбить `accepts`
                let amount = if rng.next_below(2) == 0 {
                    Money::new(minimum.amount() - Decimal::from(rng.next_below(500)))
                } else {
                    Money::new(minimum.amount() + Decimal::from(rng.next_below(5_000)))
                };

                match accepts(start, current_max, step, amount) {
                    Ok(()) => {
                        assert!(
                            amount >= minimum,
                            "принята ставка ниже минимума: {amount:?} < {minimum:?}"
                        );
                        assert!(
                            current_max.is_none_or(|max| amount > max),
                            "максимум обязан расти строго"
                        );
                        current_max = Some(amount);
                        ledger.push(Bid {
                            application_id,
                            amount,
                        });
                    }
                    Err(BidRejected::BelowMinimum) => assert!(amount < minimum),
                }
            }

            let result = outcome(&ledger);
            match ledger.last() {
                None => assert_eq!(result, Outcome::default()),
                Some(last) => {
                    let winner = result.winner.expect("победитель при непустой ленте");
                    assert_eq!(
                        (winner.application_id, winner.amount),
                        (last.application_id, last.amount),
                        "победитель - последняя (она же максимальная) ставка"
                    );
                    if let Some(runner_up) = result.runner_up {
                        assert_ne!(
                            runner_up.application_id, winner.application_id,
                            "второе место - другой участник (п. 74)"
                        );
                        assert!(runner_up.amount < winner.amount);
                        let best_other = ledger
                            .iter()
                            .filter(|b| b.application_id != winner.application_id)
                            .map(|b| b.amount)
                            .max();
                        assert_eq!(Some(runner_up.amount), best_other);
                    } else {
                        assert!(
                            ledger
                                .iter()
                                .all(|b| b.application_id == winner.application_id),
                            "второго места нет только когда торговался один участник"
                        );
                    }
                }
            }
        }
    }

    /// Детерминированный генератор для property-теста: воспроизводимость
    /// важнее качества распределения, внешняя зависимость не нужна.
    struct Xorshift(u64);

    impl Xorshift {
        fn new(seed: u64) -> Self {
            Self(seed | 1)
        }

        fn next_u64(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }

        fn next_below(&mut self, bound: u64) -> u64 {
            if bound == 0 {
                0
            } else {
                self.next_u64() % bound
            }
        }
    }
}
