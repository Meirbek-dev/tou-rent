//! Политика доступа «роль × действие» (ТЗ § 3, INV-POL-01).
//!
//! Единственный источник прав в системе: слой http спрашивает
//! [`is_allowed`] перед каждой операцией. `match` по роли исчерпывающий,
//! без catch-all - новая роль не скомпилируется, пока не описаны ее права.
//! Матрица генерируется тестом в снапшот (`tests/policy_matrix.rs`):
//! изменение прав видно в диффе и требует аппрува инженера (защищенный путь).

use serde::{Deserialize, Serialize};

use crate::role::Role;

/// Действия системы (пополняется по контурам; изменение ломает снапшот матрицы).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    // М1: реестр объектов
    ObjectRead,
    ObjectManage,
    // М2: калькулятор ставок (Прил. 4)
    RateCalculate,
    // М3: тендеры и объявления
    TenderRead,
    TenderManage,
    TenderPublish,
    // М4: заявки и журнал
    ApplicationSubmit,
    ApplicationWithdraw,
    ApplicationReadOwn,
    ApplicationReadAll,
    JournalRead,
    // М5: вскрытие и допуск
    OpeningPerform,
    AdmissionDecide,
    ProtocolGenerate,
    // М6: аукцион
    AuctionManage,
    BidPlace,
    AuctionWatch,
    // М9: договорный конвейер и акты
    /// Чтение договоров тендера и перечня сверки п. 113 (FR-901, FR-902).
    ///
    /// Отдельно от [`Action::TenderRead`] намеренно: тендер публичен
    /// (п. 5-6), а договорный конвейер - нет. Пока чтение договоров
    /// шло по праву чтения тендера, любой зарегистрированный участник
    /// видел по идентификатору тендера имя нанимателя, ставку, номер
    /// регистрации и состояние сверки документов победителя.
    /// Свой договор наниматель читает не этим правом, а участием
    /// в нем (`GET /contracts/my`).
    ContractRead,
    // М10: взносы и депозиты
    FeeConfirm,
    LedgerRead,
    // М11: комиссия (контур 2)
    /// Состав комиссии и его утверждение (FR-1101)
    CommissionManage,
    /// Явка, открытие заседания при кворуме, фиксация отвода (FR-1102, FR-1104)
    MeetingManage,
    /// Декларация об отсутствии конфликта интересов (FR-1104)
    CoiDeclare,
    VoteCast,
    // М12: особый порядок (контур 3)
    BoardDecide,
    // М13: уведомления
    NotificationReadOwn,
    // М15: администрирование
    UserManage,
    RoleGrant,
    RefdataManage,
    // М16: аудит
    AuditRead,
}

impl Action {
    pub const ALL: [Action; 30] = [
        Action::ObjectRead,
        Action::ObjectManage,
        Action::RateCalculate,
        Action::TenderRead,
        Action::TenderManage,
        Action::TenderPublish,
        Action::ApplicationSubmit,
        Action::ApplicationWithdraw,
        Action::ApplicationReadOwn,
        Action::ApplicationReadAll,
        Action::JournalRead,
        Action::OpeningPerform,
        Action::AdmissionDecide,
        Action::ProtocolGenerate,
        Action::AuctionManage,
        Action::BidPlace,
        Action::AuctionWatch,
        Action::ContractRead,
        Action::FeeConfirm,
        Action::LedgerRead,
        Action::CommissionManage,
        Action::MeetingManage,
        Action::CoiDeclare,
        Action::VoteCast,
        Action::BoardDecide,
        Action::NotificationReadOwn,
        Action::UserManage,
        Action::RoleGrant,
        Action::RefdataManage,
        Action::AuditRead,
    ];
}

/// INV-POL-01: право роли на действие. Каждая роль перечислена явно -
/// catch-all по роли запрещен, компилятор требует решения для новой роли.
pub fn is_allowed(role: Role, action: Action) -> bool {
    use Action as A;
    match role {
        // Публичный портал: только чтение открытых данных (п. 5–6)
        Role::Guest => matches!(action, A::ObjectRead | A::TenderRead),

        // Внешний участник (ТЗ § 3): свои заявки, свои торги, свои уведомления
        Role::Participant => matches!(
            action,
            A::ObjectRead
                | A::TenderRead
                | A::ApplicationSubmit
                | A::ApplicationWithdraw
                | A::ApplicationReadOwn
                | A::BidPlace
                | A::AuctionWatch
                | A::NotificationReadOwn
        ),

        // Юридическая служба - организатор тендера (п. 2.4)
        Role::Organizer => matches!(
            action,
            A::ObjectRead
                | A::ObjectManage
                | A::RateCalculate
                | A::TenderRead
                | A::TenderManage
                | A::TenderPublish
                | A::ContractRead
                | A::AuctionWatch
                | A::NotificationReadOwn
        ),

        // Секретарь комиссии (п. 17): журнал, вскрытие, заседания, протоколы
        Role::Secretary => matches!(
            action,
            A::ObjectRead
                | A::TenderRead
                | A::ApplicationReadAll
                | A::JournalRead
                | A::OpeningPerform
                | A::AdmissionDecide
                | A::MeetingManage
                | A::ProtocolGenerate
                | A::ContractRead
                | A::AuctionManage
                | A::AuctionWatch
                | A::NotificationReadOwn
        ),

        // Член комиссии: материалы после вскрытия, голосование (контур 2)
        Role::Commission => matches!(
            action,
            A::ObjectRead
                | A::TenderRead
                | A::ApplicationReadAll
                | A::ContractRead
                | A::AuctionWatch
                | A::CoiDeclare
                | A::VoteCast
                | A::NotificationReadOwn
        ),

        // Правление: особый порядок, спорные случаи (раздел 12, контур 3)
        Role::Board => matches!(
            action,
            A::ObjectRead
                | A::TenderRead
                | A::ContractRead
                | A::BoardDecide
                | A::NotificationReadOwn
        ),

        // Департамент финансов: взносы, депозитная книга
        Role::Finance => matches!(
            action,
            A::ObjectRead
                | A::TenderRead
                | A::ContractRead
                | A::FeeConfirm
                | A::LedgerRead
                | A::NotificationReadOwn
        ),

        // Департамент цифрового развития: пользователи, роли, справочники, аудит.
        // Admin НЕ участвует в тендерном процессе (и не видит цены до вскрытия - INV-040)
        Role::Admin => matches!(
            action,
            A::ObjectRead
                | A::TenderRead
                | A::UserManage
                | A::RoleGrant
                | A::RefdataManage
                | A::CommissionManage
                | A::AuditRead
                | A::NotificationReadOwn
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Договорный конвейер закрыт от участников и гостей: тендер публичен
    /// (п. 5-6), а имя нанимателя, ставка и состояние сверки документов -
    /// нет. Свой договор наниматель читает участием в нем, а не этим правом.
    #[test]
    fn contracts_are_not_readable_by_the_public() {
        for role in [Role::Guest, Role::Participant] {
            assert!(
                !is_allowed(role, Action::ContractRead),
                "{role:?} не должен читать договоры тендера"
            );
        }
        for role in [
            Role::Organizer,
            Role::Secretary,
            Role::Commission,
            Role::Board,
            Role::Finance,
        ] {
            assert!(
                is_allowed(role, Action::ContractRead),
                "{role:?} ведет процесс и обязан видеть договоры"
            );
        }
    }

    #[test]
    fn guest_cannot_mutate_anything() {
        for action in Action::ALL {
            let allowed = is_allowed(Role::Guest, action);
            assert_eq!(
                allowed,
                matches!(action, Action::ObjectRead | Action::TenderRead),
                "guest и {action:?}"
            );
        }
    }

    #[test]
    fn sealed_prices_have_no_reader_before_opening() {
        // INV-040: ApplicationReadAll не дает чтения цен - их прячет RLS;
        // а само действие недоступно organizer и admin
        assert!(!is_allowed(Role::Organizer, Action::ApplicationReadAll));
        assert!(!is_allowed(Role::Admin, Action::ApplicationReadAll));
    }

    #[test]
    fn only_secretary_performs_opening() {
        for role in Role::ALL {
            assert_eq!(
                is_allowed(role, Action::OpeningPerform),
                role == Role::Secretary,
                "{role:?} и вскрытие"
            );
        }
    }

    #[test]
    fn only_finance_confirms_fees() {
        for role in Role::ALL {
            assert_eq!(is_allowed(role, Action::FeeConfirm), role == Role::Finance);
        }
    }

    #[test]
    fn only_admin_grants_roles() {
        for role in Role::ALL {
            assert_eq!(is_allowed(role, Action::RoleGrant), role == Role::Admin);
        }
    }

    #[test]
    fn only_participant_places_bids() {
        for role in Role::ALL {
            assert_eq!(
                is_allowed(role, Action::BidPlace),
                role == Role::Participant
            );
        }
    }
}
