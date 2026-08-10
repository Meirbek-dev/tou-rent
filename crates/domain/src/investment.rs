//! Инвестиционные договоры особого порядка (М12, FR-1204, п. 91–94).
//!
//! Величины здесь - из ТЗ, а не из головы: семь лет предельного срока,
//! три года однократного продления, пороги 30 и 100 млн ₸ названы в FR-1204
//! и в п. 93–94 Правил. Все, что этими величинами не описано (компенсация
//! за неисполнение и повышенная плата, п. 94), в домене не заводится, пока
//! Правила не дадут формулу: см. Q-014.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// INV-094 (п. 94): предельный срок инвестиционного договора - семь лет.
pub const MAX_TERM_MONTHS: i32 = 7 * 12;

/// Однократное продление (п. 93) - три года.
pub const EXTENSION_MONTHS: i32 = 3 * 12;

/// Порог объема инвестиций для продления на три года (п. 93), тенге.
pub const EXTENSION_THRESHOLD: i64 = 30_000_000;

/// Порог объема инвестиций для пролонгации решением Правления (п. 93), тенге.
pub const PROLONGATION_THRESHOLD: i64 = 100_000_000;

/// Обязательное приложение инвестиционного проекта (п. 91): закрытый
/// перечень, «прочих документов» в нем не предусмотрено.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Attachment {
    /// Смета инвестиционного проекта
    Estimate,
    /// График выполнения работ
    Schedule,
    /// Заключение оценщика
    Appraisal,
    /// Гарантия исполнения обязательств
    Guarantee,
}

impl Attachment {
    pub const ALL: [Attachment; 4] = [
        Attachment::Estimate,
        Attachment::Schedule,
        Attachment::Appraisal,
        Attachment::Guarantee,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Attachment::Estimate => "estimate",
            Attachment::Schedule => "schedule",
            Attachment::Appraisal => "appraisal",
            Attachment::Guarantee => "guarantee",
        }
    }

    /// Пункт Правил, которым приложение предусмотрено
    pub fn rule_ref(self) -> &'static str {
        "п. 91"
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестное приложение инвестиционного проекта: {0}")]
pub struct UnknownAttachment(pub String);

impl std::str::FromStr for Attachment {
    type Err = UnknownAttachment;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Attachment::ALL
            .into_iter()
            .find(|attachment| attachment.as_str() == s)
            .ok_or_else(|| UnknownAttachment(s.to_owned()))
    }
}

/// Срок инвестиционного договора (INV-094, п. 94): значение этого типа
/// не превышает семи лет по построению.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Term(i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TermError {
    #[error("срок инвестиционного договора задается в месяцах и больше нуля (п. 94)")]
    NotPositive,
    #[error("INV-094: срок инвестиционного договора не превышает семи лет (п. 94)")]
    TooLong,
}

impl Term {
    pub fn months(self) -> i32 {
        self.0
    }

    /// Срок из месяцев: за границей семи лет значение не создается.
    pub fn new(months: i32) -> Result<Self, TermError> {
        if months <= 0 {
            return Err(TermError::NotPositive);
        }
        if months > MAX_TERM_MONTHS {
            return Err(TermError::TooLong);
        }
        Ok(Self(months))
    }
}

/// Исполнение обязательств по договору (п. 92): сколько инвестиций принято
/// комиссией из обещанного объема.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    /// Объем инвестиций из решения Правления (существенное условие, FR-901)
    pub promised: Decimal,
    /// Принято актами приемки (п. 92)
    pub accepted: Decimal,
}

impl Progress {
    /// Обязательства исполнены полностью (п. 93: продление - при 100 %)
    pub fn is_complete(self) -> bool {
        self.accepted >= self.promised
    }
}

/// Способ продлить договор (п. 93).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Extension {
    /// Однократное продление на три года при объеме от 30 млн ₸
    ThreeYears,
    /// Пролонгация на аналогичный период решением Правления (от 100 млн ₸)
    Prolongation,
}

impl Extension {
    pub const ALL: [Extension; 2] = [Extension::ThreeYears, Extension::Prolongation];

    pub fn as_str(self) -> &'static str {
        match self {
            Extension::ThreeYears => "three_years",
            Extension::Prolongation => "prolongation",
        }
    }

    /// Порог объема инвестиций (п. 93), тенге
    pub fn threshold(self) -> Decimal {
        match self {
            Extension::ThreeYears => Decimal::from(EXTENSION_THRESHOLD),
            Extension::Prolongation => Decimal::from(PROLONGATION_THRESHOLD),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестный способ продления договора: {0}")]
pub struct UnknownExtension(pub String);

impl std::str::FromStr for Extension {
    type Err = UnknownExtension;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Extension::ALL
            .into_iter()
            .find(|extension| extension.as_str() == s)
            .ok_or_else(|| UnknownExtension(s.to_owned()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ExtensionError {
    #[error("FR-1204: обязательства исполнены не полностью (п. 93)")]
    NotComplete,
    #[error("FR-1204: объем инвестиций ниже порога продления (п. 93)")]
    BelowThreshold,
    #[error("FR-1204: продление договора однократно (п. 93)")]
    AlreadyExtended,
}

/// Состояние продлений договора (п. 93).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Extensions {
    pub extended: bool,
    pub prolonged: bool,
}

impl Extensions {
    /// Можно ли продлить договор выбранным способом (п. 93): исполнение
    /// полное, объем выше порога, повторов нет.
    pub fn allow(self, extension: Extension, progress: Progress) -> Result<(), ExtensionError> {
        let already = match extension {
            Extension::ThreeYears => self.extended,
            Extension::Prolongation => self.prolonged,
        };
        if already {
            return Err(ExtensionError::AlreadyExtended);
        }
        if !progress.is_complete() {
            return Err(ExtensionError::NotComplete);
        }
        if progress.promised < extension.threshold() {
            return Err(ExtensionError::BelowThreshold);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(promised: i64, accepted: i64) -> Progress {
        Progress {
            promised: Decimal::from(promised),
            accepted: Decimal::from(accepted),
        }
    }

    /// INV-094 (п. 94): срок больше семи лет значением не станет.
    #[test]
    fn inv094_term_is_capped_at_seven_years() {
        assert_eq!(Term::new(84).map(Term::months), Ok(84));
        assert_eq!(Term::new(85), Err(TermError::TooLong));
        assert_eq!(Term::new(0), Err(TermError::NotPositive));
        assert_eq!(Term::new(-12), Err(TermError::NotPositive));
        assert_eq!(MAX_TERM_MONTHS, 84, "семь лет из п. 94");
    }

    /// Продление на три года - при полном исполнении и объеме от 30 млн ₸.
    #[test]
    fn three_year_extension_needs_full_performance() {
        let done = progress(30_000_000, 30_000_000);
        assert_eq!(
            Extensions::default().allow(Extension::ThreeYears, done),
            Ok(())
        );

        let partial = progress(30_000_000, 29_999_999);
        assert_eq!(
            Extensions::default().allow(Extension::ThreeYears, partial),
            Err(ExtensionError::NotComplete)
        );

        let small = progress(29_000_000, 29_000_000);
        assert_eq!(
            Extensions::default().allow(Extension::ThreeYears, small),
            Err(ExtensionError::BelowThreshold)
        );
    }

    /// Продление однократно (п. 93): второй раз - отказ.
    #[test]
    fn extension_happens_once() {
        let done = progress(50_000_000, 50_000_000);
        let extended = Extensions {
            extended: true,
            prolonged: false,
        };
        assert_eq!(
            extended.allow(Extension::ThreeYears, done),
            Err(ExtensionError::AlreadyExtended)
        );
        // Пролонгация - другой способ, но своего порога она требует
        assert_eq!(
            extended.allow(Extension::Prolongation, done),
            Err(ExtensionError::BelowThreshold)
        );
    }

    /// Пролонгация - от 100 млн ₸ (п. 93).
    #[test]
    fn prolongation_needs_a_hundred_million() {
        let big = progress(100_000_000, 120_000_000);
        assert_eq!(
            Extensions::default().allow(Extension::Prolongation, big),
            Ok(())
        );
        assert!(big.is_complete(), "принято больше обещанного - исполнено");

        let modest = progress(99_000_000, 99_000_000);
        assert_eq!(
            Extensions::default().allow(Extension::Prolongation, modest),
            Err(ExtensionError::BelowThreshold)
        );
    }

    #[test]
    fn wire_names_round_trip() {
        for attachment in Attachment::ALL {
            assert_eq!(attachment.as_str().parse::<Attachment>(), Ok(attachment));
            assert_eq!(attachment.rule_ref(), "п. 91");
        }
        for extension in Extension::ALL {
            assert_eq!(extension.as_str().parse::<Extension>(), Ok(extension));
        }
        assert_eq!(Attachment::ALL.len(), 4, "четыре приложения п. 91");
    }
}
