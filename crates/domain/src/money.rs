use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};

/// Денежная сумма в тенге (KZT); в БД - `numeric(14,2)` (ТЗ § 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Money(Decimal);

impl Money {
    /// Нормализует сумму до тиынов (2 знака).
    pub fn new(amount: Decimal) -> Self {
        Self(amount.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero))
    }

    /// Округление до целых тенге по FR-204 (п. 140–143 Правил):
    /// тиыны < 50 отбрасываются, ≥ 50 - округляются до 1 тенге.
    pub fn round_to_tenge(self) -> Self {
        Self(
            self.0
                .round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero),
        )
    }

    pub fn amount(self) -> Decimal {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    #[test]
    fn tiyn_below_fifty_rounds_down() {
        // FR-204
        assert_eq!(
            Money::new(dec("1234.49")).round_to_tenge().amount(),
            dec("1234")
        );
    }

    #[test]
    fn tiyn_at_fifty_rounds_up() {
        // FR-204
        assert_eq!(
            Money::new(dec("1234.50")).round_to_tenge().amount(),
            dec("1235")
        );
    }

    #[test]
    fn new_normalizes_to_two_decimal_places() {
        assert_eq!(Money::new(dec("10.005")).amount(), dec("10.01"));
    }
}
