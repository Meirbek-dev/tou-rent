//! Изменение тендерной документации и отмена тендера (М3, FR-304, FR-305,
//! FR-1004, п. 26.5, 27, 78–79).
//!
//! Правка условий после публикации - не редактирование карточки, а новая
//! редакция документации: она возможна только пока до дедлайна больше двух
//! календарных дней, обязана продлить прием заявок не менее чем на десять
//! календарных дней и порождает право участника отказаться с возвратом
//! взноса (п. 26.5). Оба срока живут здесь константами со ссылкой на пункт,
//! а не «в валидации формы».
//!
//! Отмена (п. 78–79) возможна до заключения договора и только с причиной:
//! «отменить, потому что передумали» Правила не знают.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

const SECONDS_PER_DAY: i64 = 24 * 60 * 60;

/// Секунды между моментами; отрицательное значение означает прошедший срок.
fn seconds_between(from: Timestamp, to: Timestamp) -> i64 {
    to.as_second() - from.as_second()
}

/// Момент времени домена: слой данных обращается к нему через этот псевдоним,
/// не завися от календарной библиотеки (арх. § 4 - зависимости смотрят внутрь).
pub type Instant = Timestamp;

/// Момент из unix-секунд (отметки времени БД).
pub fn instant(unix_seconds: i64) -> Timestamp {
    Timestamp::from_second(unix_seconds).unwrap_or(Timestamp::UNIX_EPOCH)
}

/// Ближе этого срока до дедлайна документация не меняется (п. 27).
pub const FREEZE_CALENDAR_DAYS: i64 = 2;

/// Изменение обязано продлить прием заявок не менее чем на столько
/// календарных дней от даты публикации новой редакции (п. 27).
pub const MIN_EXTENSION_CALENDAR_DAYS: i64 = 10;

/// Состояние тендера глазами п. 27: когда правка еще возможна.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AmendmentFacts {
    /// Момент публикации новой редакции (время сервера, NFR-03)
    pub now: Timestamp,
    /// Действующий дедлайн приема заявок
    pub deadline: Option<Timestamp>,
    /// Прием заявок открыт либо объявление опубликовано: до публикации
    /// правится черновик, а не документация
    pub published: bool,
    /// Заявки уже вскрыты (п. 50): менять условия после вскрытия поздно
    pub opened: bool,
}

impl AmendmentFacts {
    /// Можно ли опубликовать новую редакцию с этим дедлайном (FR-304).
    ///
    /// Проверки идут от более грубой к более тонкой: сначала «уместна ли
    /// правка вообще», затем «не поздно ли» и лишь потом «достаточно ли
    /// продлен прием».
    pub fn check(&self, new_deadline: Timestamp) -> Result<(), AmendmentError> {
        if !self.published {
            return Err(AmendmentError::NotPublished);
        }
        if self.opened {
            return Err(AmendmentError::AlreadyOpened);
        }
        let deadline = self.deadline.ok_or(AmendmentError::NoDeadline)?;

        if deadline < self.now {
            return Err(AmendmentError::DeadlinePassed);
        }
        // Окно заморозки: последние два календарных дня приема (п. 27).
        // Календарный день считается сутками: в Казахстане перевода часов нет
        // (NFR-03, Asia/Almaty без DST), поэтому сутки всегда 24 часа.
        if seconds_between(self.now, deadline) < FREEZE_CALENDAR_DAYS * SECONDS_PER_DAY {
            return Err(AmendmentError::FrozenBeforeDeadline);
        }
        if new_deadline <= deadline {
            return Err(AmendmentError::NotExtended);
        }
        if seconds_between(self.now, new_deadline) < MIN_EXTENSION_CALENDAR_DAYS * SECONDS_PER_DAY {
            return Err(AmendmentError::ExtensionTooShort);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AmendmentError {
    #[error(
        "документация изменяется между публикацией объявления и вскрытием: у черновика \
         правятся поля, у завершенного тендера не меняется ничего (п. 5–6, 50)"
    )]
    NotPublished,
    #[error("заявки вскрыты - условия тендера больше не меняются (п. 50)")]
    AlreadyOpened,
    #[error("у тендера не назначен срок приема заявок (FR-303)")]
    NoDeadline,
    #[error("срок приема заявок истек - изменение документации невозможно (п. 27)")]
    DeadlinePassed,
    #[error(
        "до окончания приема заявок осталось меньше {FREEZE_CALENDAR_DAYS} календарных дней - \
         документация не изменяется (п. 27)"
    )]
    FrozenBeforeDeadline,
    #[error("новая редакция обязана продлить срок приема заявок (п. 27)")]
    NotExtended,
    #[error(
        "срок приема заявок продлевается не менее чем на {MIN_EXTENSION_CALENDAR_DAYS} \
         календарных дней от публикации новой редакции (п. 27)"
    )]
    ExtensionTooShort,
}

/// Состояние тендера глазами п. 78–79: возможна ли отмена.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CancellationFacts {
    /// По тендеру уже заключен договор (п. 78: отмена возможна до заключения)
    pub contract_concluded: bool,
    /// Тендер уже отменен
    pub cancelled: bool,
}

impl CancellationFacts {
    /// Причина обязательна: отмена возможна при нарушениях, а не по
    /// усмотрению организатора (п. 78).
    pub fn check(&self, reason: &str) -> Result<(), CancellationError> {
        if self.cancelled {
            return Err(CancellationError::AlreadyCancelled);
        }
        if self.contract_concluded {
            return Err(CancellationError::ContractConcluded);
        }
        if reason.trim().is_empty() {
            return Err(CancellationError::ReasonRequired);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CancellationError {
    #[error("тендер уже отменен (п. 78)")]
    AlreadyCancelled,
    #[error("по тендеру заключен договор - отмена возможна только до его заключения (п. 78)")]
    ContractConcluded,
    #[error("отмена фиксируется с основанием: нарушение, повлекшее ее (п. 78)")]
    ReasonRequired,
}

/// Что именно отменяется (FR-305): тендер целиком или отдельный лот.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationScope {
    Tender,
    Lot,
}

impl CancellationScope {
    pub const ALL: [CancellationScope; 2] = [CancellationScope::Tender, CancellationScope::Lot];

    pub fn as_str(self) -> &'static str {
        match self {
            CancellationScope::Tender => "tender",
            CancellationScope::Lot => "lot",
        }
    }

    pub fn title_ru(self) -> &'static str {
        match self {
            CancellationScope::Tender => "тендер",
            CancellationScope::Lot => "лот",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестный предмет отмены: {0}")]
pub struct UnknownScope(pub String);

impl std::str::FromStr for CancellationScope {
    type Err = UnknownScope;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        CancellationScope::ALL
            .into_iter()
            .find(|scope| scope.as_str() == s)
            .ok_or_else(|| UnknownScope(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(value: &str) -> Timestamp {
        value.parse().unwrap_or(Timestamp::UNIX_EPOCH)
    }

    fn facts(deadline: &str) -> AmendmentFacts {
        AmendmentFacts {
            now: ts("2026-08-07T09:00:00Z"),
            deadline: Some(ts(deadline)),
            published: true,
            opened: false,
        }
    }

    #[test]
    fn amendment_extends_acceptance_by_at_least_ten_days() {
        // Прием заканчивается через пять дней: продлить нужно так, чтобы от
        // публикации редакции осталось не меньше десяти календарных дней
        let facts = facts("2026-08-12T09:00:00Z");
        assert_eq!(facts.check(ts("2026-08-17T09:00:00Z")), Ok(()));
        assert_eq!(
            facts.check(ts("2026-08-16T09:00:00Z")),
            Err(AmendmentError::ExtensionTooShort)
        );
    }

    #[test]
    fn amendment_must_move_the_deadline_forward() {
        let facts = facts("2026-08-20T09:00:00Z");
        assert_eq!(
            facts.check(ts("2026-08-20T09:00:00Z")),
            Err(AmendmentError::NotExtended)
        );
        assert_eq!(
            facts.check(ts("2026-08-19T09:00:00Z")),
            Err(AmendmentError::NotExtended)
        );
    }

    #[test]
    fn last_two_days_of_acceptance_freeze_the_documentation() {
        // Меньше двух календарных дней до дедлайна - правки нет (п. 27)
        let frozen = facts("2026-08-08T20:00:00Z");
        assert_eq!(
            frozen.check(ts("2026-09-01T09:00:00Z")),
            Err(AmendmentError::FrozenBeforeDeadline)
        );

        // Ровно двое суток - еще можно
        let last_moment = facts("2026-08-09T09:00:00Z");
        assert_eq!(last_moment.check(ts("2026-09-01T09:00:00Z")), Ok(()));
    }

    #[test]
    fn documentation_changes_only_between_publication_and_opening() {
        let draft = AmendmentFacts {
            published: false,
            ..facts("2026-08-20T09:00:00Z")
        };
        assert_eq!(
            draft.check(ts("2026-09-01T09:00:00Z")),
            Err(AmendmentError::NotPublished)
        );

        let opened = AmendmentFacts {
            opened: true,
            ..facts("2026-08-20T09:00:00Z")
        };
        assert_eq!(
            opened.check(ts("2026-09-01T09:00:00Z")),
            Err(AmendmentError::AlreadyOpened)
        );

        let expired = AmendmentFacts {
            deadline: Some(ts("2026-08-01T09:00:00Z")),
            ..facts("2026-08-20T09:00:00Z")
        };
        assert_eq!(
            expired.check(ts("2026-09-01T09:00:00Z")),
            Err(AmendmentError::DeadlinePassed)
        );

        let undated = AmendmentFacts {
            deadline: None,
            ..facts("2026-08-20T09:00:00Z")
        };
        assert_eq!(
            undated.check(ts("2026-09-01T09:00:00Z")),
            Err(AmendmentError::NoDeadline)
        );
    }

    #[test]
    fn cancellation_needs_a_reason_and_no_contract() {
        let fresh = CancellationFacts::default();
        assert_eq!(fresh.check("нарушение п. 5"), Ok(()));
        assert_eq!(
            fresh.check("   "),
            Err(CancellationError::ReasonRequired),
            "отмена без основания невозможна"
        );

        let concluded = CancellationFacts {
            contract_concluded: true,
            ..fresh
        };
        assert_eq!(
            concluded.check("нарушение"),
            Err(CancellationError::ContractConcluded)
        );

        let twice = CancellationFacts {
            cancelled: true,
            ..fresh
        };
        assert_eq!(
            twice.check("нарушение"),
            Err(CancellationError::AlreadyCancelled)
        );
    }

    #[test]
    fn scope_wire_names_round_trip() {
        for scope in CancellationScope::ALL {
            assert_eq!(scope.as_str().parse::<CancellationScope>(), Ok(scope));
            assert!(!scope.title_ru().is_empty());
        }
    }
}
