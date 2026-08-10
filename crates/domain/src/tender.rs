//! Статусная модель тендера (FR-302, М3).
//!
//! Один источник истины в Rust - макрос [`transitions!`] ниже: он порождает
//! И typestate-методы переходов, И константу [`TRANSITIONS`]. Тест паритета
//! (`crates/db/tests/transition_parity.rs`) сверяет константу с seed
//! `refdata.tender_status_transitions` - рассинхрон typestate ↔ БД невозможен
//! незаметно (INV-021). В рантайме переходы дополнительно охраняет триггер БД.

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

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
}
