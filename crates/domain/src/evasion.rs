//! Уклонение победителя от подписания договора (М9, FR-903, FR-505,
//! п. 116–120).
//!
//! Уклонение - не решение организатора, а вывод из фактов конвейера:
//! пока победитель не вернул подписанный договор, срок п. 111 идет; когда
//! он истек либо получен письменный отказ, наступает уклонение с готовыми
//! следствиями (п. 116–118). Следствия тоже не выбираются: взнос уклониста
//! удерживается всегда, договор предлагается участнику № 2 - а если его
//! нет или он уклонился тоже, тендер признается несостоявшимся (п. 81.4).
//!
//! Уклонившийся попадает в реестр (FR-505): его заявки в будущих тендерах
//! отклоняются основанием п. 52.4 автоматически, без обсуждения комиссией.

use serde::{Deserialize, Serialize};

/// Место в итогах торгов (п. 74): договор предлагается по порядку мест.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Place {
    /// Победитель торгов
    Winner,
    /// Участник № 2 - следующий после победителя (п. 74, 117)
    RunnerUp,
}

impl Place {
    pub const ALL: [Place; 2] = [Place::Winner, Place::RunnerUp];

    pub fn as_str(self) -> &'static str {
        match self {
            Place::Winner => "winner",
            Place::RunnerUp => "runner_up",
        }
    }

    /// Название для протокола и интерфейса (ru - печатные формы, NFR-01).
    pub fn title_ru(self) -> &'static str {
        match self {
            Place::Winner => "победитель",
            Place::RunnerUp => "участник № 2",
        }
    }

    /// Кому договор предлагается после уклонения: за участником № 2
    /// третьего места Правила не знают (п. 117, 81.4).
    pub fn next(self) -> Option<Place> {
        match self {
            Place::Winner => Some(Place::RunnerUp),
            Place::RunnerUp => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестное место в итогах торгов: {0}")]
pub struct UnknownPlace(pub String);

impl std::str::FromStr for Place {
    type Err = UnknownPlace;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Place::ALL
            .into_iter()
            .find(|place| place.as_str() == s)
            .ok_or_else(|| UnknownPlace(s.to_owned()))
    }
}

/// Основание уклонения (п. 116) - закрытый перечень, как и основания
/// отклонения заявки (INV-052): «уклонился» нельзя записать по усмотрению.
///
/// TODO-ENGINEER: формулировки и состав перечня выверяются по Правилам
/// (Q-006); коды и привязка к срокам п. 111–112 зафиксированы по ТЗ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvasionGround {
    /// Подписанный договор не возвращен в срок (п. 111, 116)
    SigningDeadlineMissed,
    /// Документы для сверки не представлены в срок (п. 112, 116)
    DocumentsDeadlineMissed,
    /// Письменный отказ от подписания договора (п. 116)
    Refused,
}

impl EvasionGround {
    pub const ALL: [EvasionGround; 3] = [
        EvasionGround::SigningDeadlineMissed,
        EvasionGround::DocumentsDeadlineMissed,
        EvasionGround::Refused,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            EvasionGround::SigningDeadlineMissed => "signing_deadline_missed",
            EvasionGround::DocumentsDeadlineMissed => "documents_deadline_missed",
            EvasionGround::Refused => "refused",
        }
    }

    /// Пункт Правил - идет в протокол о победителе № 2 и в реестр.
    pub fn rule_ref(self) -> &'static str {
        match self {
            EvasionGround::SigningDeadlineMissed => "п. 111, 116",
            EvasionGround::DocumentsDeadlineMissed => "п. 112, 116",
            EvasionGround::Refused => "п. 116",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестное основание уклонения: {0}")]
pub struct UnknownGround(pub String);

impl std::str::FromStr for EvasionGround {
    type Err = UnknownGround;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        EvasionGround::ALL
            .into_iter()
            .find(|ground| ground.as_str() == s)
            .ok_or_else(|| UnknownGround(s.to_owned()))
    }
}

/// Факты договора, из которых выводится уклонение (п. 116–118).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Facts {
    /// Чей это договор: победителя или уже участника № 2
    pub place: Place,
    /// Экземпляр договора передан стороне (п. 110): до передачи уклоняться не от чего
    pub handed_to_tenant: bool,
    /// Подписанный договор возвращен (п. 111) - уклонения больше нет
    pub tenant_signed: bool,
    /// Уклонение по этому договору уже зафиксировано
    pub declared: bool,
    /// В итогах торгов есть участник № 2 с его ставкой (п. 74)
    pub runner_up_available: bool,
}

impl Facts {
    /// Можно ли признать уклонение сейчас и что из него следует (FR-903).
    ///
    /// Взнос уклониста удерживается при любом основании и любом месте
    /// (п. 116) - это не вариант следствия, а его неизменная часть.
    pub fn check(&self) -> Result<Consequence, EvasionError> {
        if self.tenant_signed {
            return Err(EvasionError::AlreadySigned);
        }
        if !self.handed_to_tenant {
            return Err(EvasionError::NotHandedOver);
        }
        if self.declared {
            return Err(EvasionError::AlreadyDeclared(self.place));
        }
        Ok(self.consequence())
    }

    /// Следствие уклонения: договор идет участнику № 2 (п. 117), а если
    /// его нет или уклонился он сам - тендер несостоявшийся (п. 81.4).
    fn consequence(&self) -> Consequence {
        match self.place.next() {
            Some(Place::RunnerUp) if self.runner_up_available => Consequence::OfferToRunnerUp,
            _ => Consequence::TenderFailed,
        }
    }
}

/// Следствие уклонения (п. 117, 81.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Consequence {
    /// Договор предлагается участнику № 2: протокол за 5 р. дней,
    /// уведомление не позднее следующего рабочего дня, дальше - те же
    /// сроки конвейера п. 110–115 (FR-903)
    OfferToRunnerUp,
    /// Второго места нет либо уклонился и он: основание п. 81.4 (FR-801)
    TenderFailed,
}

impl Consequence {
    pub const ALL: [Consequence; 2] = [Consequence::OfferToRunnerUp, Consequence::TenderFailed];

    pub fn as_str(self) -> &'static str {
        match self {
            Consequence::OfferToRunnerUp => "offer_to_runner_up",
            Consequence::TenderFailed => "tender_failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EvasionError {
    #[error("договор подписан нанимателем - уклонения нет (п. 111)")]
    AlreadySigned,
    #[error("экземпляр договора не передавался - уклоняться не от чего (п. 110)")]
    NotHandedOver,
    #[error("уклонение уже зафиксировано ({})", .0.title_ru())]
    AlreadyDeclared(Place),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn winner() -> Facts {
        Facts {
            place: Place::Winner,
            handed_to_tenant: true,
            tenant_signed: false,
            declared: false,
            runner_up_available: true,
        }
    }

    #[test]
    fn wire_names_round_trip() {
        for place in Place::ALL {
            assert_eq!(place.as_str().parse::<Place>(), Ok(place));
            assert!(!place.title_ru().is_empty());
        }
        for ground in EvasionGround::ALL {
            assert_eq!(ground.as_str().parse::<EvasionGround>(), Ok(ground));
            assert!(ground.rule_ref().contains("116"));
            assert_eq!(
                serde_json::to_value(ground).unwrap(),
                serde_json::Value::String(ground.as_str().to_owned())
            );
        }
        for consequence in Consequence::ALL {
            assert_eq!(
                serde_json::to_value(consequence).unwrap(),
                serde_json::Value::String(consequence.as_str().to_owned())
            );
        }
    }

    #[test]
    fn signed_contract_leaves_no_room_for_evasion() {
        let signed = Facts {
            tenant_signed: true,
            ..winner()
        };
        assert_eq!(signed.check(), Err(EvasionError::AlreadySigned));
    }

    #[test]
    fn evasion_starts_after_the_copy_is_handed_over() {
        let not_handed = Facts {
            handed_to_tenant: false,
            ..winner()
        };
        assert_eq!(not_handed.check(), Err(EvasionError::NotHandedOver));
        assert_eq!(winner().check(), Ok(Consequence::OfferToRunnerUp));
    }

    #[test]
    fn evasion_is_recorded_once() {
        let declared = Facts {
            declared: true,
            ..winner()
        };
        assert_eq!(
            declared.check(),
            Err(EvasionError::AlreadyDeclared(Place::Winner))
        );
    }

    #[test]
    fn winner_without_a_runner_up_fails_the_tender() {
        // п. 74: второго места может не быть (торги без второй ставки)
        let alone = Facts {
            runner_up_available: false,
            ..winner()
        };
        assert_eq!(alone.check(), Ok(Consequence::TenderFailed));
    }

    #[test]
    fn evasion_of_the_runner_up_ends_the_tender() {
        // За участником № 2 третьего места Правила не знают (п. 81.4)
        let second = Facts {
            place: Place::RunnerUp,
            ..winner()
        };
        assert_eq!(Place::RunnerUp.next(), None);
        assert_eq!(second.check(), Ok(Consequence::TenderFailed));
    }
}
