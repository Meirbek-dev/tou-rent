//! Договорный конвейер (М9, FR-901–902, INV-115, п. 108–115, 125–126).
//!
//! Существенные условия договора - снимок итогов торгов: ставка победителя,
//! объект и срок найма. Они выражены отдельным типом без публичных сеттеров:
//! «поправить ставку в договоре» нельзя написать даже случайно, а БД
//! запрещает менять их и в обход приложения (FR-901).
//!
//! Сроки п. 110–115 идут цепочкой: каждый шаг конвейера открывает следующий
//! и закрывает свой срок (FR-902). Порядок шагов задан типом - пропустить
//! сверку документов перед подписанием наймодателя невозможно (INV-115).

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::money::Money;

/// Существенные условия (FR-901): переносятся из протокола итогов и дальше
/// только читаются. Конструктор один - из итогов торгов.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EssentialTerms {
    monthly_rate: Decimal,
    lease_months: i32,
}

impl EssentialTerms {
    /// Из итогов торгов: ставка победителя и срок найма лота (п. 108).
    pub fn from_auction(winning_bid: Money, lease_months: i32) -> Result<Self, TermsError> {
        if winning_bid.amount() <= Decimal::ZERO {
            return Err(TermsError::NonPositiveRate);
        }
        if lease_months <= 0 {
            return Err(TermsError::NonPositiveTerm);
        }
        Ok(Self {
            monthly_rate: winning_bid.amount(),
            lease_months,
        })
    }

    /// Месячная плата - ставка, на которой закончились торги (FR-606).
    pub fn monthly_rate(&self) -> Money {
        Money::new(self.monthly_rate)
    }

    pub fn lease_months(&self) -> i32 {
        self.lease_months
    }

    /// Депозит по договору равен месячной плате (п. 132, FR-1003).
    pub fn deposit(&self) -> Money {
        self.monthly_rate()
    }

    /// Совпадают ли условия договора с итогами торгов: используется при
    /// сверке снимка договора с протоколом (FR-901).
    pub fn matches(&self, winning_bid: Money, lease_months: i32) -> bool {
        self.monthly_rate == winning_bid.amount() && self.lease_months == lease_months
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TermsError {
    #[error("ставка договора должна быть положительной (п. 108)")]
    NonPositiveRate,
    #[error("срок найма должен быть положительным (п. 108)")]
    NonPositiveTerm,
}

/// Шаг договорного конвейера (FR-902, п. 110–115).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Договор составлен наймодателем (15 р. дней от протокола, п. 110)
    Drafted,
    /// Экземпляр передан победителю
    HandedToTenant,
    /// Победитель вернул подписанный договор (10 р. дней, п. 111)
    TenantSigned,
    /// Победитель представил документы для сверки (7 р. дней, п. 112)
    DocumentsReceived,
    /// Сверка документов завершена (п. 113) - без нее подписания нет (INV-115)
    ChecklistCompleted,
    /// Наймодатель подписал (2 р. дня, п. 114)
    LandlordSigned,
    /// Экземпляр направлен нанимателю (2 р. дня, п. 115)
    CopySent,
    /// Договор зарегистрирован в журнале - дата регистрации = дата заключения (п. 126)
    Registered,
}

impl Stage {
    pub const ALL: [Stage; 8] = [
        Stage::Drafted,
        Stage::HandedToTenant,
        Stage::TenantSigned,
        Stage::DocumentsReceived,
        Stage::ChecklistCompleted,
        Stage::LandlordSigned,
        Stage::CopySent,
        Stage::Registered,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Drafted => "drafted",
            Stage::HandedToTenant => "handed_to_tenant",
            Stage::TenantSigned => "tenant_signed",
            Stage::DocumentsReceived => "documents_received",
            Stage::ChecklistCompleted => "checklist_completed",
            Stage::LandlordSigned => "landlord_signed",
            Stage::CopySent => "copy_sent",
            Stage::Registered => "registered",
        }
    }

    /// Название шага для пользователя: ошибки конвейера читают люди,
    /// а не отладчик (печатные формы контура 1 - ru, NFR-01).
    pub fn title_ru(self) -> &'static str {
        match self {
            Stage::Drafted => "договор составлен",
            Stage::HandedToTenant => "экземпляр передан победителю",
            Stage::TenantSigned => "победитель вернул подписанный договор",
            Stage::DocumentsReceived => "документы для сверки представлены",
            Stage::ChecklistCompleted => "сверка документов завершена",
            Stage::LandlordSigned => "договор подписан наймодателем",
            Stage::CopySent => "экземпляр направлен нанимателю",
            Stage::Registered => "договор зарегистрирован",
        }
    }

    /// Пункт Правил, задающий шаг.
    pub fn rule_ref(self) -> &'static str {
        match self {
            Stage::Drafted => "п. 110",
            Stage::HandedToTenant => "п. 110",
            Stage::TenantSigned => "п. 111",
            Stage::DocumentsReceived => "п. 112",
            Stage::ChecklistCompleted => "п. 113",
            Stage::LandlordSigned => "п. 114",
            Stage::CopySent => "п. 115",
            Stage::Registered => "п. 126",
        }
    }

    /// Шаг, который должен быть пройден непосредственно перед этим.
    pub fn previous(self) -> Option<Stage> {
        let index = Stage::ALL.iter().position(|stage| *stage == self)?;
        index.checked_sub(1).map(|prev| Stage::ALL[prev])
    }

    /// Следующий шаг конвейера.
    pub fn next(self) -> Option<Stage> {
        let index = Stage::ALL.iter().position(|stage| *stage == self)?;
        Stage::ALL.get(index + 1).copied()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестный шаг договорного конвейера: {0}")]
pub struct UnknownStage(pub String);

impl std::str::FromStr for Stage {
    type Err = UnknownStage;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Stage::ALL
            .into_iter()
            .find(|stage| stage.as_str() == s)
            .ok_or_else(|| UnknownStage(s.to_owned()))
    }
}

/// Пройденные шаги договора: конвейер идет строго по порядку.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Progress {
    /// Последний пройденный шаг; `None` - договор только создан
    pub current: Option<Stage>,
    /// Сверка документов завершена (п. 113): проверяется отдельно, потому
    /// что ее выполняет другой человек и в свое время
    pub checklist_complete: bool,
}

impl Progress {
    /// Можно ли выполнить шаг сейчас (FR-902): предыдущий пройден, шаг не
    /// повторяется, а подписание наймодателя дополнительно требует
    /// завершенной сверки (INV-115).
    pub fn check(&self, stage: Stage) -> Result<(), StageError> {
        if let Some(current) = self.current
            && stage <= current
        {
            return Err(StageError::AlreadyDone(stage));
        }
        match (stage.previous(), self.current) {
            (Some(required), Some(current)) if current >= required => {}
            (Some(required), _) => return Err(StageError::OutOfOrder { required, stage }),
            (None, _) => {}
        }
        if stage == Stage::LandlordSigned && !self.checklist_complete {
            return Err(StageError::ChecklistIncomplete);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum StageError {
    #[error("шаг «{}» уже пройден", .0.title_ru())]
    AlreadyDone(Stage),
    #[error("сначала «{}» ({}), затем «{}»", required.title_ru(), required.rule_ref(), stage.title_ru())]
    OutOfOrder { required: Stage, stage: Stage },
    #[error(
        "INV-115: наймодатель не подписывает договор без завершенной сверки документов (п. 113, 115)"
    )]
    ChecklistIncomplete,
}

/// Поле договора (FR-901, FR-906, п. 108, 125). Перечень закрыт и разделен
/// надвое: существенные условия защищены и допсоглашением не меняются,
/// остальное Правила менять разрешают.
///
/// TODO-ENGINEER: п. 125 агенту недоступен (Q-017) - состав изменяемых полей
/// заведомо черновой и уточняется вместе со справочником.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractField {
    /// Ставка найма - снимок итогов торгов (FR-901, п. 108)
    MonthlyRate,
    /// Объект найма (FR-901, п. 108)
    Object,
    /// Срок найма (FR-901, п. 108)
    LeaseTerm,
    /// Наниматель - победитель торгов либо заявитель особого порядка (FR-901)
    Tenant,
    /// Банковские реквизиты стороны (п. 125)
    BankDetails,
    /// Почтовый адрес и контактные данные стороны (п. 125)
    ContactDetails,
    /// Уполномоченный представитель стороны (п. 125)
    Representative,
    /// Порядок внесения платы: сроки и способ, но не сама ставка (п. 125)
    PaymentOrder,
}

impl ContractField {
    pub const ALL: [ContractField; 8] = [
        ContractField::MonthlyRate,
        ContractField::Object,
        ContractField::LeaseTerm,
        ContractField::Tenant,
        ContractField::BankDetails,
        ContractField::ContactDetails,
        ContractField::Representative,
        ContractField::PaymentOrder,
    ];

    /// Существенные условия: их не меняет ни приложение, ни допсоглашение
    /// (FR-901, п. 108) - тот же перечень стережет триггер `freeze_terms`.
    pub const PROTECTED: [ContractField; 4] = [
        ContractField::MonthlyRate,
        ContractField::Object,
        ContractField::LeaseTerm,
        ContractField::Tenant,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ContractField::MonthlyRate => "monthly_rate",
            ContractField::Object => "object",
            ContractField::LeaseTerm => "lease_term",
            ContractField::Tenant => "tenant",
            ContractField::BankDetails => "bank_details",
            ContractField::ContactDetails => "contact_details",
            ContractField::Representative => "representative",
            ContractField::PaymentOrder => "payment_order",
        }
    }

    /// Название поля (ru - делопроизводство, NFR-01).
    pub fn title_ru(self) -> &'static str {
        match self {
            ContractField::MonthlyRate => "ставка найма",
            ContractField::Object => "объект найма",
            ContractField::LeaseTerm => "срок найма",
            ContractField::Tenant => "наниматель",
            ContractField::BankDetails => "банковские реквизиты",
            ContractField::ContactDetails => "адрес и контактные данные",
            ContractField::Representative => "уполномоченный представитель",
            ContractField::PaymentOrder => "порядок внесения платы",
        }
    }

    /// Защищено ли поле существенным условием (FR-901).
    pub fn is_protected(self) -> bool {
        ContractField::PROTECTED.contains(&self)
    }

    /// Поля, которые допсоглашение вправе менять (FR-906, п. 125).
    pub fn amendable() -> Vec<ContractField> {
        ContractField::ALL
            .into_iter()
            .filter(|field| !field.is_protected())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестное поле договора: {0}")]
pub struct UnknownField(pub String);

impl std::str::FromStr for ContractField {
    type Err = UnknownField;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ContractField::ALL
            .into_iter()
            .find(|field| field.as_str() == s)
            .ok_or_else(|| UnknownField(s.to_owned()))
    }
}

/// Одна правка допсоглашения (FR-906): что именно и на что меняется.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldChange {
    pub field: ContractField,
    pub old_value: String,
    pub new_value: String,
}

/// Diff-контроль допсоглашения (FR-906, п. 125): допсоглашение без правок
/// не бывает, защищенное условие им не меняется, а «правка» без изменения
/// значения - не правка.
pub fn check_changes(changes: &[FieldChange]) -> Result<(), AmendmentError> {
    if changes.is_empty() {
        return Err(AmendmentError::Empty);
    }
    for change in changes {
        if change.field.is_protected() {
            return Err(AmendmentError::Protected(change.field));
        }
        if change.old_value.trim() == change.new_value.trim() {
            return Err(AmendmentError::Unchanged(change.field));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AmendmentError {
    #[error("допсоглашение без изменений не составляется (FR-906, п. 125)")]
    Empty,
    #[error(
        "FR-901: существенное условие договора ({}) допсоглашением не меняется (п. 108, 125)",
        .0.title_ru()
    )]
    Protected(ContractField),
    #[error("значение поля «{}» не изменилось - правки нет (FR-906)", .0.title_ru())]
    Unchanged(ContractField),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn money(value: &str) -> Money {
        Money::new(value.parse().unwrap_or_default())
    }

    #[test]
    fn essential_terms_come_from_the_auction_outcome() {
        let terms = EssentialTerms::from_auction(money("79750"), 12).expect("условия");
        assert_eq!(terms.monthly_rate(), money("79750"));
        assert_eq!(terms.lease_months(), 12);
        // Депозит по договору равен месячной плате (п. 132)
        assert_eq!(terms.deposit(), money("79750"));
        assert!(terms.matches(money("79750"), 12));
        assert!(
            !terms.matches(money("80000"), 12),
            "ставку подменить нельзя"
        );
    }

    #[test]
    fn terms_reject_impossible_values() {
        assert_eq!(
            EssentialTerms::from_auction(money("0"), 12),
            Err(TermsError::NonPositiveRate)
        );
        assert_eq!(
            EssentialTerms::from_auction(money("1000"), 0),
            Err(TermsError::NonPositiveTerm)
        );
    }

    #[test]
    fn pipeline_runs_in_order() {
        let mut progress = Progress::default();
        assert_eq!(progress.check(Stage::Drafted), Ok(()));
        assert_eq!(
            progress.check(Stage::TenantSigned),
            Err(StageError::OutOfOrder {
                required: Stage::HandedToTenant,
                stage: Stage::TenantSigned
            })
        );

        progress.current = Some(Stage::Drafted);
        assert_eq!(progress.check(Stage::HandedToTenant), Ok(()));
        assert_eq!(
            progress.check(Stage::Drafted),
            Err(StageError::AlreadyDone(Stage::Drafted))
        );
    }

    #[test]
    fn inv115_blocks_landlord_signature_without_the_checklist() {
        let progress = Progress {
            current: Some(Stage::ChecklistCompleted),
            checklist_complete: false,
        };
        assert_eq!(
            progress.check(Stage::LandlordSigned),
            Err(StageError::ChecklistIncomplete)
        );

        let ready = Progress {
            checklist_complete: true,
            ..progress
        };
        assert_eq!(ready.check(Stage::LandlordSigned), Ok(()));
    }

    #[test]
    fn stage_order_and_wire_names_are_stable() {
        assert_eq!(Stage::Drafted.previous(), None);
        assert_eq!(Stage::Registered.next(), None);
        assert_eq!(
            Stage::LandlordSigned.previous(),
            Some(Stage::ChecklistCompleted)
        );

        for stage in Stage::ALL {
            assert_eq!(stage.as_str().parse::<Stage>(), Ok(stage));
            assert!(stage.rule_ref().starts_with("п. "));
            assert!(
                !stage.title_ru().is_empty(),
                "у шага есть человекочитаемое название"
            );
        }
    }
    #[test]
    fn fr901_protected_fields_are_not_amendable() {
        // Существенные условия (п. 108) в перечень изменяемых не попадают
        for field in ContractField::PROTECTED {
            assert!(field.is_protected(), "{field:?} - существенное условие");
        }
        let amendable = ContractField::amendable();
        assert!(!amendable.is_empty(), "п. 125 что-то менять разрешает");
        for field in &amendable {
            assert!(!field.is_protected(), "{field:?} не защищено");
        }
        assert_eq!(
            amendable.len() + ContractField::PROTECTED.len(),
            ContractField::ALL.len(),
            "перечень полей закрыт и разделен надвое"
        );
    }

    #[test]
    fn fr906_diff_control_rejects_protected_and_empty_changes() {
        let change = |field: ContractField, old: &str, new: &str| FieldChange {
            field,
            old_value: old.to_owned(),
            new_value: new.to_owned(),
        };

        assert_eq!(check_changes(&[]), Err(AmendmentError::Empty));
        assert_eq!(
            check_changes(&[change(ContractField::MonthlyRate, "79750", "60000")]),
            Err(AmendmentError::Protected(ContractField::MonthlyRate))
        );
        assert_eq!(
            check_changes(&[change(ContractField::BankDetails, "KZ11", "KZ11")]),
            Err(AmendmentError::Unchanged(ContractField::BankDetails))
        );
        assert_eq!(
            check_changes(&[
                change(ContractField::BankDetails, "KZ11", "KZ22"),
                change(ContractField::Representative, "Иванов", "Петров"),
            ]),
            Ok(())
        );
    }

    #[test]
    fn contract_fields_have_stable_wire_names() {
        let mut names = std::collections::BTreeSet::new();
        for field in ContractField::ALL {
            assert_eq!(field.as_str().parse::<ContractField>(), Ok(field));
            assert!(names.insert(field.as_str()));
            assert!(!field.title_ru().is_empty());
        }
        assert_eq!(
            "rate".parse::<ContractField>(),
            Err(UnknownField("rate".to_owned()))
        );
    }
}
