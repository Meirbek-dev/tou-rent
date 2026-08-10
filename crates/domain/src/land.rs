//! Земельные участки (М18: FR-1801, INV-105, п. 104–107).
//!
//! Порядок раздела 14: университет публикует характеристики участка
//! (под общежития и иное, п. 104) → инвестор подает заявку с проектом
//! и объемом инвестиций (п. 105) → Правление решает (п. 106) → заключается
//! договор с особыми условиями (п. 107).
//!
//! INV-105: особые условия - запрет залога самого участка и возводимых на нем
//! зданий - закрытый перечень [`Covenant`]. Договор без полного комплекта
//! не подписывается, а внесенное условие не снимается: то же правило стоит
//! триггером в БД (`check_land_covenants`), как и у комплекта приложений
//! инвестиционного проекта (INV-091).
//!
//! Раздел 14 Правил агенту недоступен (Q-016): назначения участков
//! и формулировки условий взяты из ТЗ FR-1801 и уточняются данными
//! справочников без правки кода.

use serde::{Deserialize, Serialize};

/// Назначение участка (п. 104): под общежития и иное.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LandDesignation {
    Dormitory,
    Other,
}

impl LandDesignation {
    pub const ALL: [LandDesignation; 2] = [LandDesignation::Dormitory, LandDesignation::Other];

    pub fn as_str(self) -> &'static str {
        match self {
            LandDesignation::Dormitory => "dormitory",
            LandDesignation::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестное назначение участка: {0}")]
pub struct UnknownDesignation(pub String);

impl std::str::FromStr for LandDesignation {
    type Err = UnknownDesignation;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        LandDesignation::ALL
            .into_iter()
            .find(|value| value.as_str() == s)
            .ok_or_else(|| UnknownDesignation(s.to_owned()))
    }
}

/// INV-105 (п. 107): особое условие договора на участок. Перечень закрыт -
/// условие, которого нет в Правилах, договором не появляется, а названные
/// Правилами обязательны для каждого договора раздела 14.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Covenant {
    /// Запрет залога земельного участка (п. 107)
    NoPledgePlot,
    /// Запрет залога возводимых на участке зданий (п. 107)
    NoPledgeBuildings,
}

impl Covenant {
    pub const ALL: [Covenant; 2] = [Covenant::NoPledgePlot, Covenant::NoPledgeBuildings];

    pub fn as_str(self) -> &'static str {
        match self {
            Covenant::NoPledgePlot => "no_pledge_plot",
            Covenant::NoPledgeBuildings => "no_pledge_buildings",
        }
    }

    /// Формулировка условия (ru - делопроизводство, NFR-01).
    pub fn title_ru(self) -> &'static str {
        match self {
            Covenant::NoPledgePlot => "запрет залога земельного участка",
            Covenant::NoPledgeBuildings => "запрет залога возводимых на участке зданий",
        }
    }

    pub fn rule_ref(self) -> &'static str {
        "п. 107"
    }

    /// Условия, которых договору не хватает (INV-105).
    pub fn missing(present: &[Covenant]) -> Vec<Covenant> {
        Covenant::ALL
            .into_iter()
            .filter(|covenant| !present.contains(covenant))
            .collect()
    }

    /// INV-105: договор на участок подписывается только с полным комплектом
    /// особых условий (п. 107).
    pub fn check_complete(present: &[Covenant]) -> Result<(), CovenantError> {
        let missing = Covenant::missing(present);
        if missing.is_empty() {
            Ok(())
        } else {
            Err(CovenantError::Missing(missing))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CovenantError {
    #[error("INV-105: в договоре не закреплены особые условия участка (п. 107): {}",
        .0.iter().map(|c| c.title_ru()).collect::<Vec<_>>().join(", "))]
    Missing(Vec<Covenant>),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестное особое условие: {0}")]
pub struct UnknownCovenant(pub String);

impl std::str::FromStr for Covenant {
    type Err = UnknownCovenant;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Covenant::ALL
            .into_iter()
            .find(|covenant| covenant.as_str() == s)
            .ok_or_else(|| UnknownCovenant(s.to_owned()))
    }
}

/// Состояние заявки инвестора (п. 105–106). Решение и отзыв окончательны:
/// заявка - юридический факт, а не черновик.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LandApplicationStatus {
    Submitted,
    Granted,
    Refused,
    Withdrawn,
}

impl LandApplicationStatus {
    pub const ALL: [LandApplicationStatus; 4] = [
        LandApplicationStatus::Submitted,
        LandApplicationStatus::Granted,
        LandApplicationStatus::Refused,
        LandApplicationStatus::Withdrawn,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            LandApplicationStatus::Submitted => "submitted",
            LandApplicationStatus::Granted => "granted",
            LandApplicationStatus::Refused => "refused",
            LandApplicationStatus::Withdrawn => "withdrawn",
        }
    }

    /// Рассматривается ли заявка сейчас (п. 105).
    pub fn is_open(self) -> bool {
        matches!(self, LandApplicationStatus::Submitted)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестное состояние заявки на участок: {0}")]
pub struct UnknownLandStatus(pub String);

impl std::str::FromStr for LandApplicationStatus {
    type Err = UnknownLandStatus;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        LandApplicationStatus::ALL
            .into_iter()
            .find(|status| status.as_str() == s)
            .ok_or_else(|| UnknownLandStatus(s.to_owned()))
    }
}

/// Решение Правления по заявке на участок (п. 106).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LandDecision {
    Grant,
    Refuse,
}

impl LandDecision {
    pub const ALL: [LandDecision; 2] = [LandDecision::Grant, LandDecision::Refuse];

    pub fn as_str(self) -> &'static str {
        match self {
            LandDecision::Grant => "grant",
            LandDecision::Refuse => "refuse",
        }
    }

    pub fn title_ru(self) -> &'static str {
        match self {
            LandDecision::Grant => "предоставить участок",
            LandDecision::Refuse => "отказать",
        }
    }

    /// Состояние, в которое решение переводит заявку (п. 106).
    pub fn outcome(self) -> LandApplicationStatus {
        match self {
            LandDecision::Grant => LandApplicationStatus::Granted,
            LandDecision::Refuse => LandApplicationStatus::Refused,
        }
    }

    /// Решение принимается по заявке, которая рассматривается (п. 105–106).
    pub fn take(self, status: LandApplicationStatus) -> Result<LandApplicationStatus, LandError> {
        if !status.is_open() {
            return Err(LandError::NotOpen(status));
        }
        Ok(self.outcome())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LandError {
    #[error("заявка на участок уже не рассматривается (состояние {:?}), п. 105–106", .0)]
    NotOpen(LandApplicationStatus),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестное решение по заявке на участок: {0}")]
pub struct UnknownLandDecision(pub String);

impl std::str::FromStr for LandDecision {
    type Err = UnknownLandDecision;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        LandDecision::ALL
            .into_iter()
            .find(|decision| decision.as_str() == s)
            .ok_or_else(|| UnknownLandDecision(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inv105_contract_needs_every_covenant() {
        // п. 107: залог запрещен и для участка, и для возводимых зданий -
        // комплект неполон, пока не закреплены оба условия
        assert_eq!(
            Covenant::check_complete(&[]),
            Err(CovenantError::Missing(Covenant::ALL.to_vec()))
        );
        assert_eq!(
            Covenant::check_complete(&[Covenant::NoPledgePlot]),
            Err(CovenantError::Missing(vec![Covenant::NoPledgeBuildings]))
        );
        assert_eq!(Covenant::check_complete(&Covenant::ALL), Ok(()));
        assert!(Covenant::missing(&Covenant::ALL).is_empty());
    }

    #[test]
    fn covenants_have_stable_names_and_cite_the_rules() {
        let mut names = std::collections::BTreeSet::new();
        for covenant in Covenant::ALL {
            assert_eq!(covenant.as_str().parse::<Covenant>(), Ok(covenant));
            assert!(names.insert(covenant.as_str()));
            assert!(!covenant.title_ru().is_empty());
            assert_eq!(covenant.rule_ref(), "п. 107");
        }
        assert_eq!(
            "no_pledge_anything".parse::<Covenant>(),
            Err(UnknownCovenant("no_pledge_anything".to_owned()))
        );
    }

    #[test]
    fn decision_is_taken_on_an_open_application() {
        // п. 106: решают по поданной заявке, повторно - не решают
        assert_eq!(
            LandDecision::Grant.take(LandApplicationStatus::Submitted),
            Ok(LandApplicationStatus::Granted)
        );
        assert_eq!(
            LandDecision::Refuse.take(LandApplicationStatus::Submitted),
            Ok(LandApplicationStatus::Refused)
        );
        assert_eq!(
            LandDecision::Grant.take(LandApplicationStatus::Granted),
            Err(LandError::NotOpen(LandApplicationStatus::Granted))
        );
        assert_eq!(
            LandDecision::Grant.take(LandApplicationStatus::Withdrawn),
            Err(LandError::NotOpen(LandApplicationStatus::Withdrawn))
        );
    }

    #[test]
    fn wire_names_round_trip() {
        for status in LandApplicationStatus::ALL {
            assert_eq!(status.as_str().parse::<LandApplicationStatus>(), Ok(status));
        }
        for decision in LandDecision::ALL {
            assert_eq!(decision.as_str().parse::<LandDecision>(), Ok(decision));
            assert!(!decision.title_ru().is_empty());
        }
        for designation in LandDesignation::ALL {
            assert_eq!(
                designation.as_str().parse::<LandDesignation>(),
                Ok(designation)
            );
        }
    }

    #[test]
    fn only_a_submitted_application_is_open() {
        assert!(LandApplicationStatus::Submitted.is_open());
        for status in [
            LandApplicationStatus::Granted,
            LandApplicationStatus::Refused,
            LandApplicationStatus::Withdrawn,
        ] {
            assert!(!status.is_open(), "{status:?} закрыта для решения");
        }
    }
}
