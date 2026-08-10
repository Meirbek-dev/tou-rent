//! Несостоявшийся тендер (М8, FR-801–802, п. 81–83).
//!
//! Основания признания - закрытый перечень: «не состоялся» нельзя объявить
//! по усмотрению, для этого должен наступить один из четырех случаев п. 81.
//! Следствие тоже выводится из фактов, а не выбирается: одна соответствующая
//! заявка - договор из одного источника по решению комиссии, два подряд
//! несостоявшихся - вопрос Правлению, иначе повторный тендер (п. 82–83).

use serde::{Deserialize, Serialize};

/// Основание признания тендера несостоявшимся (п. 81).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureGround {
    /// Не подано ни одной заявки
    NoApplications,
    /// Подана единственная заявка
    SingleApplication,
    /// К торгам допущено менее двух участников
    FewerThanTwoAdmitted,
    /// От подписания договора уклонились и победитель, и участник № 2
    WinnersEvaded,
}

impl FailureGround {
    pub const ALL: [FailureGround; 4] = [
        FailureGround::NoApplications,
        FailureGround::SingleApplication,
        FailureGround::FewerThanTwoAdmitted,
        FailureGround::WinnersEvaded,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            FailureGround::NoApplications => "no_applications",
            FailureGround::SingleApplication => "single_application",
            FailureGround::FewerThanTwoAdmitted => "fewer_than_two_admitted",
            FailureGround::WinnersEvaded => "winners_evaded",
        }
    }

    /// Подпункт п. 81 - идет в протокол и в справочник оснований.
    /// TODO-ENGINEER: нумерация подпунктов сверяется по Правилам (Q-004).
    pub fn rule_ref(self) -> &'static str {
        match self {
            FailureGround::NoApplications => "п. 81.1",
            FailureGround::SingleApplication => "п. 81.2",
            FailureGround::FewerThanTwoAdmitted => "п. 81.3",
            FailureGround::WinnersEvaded => "п. 81.4",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестное основание несостоявшегося тендера: {0}")]
pub struct UnknownGround(pub String);

impl std::str::FromStr for FailureGround {
    type Err = UnknownGround;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        FailureGround::ALL
            .into_iter()
            .find(|ground| ground.as_str() == s)
            .ok_or_else(|| UnknownGround(s.to_owned()))
    }
}

/// Факты тендера, из которых выводится основание (п. 81).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Facts {
    /// Заявки, поданные и не отозванные к дедлайну
    pub applications: usize,
    /// Допущенные по итогам первого этапа
    pub admitted: usize,
    /// Срок приема заявок истек: до дедлайна судить о числе заявок рано
    pub deadline_passed: bool,
    /// Вскрытие состоялось: до него о допуске судить рано
    pub opened: bool,
    /// Победитель и участник № 2 уклонились от подписания (п. 116–118)
    pub winners_evaded: bool,
}

impl Facts {
    /// Основание, если оно наступило. Порядок проверок - от более раннего
    /// события к более позднему: тендер без заявок не может «недопустить»
    /// участников, а уклонение возможно только после итогов.
    pub fn ground(&self) -> Option<FailureGround> {
        if self.winners_evaded {
            return Some(FailureGround::WinnersEvaded);
        }
        match self.applications {
            // Пока прием идет, заявки еще могут поступить (п. 36–39)
            0 if self.deadline_passed => Some(FailureGround::NoApplications),
            1 if self.deadline_passed => Some(FailureGround::SingleApplication),
            _ if self.opened && self.admitted < 2 => Some(FailureGround::FewerThanTwoAdmitted),
            _ => None,
        }
    }
}

/// Следствие несостоявшегося тендера (п. 82–83).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Consequence {
    /// Повторный тендер (общий случай, п. 82)
    Repeat,
    /// Договор из одного источника по решению комиссии: подана
    /// единственная соответствующая требованиям заявка (п. 82)
    SingleSource,
    /// После двух несостоявшихся подряд вопрос передается Правлению (п. 83)
    BoardReferral,
}

impl Consequence {
    pub const ALL: [Consequence; 3] = [
        Consequence::Repeat,
        Consequence::SingleSource,
        Consequence::BoardReferral,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Consequence::Repeat => "repeat",
            Consequence::SingleSource => "single_source",
            Consequence::BoardReferral => "board_referral",
        }
    }

    /// Следствие по фактам: два несостоявшихся подряд перевешивают все
    /// остальное (п. 83), единственная допущенная заявка открывает договор
    /// из одного источника (п. 82), иначе - повторный тендер.
    pub fn of(ground: FailureGround, facts: Facts, previous_failures: usize) -> Self {
        if previous_failures >= 1 {
            return Consequence::BoardReferral;
        }
        match ground {
            FailureGround::SingleApplication | FailureGround::FewerThanTwoAdmitted
                if facts.admitted == 1 =>
            {
                Consequence::SingleSource
            }
            _ => Consequence::Repeat,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестное следствие несостоявшегося тендера: {0}")]
pub struct UnknownConsequence(pub String);

impl std::str::FromStr for Consequence {
    type Err = UnknownConsequence;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Consequence::ALL
            .into_iter()
            .find(|value| value.as_str() == s)
            .ok_or_else(|| UnknownConsequence(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_names_round_trip() {
        for ground in FailureGround::ALL {
            assert_eq!(ground.as_str().parse::<FailureGround>(), Ok(ground));
            assert!(ground.rule_ref().starts_with("п. 81"));
        }
        for consequence in Consequence::ALL {
            assert_eq!(consequence.as_str().parse::<Consequence>(), Ok(consequence));
        }
    }

    #[test]
    fn no_applications_and_single_application_are_grounds_after_the_deadline() {
        let closed = Facts {
            deadline_passed: true,
            ..Facts::default()
        };
        assert_eq!(closed.ground(), Some(FailureGround::NoApplications));
        assert_eq!(
            Facts {
                applications: 1,
                ..closed
            }
            .ground(),
            Some(FailureGround::SingleApplication)
        );

        // Пока прием открыт, отсутствие заявок ни о чем не говорит
        assert_eq!(Facts::default().ground(), None);
        assert_eq!(
            Facts {
                applications: 1,
                ..Facts::default()
            }
            .ground(),
            None
        );
    }

    #[test]
    fn fewer_than_two_admitted_counts_only_after_opening() {
        let before = Facts {
            applications: 3,
            admitted: 0,
            deadline_passed: true,
            opened: false,
            winners_evaded: false,
        };
        assert_eq!(before.ground(), None, "до вскрытия о допуске судить рано");

        let after = Facts {
            opened: true,
            admitted: 1,
            ..before
        };
        assert_eq!(after.ground(), Some(FailureGround::FewerThanTwoAdmitted));

        let enough = Facts {
            admitted: 2,
            ..after
        };
        assert_eq!(enough.ground(), None, "двое допущенных - тендер состоялся");
    }

    #[test]
    fn evasion_of_both_winners_outweighs_the_rest() {
        let facts = Facts {
            applications: 3,
            admitted: 3,
            deadline_passed: true,
            opened: true,
            winners_evaded: true,
        };
        assert_eq!(facts.ground(), Some(FailureGround::WinnersEvaded));
    }

    #[test]
    fn single_admitted_application_opens_single_source_contract() {
        let facts = Facts {
            applications: 1,
            admitted: 1,
            deadline_passed: true,
            opened: true,
            winners_evaded: false,
        };
        assert_eq!(
            Consequence::of(FailureGround::SingleApplication, facts, 0),
            Consequence::SingleSource
        );
    }

    #[test]
    fn empty_tender_leads_to_a_repeat() {
        assert_eq!(
            Consequence::of(FailureGround::NoApplications, Facts::default(), 0),
            Consequence::Repeat
        );
    }

    #[test]
    fn second_failure_in_a_row_goes_to_the_board() {
        // п. 83: после двух несостоявшихся вопрос уходит Правлению -
        // независимо от основания и числа заявок
        let facts = Facts {
            applications: 1,
            admitted: 1,
            deadline_passed: true,
            opened: true,
            winners_evaded: false,
        };
        assert_eq!(
            Consequence::of(FailureGround::SingleApplication, facts, 1),
            Consequence::BoardReferral
        );
        assert_eq!(
            Consequence::of(FailureGround::NoApplications, Facts::default(), 2),
            Consequence::BoardReferral
        );
    }
}
