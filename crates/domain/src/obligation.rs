//! Двигатель обязательств (М17, FR-1702): сроки Правил как данные.
//!
//! Каждое процессуальное событие порождает обязательство с сроком, пунктом
//! Правил и ролью-исполнителем. Каталог закрыт enum'ом: новый срок нельзя
//! «придумать по месту», он появляется здесь вместе со ссылкой на пункт.
//! Отсчет - через [`crate::calendar`] (FR-1701), одинаково с БД.

use serde::{Deserialize, Serialize};

use crate::role::Role;

/// Что именно должно быть сделано (машинный код в `core.obligations.action`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObligationAction {
    /// Оформить протокол допуска после заседания (п. 54)
    AdmissionProtocol,
    /// Уведомить допущенных о втором этапе (п. 57)
    NotifyAdmitted,
    /// Оформить протокол итогов после торгов (п. 73)
    ResultsProtocol,
    /// Опубликовать протокол итогов (п. 75, FR-702)
    PublishResults,
    /// Вернуть гарантийный взнос по основанию п. 26 (FR-1002)
    FeeRefund,
    /// Оформить протокол о несостоявшемся тендере (п. 82, FR-802)
    FailedProtocol,
    /// Составить договор с победителем (15 р. дней, п. 110)
    ContractDraft,
    /// Победитель возвращает подписанный договор (10 р. дней, п. 111)
    TenantSign,
    /// Победитель представляет документы для сверки (7 р. дней, п. 112)
    TenantDocuments,
    /// Наймодатель подписывает договор (2 р. дня, п. 114)
    LandlordSign,
    /// Экземпляр направляется нанимателю (2 р. дня, п. 115)
    ContractHandover,
    /// Протокол о победителе № 2 после уклонения (5 р. дней, п. 117)
    Winner2Protocol,
    /// Уведомление участника № 2 (следующий рабочий день, п. 118)
    NotifyRunnerUp,
    /// Извещение участников об изменении документации (1 р. день, п. 27)
    NotifyAmendment,
    /// Извещение участников об отмене тендера (3 р. дня, п. 79)
    NotifyCancellation,
    /// Заключение подразделения по заявке особого порядка (срок объявляет
    /// категория: 15 к. дней либо 10 р. дней, п. 89, FR-1201)
    SpecialReview,
    /// Решение Правления по заявке особого порядка (10 р. дней, п. 90)
    SpecialDecision,
    /// Публикация результата и обоснования особого порядка (5 р. дней,
    /// п. 97, FR-1202, FR-1403)
    SpecialPublish,
    /// Депозит по договору внесен (10 р. дней от заключения, п. 132, FR-1003)
    DepositPayment,
    /// Депозит восполнен после списания в счет долга (10 р. дней, п. 135)
    DepositTopUp,
    /// Депозит возвращен после возврата объекта без претензий
    /// (5 р. дней, п. 136)
    DepositRefund,
}

impl ObligationAction {
    pub const ALL: [ObligationAction; 21] = [
        ObligationAction::AdmissionProtocol,
        ObligationAction::NotifyAdmitted,
        ObligationAction::ResultsProtocol,
        ObligationAction::PublishResults,
        ObligationAction::FeeRefund,
        ObligationAction::FailedProtocol,
        ObligationAction::ContractDraft,
        ObligationAction::TenantSign,
        ObligationAction::TenantDocuments,
        ObligationAction::LandlordSign,
        ObligationAction::ContractHandover,
        ObligationAction::Winner2Protocol,
        ObligationAction::NotifyRunnerUp,
        ObligationAction::NotifyAmendment,
        ObligationAction::NotifyCancellation,
        ObligationAction::SpecialReview,
        ObligationAction::SpecialDecision,
        ObligationAction::SpecialPublish,
        ObligationAction::DepositPayment,
        ObligationAction::DepositTopUp,
        ObligationAction::DepositRefund,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ObligationAction::AdmissionProtocol => "admission_protocol",
            ObligationAction::NotifyAdmitted => "notify_admitted",
            ObligationAction::ResultsProtocol => "results_protocol",
            ObligationAction::PublishResults => "publish_results",
            ObligationAction::FeeRefund => "fee_refund",
            ObligationAction::FailedProtocol => "failed_protocol",
            ObligationAction::ContractDraft => "contract_draft",
            ObligationAction::TenantSign => "tenant_sign",
            ObligationAction::TenantDocuments => "tenant_documents",
            ObligationAction::LandlordSign => "landlord_sign",
            ObligationAction::ContractHandover => "contract_handover",
            ObligationAction::Winner2Protocol => "winner2_protocol",
            ObligationAction::NotifyRunnerUp => "notify_runner_up",
            ObligationAction::NotifyAmendment => "notify_amendment",
            ObligationAction::NotifyCancellation => "notify_cancellation",
            ObligationAction::SpecialReview => "special_review",
            ObligationAction::SpecialDecision => "special_decision",
            ObligationAction::SpecialPublish => "special_publish",
            ObligationAction::DepositPayment => "deposit_payment",
            ObligationAction::DepositTopUp => "deposit_top_up",
            ObligationAction::DepositRefund => "deposit_refund",
        }
    }

    /// Срок и исполнитель - из Правил, а не из настроек: изменение требует
    /// правки этого файла со ссылкой на пункт.
    pub fn rule(self) -> ObligationRule {
        match self {
            ObligationAction::AdmissionProtocol => ObligationRule {
                action: self,
                rule_ref: "п. 54",
                assignee: Role::Secretary,
                term: Term::BusinessDays(3),
            },
            ObligationAction::NotifyAdmitted => ObligationRule {
                action: self,
                rule_ref: "п. 57",
                assignee: Role::Secretary,
                term: Term::BusinessDays(1),
            },
            ObligationAction::ResultsProtocol => ObligationRule {
                action: self,
                rule_ref: "п. 73",
                assignee: Role::Secretary,
                term: Term::BusinessDays(3),
            },
            ObligationAction::PublishResults => ObligationRule {
                action: self,
                rule_ref: "п. 75",
                assignee: Role::Secretary,
                term: Term::BusinessDays(2),
            },
            // Договорный конвейер (FR-902, п. 110–115): составление и
            // подписание - за организатором (юридическая служба, п. 2.4),
            // возврат подписанного и документы - за победителем
            ObligationAction::ContractDraft => ObligationRule {
                action: self,
                rule_ref: "п. 110",
                assignee: Role::Organizer,
                term: Term::BusinessDays(15),
            },
            ObligationAction::TenantSign => ObligationRule {
                action: self,
                rule_ref: "п. 111",
                assignee: Role::Participant,
                term: Term::BusinessDays(10),
            },
            ObligationAction::TenantDocuments => ObligationRule {
                action: self,
                rule_ref: "п. 112",
                assignee: Role::Participant,
                term: Term::BusinessDays(7),
            },
            ObligationAction::LandlordSign => ObligationRule {
                action: self,
                rule_ref: "п. 114",
                assignee: Role::Organizer,
                term: Term::BusinessDays(2),
            },
            ObligationAction::ContractHandover => ObligationRule {
                action: self,
                rule_ref: "п. 115",
                assignee: Role::Organizer,
                term: Term::BusinessDays(2),
            },
            // Уклонение победителя (FR-903, п. 116–118): протокол о втором
            // месте и его уведомление ведет секретарь комиссии
            ObligationAction::Winner2Protocol => ObligationRule {
                action: self,
                rule_ref: "п. 117",
                assignee: Role::Secretary,
                term: Term::BusinessDays(5),
            },
            ObligationAction::NotifyRunnerUp => ObligationRule {
                action: self,
                rule_ref: "п. 118",
                assignee: Role::Secretary,
                term: Term::BusinessDays(1),
            },
            // Изменение документации и отмена (FR-304, FR-305): извещать
            // участников - обязанность организатора (п. 27, 79)
            ObligationAction::NotifyAmendment => ObligationRule {
                action: self,
                rule_ref: "п. 27",
                assignee: Role::Organizer,
                term: Term::BusinessDays(1),
            },
            ObligationAction::NotifyCancellation => ObligationRule {
                action: self,
                rule_ref: "п. 79",
                assignee: Role::Organizer,
                term: Term::BusinessDays(3),
            },
            // Протокол о несостоявшемся - за секретарем (FR-802, п. 82)
            ObligationAction::FailedProtocol => ObligationRule {
                action: self,
                rule_ref: "п. 82",
                assignee: Role::Secretary,
                term: Term::BusinessDays(3),
            },
            // Особый порядок (FR-1202, п. 89–90): проверку ведет
            // уполномоченное подразделение (в модели ролей - организатор,
            // юридическая служба п. 2.4; A-068), решение принимает Правление.
            // Срок проверки объявляет категория заявки (FR-1201) - здесь
            // общий случай п. 89, 15 календарных дней.
            ObligationAction::SpecialReview => ObligationRule {
                action: self,
                rule_ref: "п. 89",
                assignee: Role::Organizer,
                term: Term::CalendarDays(15),
            },
            ObligationAction::SpecialDecision => ObligationRule {
                action: self,
                rule_ref: "п. 90",
                assignee: Role::Board,
                term: Term::BusinessDays(10),
            },
            // Публикация результата и обоснования (п. 97, FR-1403): решение
            // принимает Правление, а публикует уполномоченное подразделение -
            // оно же ведет процедуру (A-068)
            ObligationAction::SpecialPublish => ObligationRule {
                action: self,
                rule_ref: "п. 97",
                assignee: Role::Organizer,
                term: Term::BusinessDays(5),
            },
            // Депозит по договору (FR-1003, п. 132–136). Исполнитель -
            // департамент финансов: он ведет книгу и подтверждает движение
            // денег, а наниматель узнает о сроке из своего кабинета
            ObligationAction::DepositPayment => ObligationRule {
                action: self,
                rule_ref: "п. 132",
                assignee: Role::Finance,
                term: Term::BusinessDays(10),
            },
            ObligationAction::DepositTopUp => ObligationRule {
                action: self,
                rule_ref: "п. 135",
                assignee: Role::Finance,
                term: Term::BusinessDays(10),
            },
            ObligationAction::DepositRefund => ObligationRule {
                action: self,
                rule_ref: "п. 136",
                assignee: Role::Finance,
                term: Term::BusinessDays(5),
            },
            // Возврат взноса - за департаментом финансов (FR-1002, п. 26)
            ObligationAction::FeeRefund => ObligationRule {
                action: self,
                rule_ref: "п. 26",
                assignee: Role::Finance,
                term: Term::BusinessDays(15),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестное обязательство: {0}")]
pub struct UnknownAction(pub String);

impl std::str::FromStr for ObligationAction {
    type Err = UnknownAction;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ObligationAction::ALL
            .into_iter()
            .find(|action| action.as_str() == s)
            .ok_or_else(|| UnknownAction(s.to_owned()))
    }
}

/// Срок из Правил: рабочие дни (производственный календарь) либо календарные.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Term {
    BusinessDays(u32),
    CalendarDays(u32),
}

impl Term {
    pub fn days(self) -> u32 {
        match self {
            Term::BusinessDays(days) | Term::CalendarDays(days) => days,
        }
    }

    /// Вид дней в значениях `core.term_kind`
    pub fn kind_str(self) -> &'static str {
        match self {
            Term::BusinessDays(_) => "business",
            Term::CalendarDays(_) => "calendar",
        }
    }

    /// Срок, заданный данными: справочники Правил хранят число и вид дней
    /// (FR-1201 - категория особого порядка объявляет свой срок проверки).
    pub fn from_parts(days: u32, kind: &str) -> Option<Self> {
        match kind {
            "business" => Some(Term::BusinessDays(days)),
            "calendar" => Some(Term::CalendarDays(days)),
            _ => None,
        }
    }
}

/// Правило срока: что, к какому времени и с кого спрашивать.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObligationRule {
    pub action: ObligationAction,
    /// Пункт Правил - попадает в `core.obligations.rule_ref` и в дашборд
    pub rule_ref: &'static str,
    pub assignee: Role,
    pub term: Term,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_names_round_trip() {
        for action in ObligationAction::ALL {
            assert_eq!(action.as_str().parse::<ObligationAction>(), Ok(action));
            assert_eq!(
                serde_json::to_value(action).unwrap(),
                serde_json::Value::String(action.as_str().to_owned())
            );
        }
    }

    #[test]
    fn every_rule_cites_a_clause_and_has_an_assignee() {
        for action in ObligationAction::ALL {
            let rule = action.rule();
            assert!(
                rule.rule_ref.starts_with("п. "),
                "{action:?} без ссылки на пункт Правил"
            );
            assert_ne!(rule.assignee, Role::Guest, "{action:?} без исполнителя");
            assert_eq!(rule.action, action);
        }
    }

    #[test]
    fn terms_round_trip_through_refdata_values() {
        for term in [Term::BusinessDays(15), Term::CalendarDays(10)] {
            assert_eq!(Term::from_parts(term.days(), term.kind_str()), Some(term));
        }
        assert_eq!(Term::from_parts(5, "weeks"), None);
    }

    #[test]
    fn protocol_terms_match_the_rules() {
        // п. 54 и п. 73 - три рабочих дня; п. 57 - один; п. 75 - два
        assert_eq!(
            ObligationAction::AdmissionProtocol.rule().term,
            Term::BusinessDays(3)
        );
        assert_eq!(
            ObligationAction::NotifyAdmitted.rule().term,
            Term::BusinessDays(1)
        );
        assert_eq!(
            ObligationAction::ResultsProtocol.rule().term,
            Term::BusinessDays(3)
        );
        assert_eq!(
            ObligationAction::PublishResults.rule().term,
            Term::BusinessDays(2)
        );
        // п. 26 - возврат взноса за 15 рабочих дней силами финблока
        let refund = ObligationAction::FeeRefund.rule();
        assert_eq!(refund.term, Term::BusinessDays(15));
        assert_eq!(refund.assignee, Role::Finance);
    }

    #[test]
    fn evasion_terms_match_the_rules() {
        // п. 117 - протокол о победителе № 2 за пять рабочих дней,
        // п. 118 - уведомление не позднее следующего рабочего дня
        let protocol = ObligationAction::Winner2Protocol.rule();
        assert_eq!(protocol.term, Term::BusinessDays(5));
        assert_eq!(protocol.rule_ref, "п. 117");

        let notice = ObligationAction::NotifyRunnerUp.rule();
        assert_eq!(notice.term, Term::BusinessDays(1));
        assert_eq!(notice.assignee, Role::Secretary);
    }

    #[test]
    fn notice_terms_of_amendment_and_cancellation() {
        // п. 27 - извещение об изменении на следующий рабочий день,
        // п. 79 - извещение об отмене за три рабочих дня; оба за организатором
        let amendment = ObligationAction::NotifyAmendment.rule();
        assert_eq!(amendment.term, Term::BusinessDays(1));
        assert_eq!(amendment.assignee, Role::Organizer);

        let cancellation = ObligationAction::NotifyCancellation.rule();
        assert_eq!(cancellation.term, Term::BusinessDays(3));
        assert_eq!(cancellation.assignee, Role::Organizer);
    }

    #[test]
    fn special_order_publication_takes_five_business_days() {
        // п. 97 (FR-1202, FR-1403): результат и обоснование публикуются
        // за пять рабочих дней силами уполномоченного подразделения (A-068)
        let publish = ObligationAction::SpecialPublish.rule();
        assert_eq!(publish.term, Term::BusinessDays(5));
        assert_eq!(publish.rule_ref, "п. 97");
        assert_eq!(publish.assignee, Role::Organizer);
    }
}
