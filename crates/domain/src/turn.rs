//! Очередность торгов по кругу (М6, FR-604–605, п. 65, 70–71).
//!
//! Конечный автомат круга: участники ходят по очереди, не готовый повысить
//! выбывает, торги идут, пока не останется один. Отсутствующему объявляется
//! его первоначальное предложение (п. 70) - в круг он не входит и повышать
//! не может; если отсутствуют все, победитель определяется по максимальному
//! первоначальному предложению без торгов (п. 71).
//!
//! Времени здесь нет: таймер ведет сервер (FR-602), порядок ставок - БД.

use serde::{Deserialize, Serialize};

use crate::ids::ApplicationId;

/// Состояние участника в круге (паритет с enum БД `core.auction_participant_status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantState {
    /// Торгуется: очередь до него дойдет
    Active,
    /// Не был готов повысить и выбыл из торгов (п. 65)
    Passed,
    /// Не явился: объявлено первоначальное предложение (п. 70)
    Absent,
}

impl ParticipantState {
    pub const ALL: [ParticipantState; 3] = [
        ParticipantState::Active,
        ParticipantState::Passed,
        ParticipantState::Absent,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ParticipantState::Active => "active",
            ParticipantState::Passed => "passed",
            ParticipantState::Absent => "absent",
        }
    }

    /// В круге остаются только активные - выбывшие и отсутствующие не ходят.
    pub fn in_circle(self) -> bool {
        matches!(self, ParticipantState::Active)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестное состояние участника торгов: {0}")]
pub struct UnknownParticipantState(pub String);

impl std::str::FromStr for ParticipantState {
    type Err = UnknownParticipantState;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ParticipantState::ALL
            .into_iter()
            .find(|state| state.as_str() == s)
            .ok_or_else(|| UnknownParticipantState(s.to_owned()))
    }
}

/// Участник круга. `order` - место в очередности; берется из журнала
/// регистрации заявок (единственный законный порядок, A-045).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Participant {
    pub application_id: ApplicationId,
    pub order: i32,
    pub state: ParticipantState,
}

/// Круг торгов: участники в порядке очередности.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Circle {
    participants: Vec<Participant>,
}

impl Circle {
    /// Порядок задается полем `order`; повторы и пропуски номеров допустимы -
    /// важен лишь относительный порядок.
    pub fn new(mut participants: Vec<Participant>) -> Self {
        participants.sort_by_key(|p| (p.order, p.application_id.into_uuid()));
        Self { participants }
    }

    pub fn participants(&self) -> &[Participant] {
        &self.participants
    }

    pub fn active(&self) -> impl Iterator<Item = &Participant> {
        self.participants.iter().filter(|p| p.state.in_circle())
    }

    pub fn active_count(&self) -> usize {
        self.active().count()
    }

    /// Первый ход - за участником с наименьшим номером очередности.
    pub fn first_turn(&self) -> Option<ApplicationId> {
        self.active().map(|p| p.application_id).next()
    }

    /// Следующий ход после `current` - по кругу, мимо выбывших (п. 65).
    /// `None` - ходить некому (в круге пусто).
    pub fn next_turn(&self, current: ApplicationId) -> Option<ApplicationId> {
        let active: Vec<ApplicationId> = self.active().map(|p| p.application_id).collect();
        if active.is_empty() {
            return None;
        }
        match active.iter().position(|id| *id == current) {
            // Круг замыкается: после последнего снова первый
            Some(index) => Some(active[(index + 1) % active.len()]),
            // Текущий уже выбыл (спасовал) - ход идет к первому, кто стоит
            // после него по номеру очередности
            None => {
                let order = self
                    .participants
                    .iter()
                    .find(|p| p.application_id == current)
                    .map(|p| p.order);
                match order {
                    Some(order) => self
                        .active()
                        .find(|p| p.order > order)
                        .or_else(|| self.active().next())
                        .map(|p| p.application_id),
                    None => active.first().copied(),
                }
            }
        }
    }

    /// Что делать после хода: продолжать круг или заканчивать торги.
    ///
    /// Торги заканчиваются, когда в круге не осталось соперников: один
    /// активный участник (п. 65) либо ни одного - все выбыли или не явились
    /// (п. 70–71).
    pub fn after_move(&self, current: ApplicationId) -> Progress {
        match self.active_count() {
            0 => Progress::Finished,
            1 => Progress::Finished,
            _ => match self.next_turn(current) {
                Some(next) => Progress::Turn(next),
                None => Progress::Finished,
            },
        }
    }
}

/// Ход круга после действия участника.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// Ход переходит к этому участнику
    Turn(ApplicationId),
    /// Соперников не осталось - торги завершаются (п. 65, 71)
    Finished,
}

/// Отказ действия участника торгов.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TurnError {
    #[error("сейчас ход другого участника (п. 65)")]
    NotYourTurn,
    #[error("участник выбыл из торгов и больше не повышает (п. 65)")]
    AlreadyPassed,
    #[error("участник не явился: объявлено его первоначальное предложение (п. 70)")]
    Absent,
    #[error("участник не допущен к торгам по этому лоту (п. 62)")]
    NotInCircle,
}

impl Circle {
    /// Может ли участник сейчас ходить (ставка или пас).
    pub fn check_move(
        &self,
        application_id: ApplicationId,
        current_turn: Option<ApplicationId>,
    ) -> Result<(), TurnError> {
        let participant = self
            .participants
            .iter()
            .find(|p| p.application_id == application_id)
            .ok_or(TurnError::NotInCircle)?;

        match participant.state {
            ParticipantState::Passed => return Err(TurnError::AlreadyPassed),
            ParticipantState::Absent => return Err(TurnError::Absent),
            ParticipantState::Active => {}
        }

        match current_turn {
            Some(turn) if turn == application_id => Ok(()),
            // Ход не назначен - торги еще не в круге (например, только начались)
            None => Ok(()),
            Some(_) => Err(TurnError::NotYourTurn),
        }
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn app(tag: u128) -> ApplicationId {
        ApplicationId::new(Uuid::from_u128(tag))
    }

    fn circle(states: &[(u128, i32, ParticipantState)]) -> Circle {
        Circle::new(
            states
                .iter()
                .map(|(tag, order, state)| Participant {
                    application_id: app(*tag),
                    order: *order,
                    state: *state,
                })
                .collect(),
        )
    }

    #[test]
    fn turn_goes_round_in_registration_order() {
        let circle = circle(&[
            (2, 2, ParticipantState::Active),
            (1, 1, ParticipantState::Active),
            (3, 3, ParticipantState::Active),
        ]);

        assert_eq!(circle.first_turn(), Some(app(1)));
        assert_eq!(circle.next_turn(app(1)), Some(app(2)));
        assert_eq!(circle.next_turn(app(2)), Some(app(3)));
        // Круг замыкается на первом (п. 65)
        assert_eq!(circle.next_turn(app(3)), Some(app(1)));
    }

    #[test]
    fn passed_and_absent_are_skipped() {
        let circle = circle(&[
            (1, 1, ParticipantState::Active),
            (2, 2, ParticipantState::Passed),
            (3, 3, ParticipantState::Absent),
            (4, 4, ParticipantState::Active),
        ]);

        assert_eq!(circle.next_turn(app(1)), Some(app(4)));
        assert_eq!(circle.next_turn(app(4)), Some(app(1)));
        // Ход после выбывшего достается следующему по очередности
        assert_eq!(circle.next_turn(app(2)), Some(app(4)));
    }

    #[test]
    fn trading_ends_when_a_single_bidder_remains() {
        let circle = circle(&[
            (1, 1, ParticipantState::Active),
            (2, 2, ParticipantState::Passed),
        ]);
        assert_eq!(circle.after_move(app(1)), Progress::Finished);
        assert_eq!(circle.active_count(), 1);
    }

    #[test]
    fn trading_continues_while_rivals_remain() {
        let circle = circle(&[
            (1, 1, ParticipantState::Active),
            (2, 2, ParticipantState::Active),
        ]);
        assert_eq!(circle.after_move(app(1)), Progress::Turn(app(2)));
    }

    #[test]
    fn nobody_present_finishes_immediately() {
        // п. 71: явились не все - победитель по первоначальным предложениям
        let circle = circle(&[
            (1, 1, ParticipantState::Absent),
            (2, 2, ParticipantState::Absent),
        ]);
        assert_eq!(circle.active_count(), 0);
        assert_eq!(circle.first_turn(), None);
        assert_eq!(circle.after_move(app(1)), Progress::Finished);
    }

    #[test]
    fn only_the_participant_on_turn_may_move() {
        let circle = circle(&[
            (1, 1, ParticipantState::Active),
            (2, 2, ParticipantState::Active),
            (3, 3, ParticipantState::Passed),
            (4, 4, ParticipantState::Absent),
        ]);

        assert_eq!(circle.check_move(app(1), Some(app(1))), Ok(()));
        assert_eq!(
            circle.check_move(app(2), Some(app(1))),
            Err(TurnError::NotYourTurn)
        );
        assert_eq!(
            circle.check_move(app(3), Some(app(3))),
            Err(TurnError::AlreadyPassed)
        );
        assert_eq!(
            circle.check_move(app(4), Some(app(4))),
            Err(TurnError::Absent)
        );
        assert_eq!(
            circle.check_move(app(9), Some(app(1))),
            Err(TurnError::NotInCircle)
        );
    }

    #[test]
    fn state_wire_names_match_db_enum() {
        for state in ParticipantState::ALL {
            assert_eq!(state.as_str().parse::<ParticipantState>(), Ok(state));
        }
        assert!(ParticipantState::Active.in_circle());
        assert!(!ParticipantState::Passed.in_circle());
        assert!(!ParticipantState::Absent.in_circle());
    }
}
