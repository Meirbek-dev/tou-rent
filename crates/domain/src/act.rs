//! Акты приема-передачи и возврата (М9, FR-904, Прил. 7–8, п. 122, 128–129).
//!
//! Акт - событие, меняющее состояние аренды: с даты приема-передачи
//! начисляется плата (п. 128–129), а возврат объекта закрывает договор
//! и освобождает объект (FR-103). Порядок актов задан типом: вернуть
//! непереданное нельзя.

use serde::{Deserialize, Serialize};

/// Вид акта (паритет с enum БД `core.act_kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActKind {
    /// Прием-передача объекта нанимателю (Прил. 7): с этой даты начисляется плата
    Handover,
    /// Возврат объекта наймодателю (Прил. 8): договор закрывается
    Return,
}

impl ActKind {
    pub const ALL: [ActKind; 2] = [ActKind::Handover, ActKind::Return];

    pub fn as_str(self) -> &'static str {
        match self {
            ActKind::Handover => "handover",
            ActKind::Return => "return",
        }
    }

    /// Название для печатной формы и интерфейса (ru - печатные формы, NFR-01).
    pub fn title_ru(self) -> &'static str {
        match self {
            ActKind::Handover => "акт приема-передачи",
            ActKind::Return => "акт возврата",
        }
    }

    /// Приложение Правил с формой акта.
    pub fn appendix(self) -> &'static str {
        match self {
            ActKind::Handover => "Прил. 7",
            ActKind::Return => "Прил. 8",
        }
    }

    pub fn rule_ref(self) -> &'static str {
        match self {
            ActKind::Handover => "п. 122, 128",
            ActKind::Return => "п. 129",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестный вид акта: {0}")]
pub struct UnknownActKind(pub String);

impl std::str::FromStr for ActKind {
    type Err = UnknownActKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ActKind::ALL
            .into_iter()
            .find(|kind| kind.as_str() == s)
            .ok_or_else(|| UnknownActKind(s.to_owned()))
    }
}

/// Состояние договора глазами актов.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ActState {
    /// Договор зарегистрирован (п. 126) - до этого передавать нечего
    pub registered: bool,
    pub handed_over: bool,
    pub returned: bool,
}

impl ActState {
    /// Можно ли составить акт этого вида сейчас (FR-904).
    pub fn check(&self, kind: ActKind) -> Result<(), ActError> {
        if !self.registered {
            return Err(ActError::NotRegistered);
        }
        match kind {
            ActKind::Handover if self.handed_over => Err(ActError::AlreadyDone(kind)),
            ActKind::Handover => Ok(()),
            ActKind::Return if !self.handed_over => Err(ActError::NotHandedOver),
            ActKind::Return if self.returned => Err(ActError::AlreadyDone(kind)),
            ActKind::Return => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ActError {
    #[error("договор не зарегистрирован - передавать объект рано (п. 126)")]
    NotRegistered,
    #[error("объект не передавался: возвращать нечего (п. 129)")]
    NotHandedOver,
    #[error("{} уже составлен", .0.title_ru())]
    AlreadyDone(ActKind),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_names_and_forms_round_trip() {
        for kind in ActKind::ALL {
            assert_eq!(kind.as_str().parse::<ActKind>(), Ok(kind));
            assert!(kind.appendix().starts_with("Прил. "));
            assert!(kind.rule_ref().starts_with("п. "));
            assert!(!kind.title_ru().is_empty());
        }
    }

    #[test]
    fn handover_requires_a_registered_contract() {
        let draft = ActState::default();
        assert_eq!(draft.check(ActKind::Handover), Err(ActError::NotRegistered));

        let registered = ActState {
            registered: true,
            ..draft
        };
        assert_eq!(registered.check(ActKind::Handover), Ok(()));
    }

    #[test]
    fn return_follows_the_handover() {
        let registered = ActState {
            registered: true,
            ..ActState::default()
        };
        assert_eq!(
            registered.check(ActKind::Return),
            Err(ActError::NotHandedOver)
        );

        let handed = ActState {
            handed_over: true,
            ..registered
        };
        assert_eq!(handed.check(ActKind::Return), Ok(()));
        assert_eq!(
            handed.check(ActKind::Handover),
            Err(ActError::AlreadyDone(ActKind::Handover))
        );

        let returned = ActState {
            returned: true,
            ..handed
        };
        assert_eq!(
            returned.check(ActKind::Return),
            Err(ActError::AlreadyDone(ActKind::Return))
        );
    }
}
