//! Статусная модель тендера (FR-302, М3).
//!
//! Один источник истины в Rust - макрос [`transitions!`] ниже: он порождает
//! И typestate-методы переходов, И константу [`TRANSITIONS`]. Тест паритета
//! (`crates/db/tests/transition_parity.rs`) сверяет константу с seed
//! `refdata.tender_status_transitions` - рассинхрон typestate ↔ БД невозможен
//! незаметно (INV-021). В рантайме переходы дополнительно охраняет триггер БД.

use std::fmt;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use crate::amendment::Instant;
use crate::ids::TenderId;

/// Статусы жизненного цикла (FR-302). Snake_case на проводе и в БД
/// (`core.tender_status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TenderStatus {
    Draft,
    Announced,
    Accepting,
    Qualification,
    Trading,
    SummedUp,
    Contracted,
    Failed,
    RepeatAnnounced,
    Cancelled,
}

impl TenderStatus {
    pub const ALL: [TenderStatus; 10] = [
        TenderStatus::Draft,
        TenderStatus::Announced,
        TenderStatus::Accepting,
        TenderStatus::Qualification,
        TenderStatus::Trading,
        TenderStatus::SummedUp,
        TenderStatus::Contracted,
        TenderStatus::Failed,
        TenderStatus::RepeatAnnounced,
        TenderStatus::Cancelled,
    ];

    /// Значение enum-типа БД `core.tender_status`.
    pub fn as_str(self) -> &'static str {
        match self {
            TenderStatus::Draft => "draft",
            TenderStatus::Announced => "announced",
            TenderStatus::Accepting => "accepting",
            TenderStatus::Qualification => "qualification",
            TenderStatus::Trading => "trading",
            TenderStatus::SummedUp => "summed_up",
            TenderStatus::Contracted => "contracted",
            TenderStatus::Failed => "failed",
            TenderStatus::RepeatAnnounced => "repeat_announced",
            TenderStatus::Cancelled => "cancelled",
        }
    }

    /// Runtime-проверка перехода (для слоя http до обращения к БД).
    pub fn can_transition(from: TenderStatus, to: TenderStatus) -> bool {
        TRANSITIONS.contains(&(from, to))
    }

    /// Статус из значения БД (`core.tender_status`); `None` - рассинхрон
    /// перечислений, а не пользовательская ошибка.
    pub fn parse(raw: &str) -> Option<TenderStatus> {
        TenderStatus::ALL
            .into_iter()
            .find(|status| status.as_str() == raw)
    }

    /// Сроки объявления обязаны быть заданы: тендер прошел публикацию, а
    /// публикуется он только с ними (FR-303). Отмененный - исключение:
    /// отменяется и черновик (п. 78), у которого сроков могло не быть.
    pub fn requires_schedule(self) -> bool {
        !matches!(self, TenderStatus::Draft | TenderStatus::Cancelled)
    }
}

/// Отметка сроков тендера - в порядке процедуры (п. 5–6, 36, 50, 59).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleMark {
    /// Публикация объявления (п. 5–6)
    AnnouncedAt,
    /// Окончание приема заявок (п. 36–39)
    SubmissionDeadline,
    /// Назначенное вскрытие конвертов (п. 50)
    OpeningAt,
    /// Торги (п. 59, 62)
    TradingAt,
}

impl ScheduleMark {
    /// Имя столбца `core.tenders` и поля DTO - одно на всех.
    pub fn as_str(self) -> &'static str {
        match self {
            ScheduleMark::AnnouncedAt => "announced_at",
            ScheduleMark::SubmissionDeadline => "submission_deadline",
            ScheduleMark::OpeningAt => "opening_at",
            ScheduleMark::TradingAt => "trading_at",
        }
    }
}

impl fmt::Display for ScheduleMark {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Сроки тендера: назначаются при публикации, переносятся редакцией
/// документации (FR-303, FR-304), а в обход процедуры - правкой записи
/// администратором (М15). Неназначенная отметка - `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Schedule {
    pub announced_at: Option<Instant>,
    pub submission_deadline: Option<Instant>,
    pub opening_at: Option<Instant>,
    pub trading_at: Option<Instant>,
}

/// Что известно о тендере помимо назначаемых сроков.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduleFacts {
    pub status: TenderStatus,
    /// Факт вскрытия секретарем (FR-403): назначенное вскрытие - не позже него
    pub opened_at: Option<Instant>,
}

impl Schedule {
    /// Отметки в порядке процедуры вместе с именами.
    fn marks(&self) -> [(ScheduleMark, Option<Instant>); 4] {
        [
            (ScheduleMark::AnnouncedAt, self.announced_at),
            (ScheduleMark::SubmissionDeadline, self.submission_deadline),
            (ScheduleMark::OpeningAt, self.opening_at),
            (ScheduleMark::TradingAt, self.trading_at),
        ]
    }

    /// Годятся ли сроки тендеру в таком состоянии (FR-303).
    ///
    /// Порядок - публикация, окончание приема, вскрытие, торги: каждая
    /// назначенная отметка не раньше назначенных до нее. У тендера,
    /// прошедшего публикацию, первые три обязательны - без них он не был
    /// бы опубликован (триггер INV-021). Окно в десять календарных дней
    /// между публикацией и вскрытием здесь не проверяется намеренно: это
    /// условие публикации (п. 5), а не свойство записи, и правка записи
    /// под уже вышедшее объявление обязана уметь отразить его как есть.
    pub fn validate(&self, facts: ScheduleFacts) -> Result<(), ScheduleError> {
        if facts.status.requires_schedule() {
            for (mark, value) in self.marks().into_iter().take(3) {
                if value.is_none() {
                    return Err(ScheduleError::Missing(mark));
                }
            }
        }

        let mut last: Option<(ScheduleMark, Instant)> = None;
        for (mark, value) in self.marks() {
            let Some(at) = value else { continue };
            if let Some((earlier, before)) = last
                && at < before
            {
                return Err(ScheduleError::OutOfOrder {
                    earlier,
                    later: mark,
                });
            }
            last = Some((mark, at));
        }

        if let (Some(opening), Some(opened)) = (self.opening_at, facts.opened_at)
            && opened < opening
        {
            return Err(ScheduleError::OpeningAfterFact);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ScheduleError {
    #[error(
        "{later} раньше, чем {earlier}: сроки идут в порядке процедуры - публикация, прием \
         заявок, вскрытие, торги (п. 5–6, 36, 50, 59)"
    )]
    OutOfOrder {
        earlier: ScheduleMark,
        later: ScheduleMark,
    },
    #[error("у тендера, прошедшего публикацию, срок {0} обязателен (FR-303)")]
    Missing(ScheduleMark),
    #[error("назначенное вскрытие позже состоявшегося: конверты уже вскрыты (FR-403, п. 50)")]
    OpeningAfterFact,
}

mod sealed {
    pub trait Sealed {}
}

/// Маркер статуса для typestate; набор закрыт (sealed).
pub trait Status: sealed::Sealed {
    const STATUS: TenderStatus;
}

macro_rules! status_markers {
    ($($name:ident),+ $(,)?) => {$(
        #[derive(Debug, Clone, Copy)]
        pub struct $name;
        impl sealed::Sealed for $name {}
        impl Status for $name {
            const STATUS: TenderStatus = TenderStatus::$name;
        }
    )+};
}

status_markers!(
    Draft,
    Announced,
    Accepting,
    Qualification,
    Trading,
    SummedUp,
    Contracted,
    Failed,
    RepeatAnnounced,
    Cancelled,
);

/// Тендер в известном на этапе компиляции статусе: метод перехода существует
/// только у того статуса, из которого переход разрешен Правилами.
#[derive(Debug, Clone, Copy)]
pub struct Tender<S: Status> {
    pub id: TenderId,
    _status: PhantomData<S>,
}

impl Tender<Draft> {
    /// Новый тендер всегда начинается черновиком.
    pub fn new(id: TenderId) -> Self {
        Self {
            id,
            _status: PhantomData,
        }
    }
}

impl<S: Status> Tender<S> {
    /// Статус как runtime-значение (для записи в БД, сериализации).
    pub fn status(&self) -> TenderStatus {
        S::STATUS
    }

    /// Восстановление из строки БД, где статус уже гарантирован колонкой
    /// `core.tenders.status` (диспетчеризацию по значению делает слой http).
    pub fn hydrate(id: TenderId) -> Self {
        Self {
            id,
            _status: PhantomData,
        }
    }

    fn into_state<T: Status>(self) -> Tender<T> {
        Tender {
            id: self.id,
            _status: PhantomData,
        }
    }
}

/// Единственное перечисление переходов (INV-021): генерирует typestate-методы
/// и константу для теста паритета с `refdata.tender_status_transitions`.
macro_rules! transitions {
    ($($from:ident -> $to:ident = $method:ident: $doc:literal),+ $(,)?) => {
        /// Разрешенные переходы (FR-302); паритет с БД проверяет
        /// `transition_parity.rs`, в рантайме охраняет триггер INV-021.
        pub const TRANSITIONS: &[(TenderStatus, TenderStatus)] = &[
            $((TenderStatus::$from, TenderStatus::$to)),+
        ];

        $(
            impl Tender<$from> {
                #[doc = $doc]
                pub fn $method(self) -> Tender<$to> {
                    self.into_state()
                }
            }
        )+
    };
}

transitions! {
    Draft           -> Announced       = announce:           "Публикация объявления (FR-303, п. 5–6)",
    Announced       -> Accepting       = open_acceptance:    "Открытие приема заявок (п. 36)",
    Accepting       -> Qualification   = close_acceptance:   "Дедлайн приема, вскрытие (п. 40, 50)",
    Qualification   -> Trading         = begin_trading:      "Допуск завершен, торги назначены (п. 57–59)",
    Trading         -> SummedUp        = sum_up:             "Подведение итогов торгов (п. 73)",
    SummedUp        -> Contracted      = conclude_contract:  "Договор заключен (п. 108)",
    Accepting       -> Failed          = fail_no_bids:       "0 или 1 заявка (п. 81, FR-801)",
    Qualification   -> Failed          = fail_underadmitted: "Допущено менее двух (п. 81)",
    SummedUp        -> Failed          = fail_evasion:       "Уклонение победителя и № 2 (п. 81)",
    Failed          -> RepeatAnnounced = announce_repeat:    "Повторный тендер (п. 82)",
    RepeatAnnounced -> Accepting       = open_acceptance:    "Прием заявок повторного тендера",
    Draft           -> Cancelled       = cancel:             "Отмена (FR-305, п. 78–79)",
    Announced       -> Cancelled       = cancel:             "Отмена (FR-305, п. 78–79)",
    Accepting       -> Cancelled       = cancel:             "Отмена (FR-305, п. 78–79)",
    Qualification   -> Cancelled       = cancel:             "Отмена (FR-305, п. 78–79)",
    Trading         -> Cancelled       = cancel:             "Отмена (FR-305, п. 78–79)",
    SummedUp        -> Cancelled       = cancel:             "Отмена (FR-305, п. 78–79)",
    RepeatAnnounced -> Cancelled       = cancel:             "Отмена (FR-305, п. 78–79)",
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn happy_path_reaches_contracted() {
        // FR-302: полный жизненный цикл выражен типами - код компилируется
        let tender = Tender::new(TenderId::new(Uuid::nil()))
            .announce()
            .open_acceptance()
            .close_acceptance()
            .begin_trading()
            .sum_up()
            .conclude_contract();
        assert_eq!(tender.status(), TenderStatus::Contracted);
    }

    #[test]
    fn failed_tender_can_be_repeated() {
        let tender = Tender::new(TenderId::new(Uuid::nil()))
            .announce()
            .open_acceptance()
            .fail_no_bids()
            .announce_repeat()
            .open_acceptance();
        assert_eq!(tender.status(), TenderStatus::Accepting);
    }

    #[test]
    fn terminal_states_have_no_transitions() {
        // Contracted и Cancelled - терминальные: из них нет ни одного перехода
        for terminal in [TenderStatus::Contracted, TenderStatus::Cancelled] {
            assert!(
                TRANSITIONS.iter().all(|(from, _)| *from != terminal),
                "{terminal:?} должен быть терминальным"
            );
        }
    }

    #[test]
    fn transitions_have_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for pair in TRANSITIONS {
            assert!(seen.insert(pair), "дубль перехода {pair:?}");
        }
    }

    #[test]
    fn can_transition_follows_the_table() {
        assert!(TenderStatus::can_transition(
            TenderStatus::Draft,
            TenderStatus::Announced
        ));
        assert!(!TenderStatus::can_transition(
            TenderStatus::Draft,
            TenderStatus::Trading
        ));
    }

    #[test]
    fn status_strings_match_db_enum() {
        // Паритет serde и core.tender_status: snake_case
        let json = serde_json::to_string(&TenderStatus::RepeatAnnounced).unwrap();
        assert_eq!(json, "\"repeat_announced\"");
        assert_eq!(TenderStatus::SummedUp.as_str(), "summed_up");
    }

    #[test]
    fn parse_round_trips_every_status() {
        for status in TenderStatus::ALL {
            assert_eq!(TenderStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(TenderStatus::parse("published"), None);
    }

    /// Сутки в секундах: календарный день без перевода часов (NFR-03).
    const DAY: i64 = 24 * 60 * 60;

    fn day(n: i64) -> Instant {
        crate::amendment::instant(n * DAY)
    }

    fn facts(status: TenderStatus) -> ScheduleFacts {
        ScheduleFacts {
            status,
            opened_at: None,
        }
    }

    /// Сроки объявления, как в самом объявлении: публикация, прием,
    /// вскрытие на следующий день, торги через три дня после вскрытия.
    fn announced_schedule() -> Schedule {
        Schedule {
            announced_at: Some(day(0)),
            submission_deadline: Some(day(10)),
            opening_at: Some(day(11)),
            trading_at: Some(day(14)),
        }
    }

    /// FR-303: сроки в порядке процедуры принимаются в любом статусе, а
    /// перепутанные - называют обе отметки, чтобы админ понял, какую из
    /// двух он перенес не туда.
    #[test]
    fn schedule_in_procedure_order_is_valid_and_disorder_names_the_marks() {
        for status in TenderStatus::ALL {
            assert_eq!(announced_schedule().validate(facts(status)), Ok(()));
        }

        let opening_before_deadline = Schedule {
            opening_at: Some(day(9)),
            ..announced_schedule()
        };
        assert_eq!(
            opening_before_deadline.validate(facts(TenderStatus::Accepting)),
            Err(ScheduleError::OutOfOrder {
                earlier: ScheduleMark::SubmissionDeadline,
                later: ScheduleMark::OpeningAt,
            })
        );

        // Неназначенная отметка из порядка выпадает: торги сверяются
        // с публикацией, если вскрытие еще не назначено
        let trading_before_announcement = Schedule {
            announced_at: Some(day(5)),
            submission_deadline: None,
            opening_at: None,
            trading_at: Some(day(4)),
        };
        assert_eq!(
            trading_before_announcement.validate(facts(TenderStatus::Draft)),
            Err(ScheduleError::OutOfOrder {
                earlier: ScheduleMark::AnnouncedAt,
                later: ScheduleMark::TradingAt,
            })
        );

        // Совпадающие отметки - не беспорядок: прием может закрываться
        // в момент вскрытия (CHECK deadline_before_opening нестрогий)
        let same_moment = Schedule {
            opening_at: Some(day(10)),
            trading_at: Some(day(10)),
            ..announced_schedule()
        };
        assert_eq!(same_moment.validate(facts(TenderStatus::Accepting)), Ok(()));
    }

    /// FR-303: опубликованный тендер без публикации, приема или вскрытия
    /// невозможен; черновику и отмененному сроки не обязательны.
    #[test]
    fn published_tender_requires_the_first_three_marks() {
        let without_deadline = Schedule {
            submission_deadline: None,
            ..announced_schedule()
        };
        for status in TenderStatus::ALL {
            let verdict = without_deadline.validate(facts(status));
            if status.requires_schedule() {
                assert_eq!(
                    verdict,
                    Err(ScheduleError::Missing(ScheduleMark::SubmissionDeadline)),
                    "{status:?}"
                );
            } else {
                assert_eq!(verdict, Ok(()), "{status:?}");
            }
        }
        assert_eq!(
            Schedule::default().validate(facts(TenderStatus::Draft)),
            Ok(())
        );
        assert!(!TenderStatus::Draft.requires_schedule());
        assert!(!TenderStatus::Cancelled.requires_schedule());
        assert!(TenderStatus::Accepting.requires_schedule());
    }

    /// FR-403: конверты вскрыты - назначить вскрытие позже факта нельзя
    /// (CHECK opened_not_before_meeting), а раньше или в тот же момент можно.
    #[test]
    fn opening_is_not_scheduled_after_the_fact() {
        let opened = ScheduleFacts {
            status: TenderStatus::Qualification,
            opened_at: Some(day(11)),
        };
        assert_eq!(announced_schedule().validate(opened), Ok(()));

        let later = Schedule {
            opening_at: Some(day(12)),
            trading_at: Some(day(14)),
            ..announced_schedule()
        };
        assert_eq!(later.validate(opened), Err(ScheduleError::OpeningAfterFact));
    }
}
