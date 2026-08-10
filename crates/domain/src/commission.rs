//! Тендерная комиссия (М11, FR-1101–1103): состав, кворум, подсчет голосов.
//!
//! Чистая логика без IO (арх. § 3). Те же правила закреплены и в БД
//! (триггеры миграции `commission_rules`) - здесь они выражены типом, чтобы
//! ошибка была видна до похода в базу и объяснима пользователю.

use serde::{Deserialize, Serialize};

/// Роль в составе комиссии (паритет с enum БД `core.commission_member_role`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberRole {
    /// Председатель - один на комиссию (п. 11)
    Chairman,
    /// Заместитель председателя - замещает председателя (п. 11–12)
    Deputy,
    Member,
    /// Резервный: голоса не имеет, пока не заменил отведенного (п. 15)
    Reserve,
}

impl MemberRole {
    pub const ALL: [MemberRole; 4] = [
        MemberRole::Chairman,
        MemberRole::Deputy,
        MemberRole::Member,
        MemberRole::Reserve,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            MemberRole::Chairman => "chairman",
            MemberRole::Deputy => "deputy",
            MemberRole::Member => "member",
            MemberRole::Reserve => "reserve",
        }
    }

    /// Голосующий состав (п. 12–13): резервные в него не входят.
    pub fn votes(self) -> bool {
        !matches!(self, MemberRole::Reserve)
    }

    /// Кто может председательствовать на заседании (п. 12, 14).
    pub fn may_chair(self) -> bool {
        matches!(self, MemberRole::Chairman | MemberRole::Deputy)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестная роль в комиссии: {0}")]
pub struct UnknownMemberRole(pub String);

impl std::str::FromStr for MemberRole {
    type Err = UnknownMemberRole;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "chairman" => Ok(MemberRole::Chairman),
            "deputy" => Ok(MemberRole::Deputy),
            "member" => Ok(MemberRole::Member),
            "reserve" => Ok(MemberRole::Reserve),
            other => Err(UnknownMemberRole(other.to_owned())),
        }
    }
}

/// Минимальный голосующий состав комиссии (п. 9).
pub const MIN_VOTING_MEMBERS: usize = 7;

/// Состав комиссии в разрезе ролей (FR-1101).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Composition {
    pub chairmen: usize,
    pub deputies: usize,
    pub members: usize,
    pub reserves: usize,
}

impl Composition {
    /// Свертка списка ролей (порядок и источник значения не важны).
    pub fn of(roles: impl IntoIterator<Item = MemberRole>) -> Self {
        roles.into_iter().fold(Self::default(), |mut acc, role| {
            match role {
                MemberRole::Chairman => acc.chairmen += 1,
                MemberRole::Deputy => acc.deputies += 1,
                MemberRole::Member => acc.members += 1,
                MemberRole::Reserve => acc.reserves += 1,
            }
            acc
        })
    }

    /// Голосующий состав: председатель, заместитель и члены (без резервных).
    pub fn voting(&self) -> usize {
        self.chairmen + self.deputies + self.members
    }

    /// Состав, который можно утверждать (п. 9–11).
    pub fn validate(&self) -> Result<(), CompositionError> {
        match (self.chairmen, self.deputies) {
            (1, 1) => {}
            (0, _) => return Err(CompositionError::NoChairman),
            (_, 0) => return Err(CompositionError::NoDeputy),
            (chairmen, _) if chairmen > 1 => return Err(CompositionError::ManyChairmen(chairmen)),
            (_, deputies) => return Err(CompositionError::ManyDeputies(deputies)),
        }

        let voting = self.voting();
        if voting < MIN_VOTING_MEMBERS {
            return Err(CompositionError::TooFew(voting));
        }
        if voting.is_multiple_of(2) {
            return Err(CompositionError::Even(voting));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CompositionError {
    #[error("в составе нет председателя (п. 11)")]
    NoChairman,
    #[error("председателей больше одного: {0} (п. 11)")]
    ManyChairmen(usize),
    #[error("в составе нет заместителя председателя (п. 11)")]
    NoDeputy,
    #[error("заместителей больше одного: {0} (п. 11)")]
    ManyDeputies(usize),
    #[error("голосующих членов {0}, требуется не менее {MIN_VOTING_MEMBERS} (п. 9)")]
    TooFew(usize),
    #[error("голосующих членов {0} - состав должен быть нечетным (п. 9)")]
    Even(usize),
}

/// Кворум ⅔ голосующего состава, округление вверх (п. 12).
pub fn quorum_required(voting_total: usize) -> usize {
    (voting_total * 2).div_ceil(3)
}

/// Явка на заседание (FR-1102).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attendance {
    /// Голосующий состав комиссии
    pub voting_total: usize,
    /// Из них присутствуют
    pub present: usize,
    /// Присутствует председатель или его заместитель
    pub chair_present: bool,
}

impl Attendance {
    /// Заседание не открывается без кворума (п. 12).
    pub fn check(&self) -> Result<(), QuorumError> {
        let required = quorum_required(self.voting_total);
        if self.present < required {
            return Err(QuorumError::NotEnough {
                present: self.present,
                required,
            });
        }
        if !self.chair_present {
            return Err(QuorumError::NoChair);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum QuorumError {
    #[error("присутствует {present} из требуемых {required} (кворум ⅔, п. 12)")]
    NotEnough { present: usize, required: usize },
    #[error("нет ни председателя, ни его заместителя (п. 12)")]
    NoChair,
}

/// Голос члена комиссии: «воздержался» не существует (INV-055, п. 55.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Vote {
    For,
    Against,
}

impl Vote {
    pub fn as_str(self) -> &'static str {
        match self {
            Vote::For => "for",
            Vote::Against => "against",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестное значение голоса: {0}")]
pub struct UnknownVote(pub String);

impl std::str::FromStr for Vote {
    type Err = UnknownVote;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "for" => Ok(Vote::For),
            "against" => Ok(Vote::Against),
            other => Err(UnknownVote(other.to_owned())),
        }
    }
}

/// Итог голосования по одному вопросу.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Admitted,
    Rejected,
}

/// Подсчет голосов присутствующих (FR-1103, п. 13–14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tally {
    /// Присутствующие с правом голоса по этому вопросу (отведенные не в счет)
    pub eligible: usize,
    pub votes_for: usize,
    pub votes_against: usize,
    /// Голос председательствующего - решает при равенстве (п. 14)
    pub chair_vote: Option<Vote>,
}

impl Tally {
    /// Решение большинством присутствующих; при равенстве - голос
    /// председательствующего. Пока проголосовали не все - решения нет:
    /// «досчитать» его за отсутствующих нельзя.
    pub fn outcome(&self) -> Result<Decision, TallyError> {
        let cast = self.votes_for + self.votes_against;
        if self.eligible == 0 {
            return Err(TallyError::NoEligible);
        }
        if cast > self.eligible {
            return Err(TallyError::MoreVotesThanMembers {
                cast,
                eligible: self.eligible,
            });
        }
        if cast < self.eligible {
            return Err(TallyError::Incomplete {
                cast,
                eligible: self.eligible,
            });
        }

        match self.votes_for.cmp(&self.votes_against) {
            std::cmp::Ordering::Greater => Ok(Decision::Admitted),
            std::cmp::Ordering::Less => Ok(Decision::Rejected),
            // Равенство возможно при четном числе присутствующих (п. 14)
            std::cmp::Ordering::Equal => match self.chair_vote {
                Some(Vote::For) => Ok(Decision::Admitted),
                Some(Vote::Against) => Ok(Decision::Rejected),
                None => Err(TallyError::TieWithoutChair),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TallyError {
    #[error("нет ни одного члена комиссии с правом голоса по этому вопросу")]
    NoEligible,
    #[error("проголосовали {cast} из {eligible} присутствующих (п. 13)")]
    Incomplete { cast: usize, eligible: usize },
    #[error("голосов {cast} больше, чем присутствующих {eligible}")]
    MoreVotesThanMembers { cast: usize, eligible: usize },
    #[error("голоса разделились поровну, а председательствующий не голосовал (п. 14)")]
    TieWithoutChair,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn composition(
        chairmen: usize,
        deputies: usize,
        members: usize,
        reserves: usize,
    ) -> Composition {
        Composition {
            chairmen,
            deputies,
            members,
            reserves,
        }
    }

    #[test]
    fn valid_composition_is_odd_and_at_least_seven() {
        // 1 председатель + 1 заместитель + 5 членов = 7, резервные не в счет
        assert_eq!(composition(1, 1, 5, 2).validate(), Ok(()));
        assert_eq!(composition(1, 1, 7, 0).validate(), Ok(()));
    }

    #[test]
    fn composition_rejects_even_and_small_and_headless() {
        assert_eq!(
            composition(1, 1, 6, 0).validate(),
            Err(CompositionError::Even(8))
        );
        assert_eq!(
            composition(1, 1, 3, 9).validate(),
            Err(CompositionError::TooFew(5))
        );
        assert_eq!(
            composition(0, 1, 6, 0).validate(),
            Err(CompositionError::NoChairman)
        );
        assert_eq!(
            composition(1, 0, 6, 0).validate(),
            Err(CompositionError::NoDeputy)
        );
        assert_eq!(
            composition(2, 1, 4, 0).validate(),
            Err(CompositionError::ManyChairmen(2))
        );
        assert_eq!(
            composition(1, 2, 4, 0).validate(),
            Err(CompositionError::ManyDeputies(2))
        );
    }

    #[test]
    fn composition_folds_roles() {
        let roles = [
            MemberRole::Chairman,
            MemberRole::Deputy,
            MemberRole::Member,
            MemberRole::Member,
            MemberRole::Reserve,
        ];
        let composition = Composition::of(roles);
        assert_eq!(composition.voting(), 4);
        assert_eq!(composition.reserves, 1);
    }

    #[test]
    fn quorum_is_two_thirds_rounded_up() {
        assert_eq!(quorum_required(7), 5); // 4,67 → 5
        assert_eq!(quorum_required(9), 6);
        assert_eq!(quorum_required(11), 8); // 7,33 → 8
    }

    #[test]
    fn meeting_opens_only_with_quorum_and_chair() {
        let full = Attendance {
            voting_total: 7,
            present: 5,
            chair_present: true,
        };
        assert_eq!(full.check(), Ok(()));

        assert_eq!(
            Attendance { present: 4, ..full }.check(),
            Err(QuorumError::NotEnough {
                present: 4,
                required: 5
            })
        );
        assert_eq!(
            Attendance {
                chair_present: false,
                ..full
            }
            .check(),
            Err(QuorumError::NoChair)
        );
    }

    #[test]
    fn majority_of_present_decides() {
        let tally = Tally {
            eligible: 5,
            votes_for: 3,
            votes_against: 2,
            chair_vote: Some(Vote::Against),
        };
        assert_eq!(tally.outcome(), Ok(Decision::Admitted));

        let tally = Tally {
            votes_for: 2,
            votes_against: 3,
            ..tally
        };
        assert_eq!(tally.outcome(), Ok(Decision::Rejected));
    }

    #[test]
    fn tie_is_broken_by_the_chairing_member() {
        let tie = Tally {
            eligible: 4,
            votes_for: 2,
            votes_against: 2,
            chair_vote: Some(Vote::For),
        };
        assert_eq!(tie.outcome(), Ok(Decision::Admitted));
        assert_eq!(
            Tally {
                chair_vote: Some(Vote::Against),
                ..tie
            }
            .outcome(),
            Ok(Decision::Rejected)
        );
        assert_eq!(
            Tally {
                chair_vote: None,
                ..tie
            }
            .outcome(),
            Err(TallyError::TieWithoutChair)
        );
    }

    #[test]
    fn decision_waits_for_every_present_member() {
        let tally = Tally {
            eligible: 5,
            votes_for: 3,
            votes_against: 1,
            chair_vote: None,
        };
        assert_eq!(
            tally.outcome(),
            Err(TallyError::Incomplete {
                cast: 4,
                eligible: 5
            })
        );
    }

    #[test]
    fn vote_and_role_wire_names_match_db_enums() {
        assert_eq!(Vote::For.as_str(), "for");
        assert_eq!("against".parse::<Vote>(), Ok(Vote::Against));
        for role in MemberRole::ALL {
            assert_eq!(role.as_str().parse::<MemberRole>(), Ok(role));
        }
        assert!(!MemberRole::Reserve.votes(), "резервный голоса не имеет");
        assert!(
            MemberRole::Deputy.may_chair(),
            "заместитель председательствует"
        );
    }
}
