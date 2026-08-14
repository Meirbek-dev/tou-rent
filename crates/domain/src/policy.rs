//! Политика доступа «роль × действие» (ТЗ § 3, INV-POL-01).
//!
//! Единственный источник прав в системе: слой http спрашивает [`is_allowed`]
//! перед каждой операцией и не решает ничего сам. `match` по роли
//! исчерпывающий, без catch-all - новая роль не скомпилируется, пока не
//! описаны ее права.
//!
//! Права разведены по областям ответственности Правил, а не по модулям кода:
//! кто ведет договоры (п. 108–125), кто акты (Прил. 7–8), кто участки
//! (раздел 14), кто рассматривает особый порядок (п. 87–90), кто публикует
//! сведения (п. 97) - у каждой области свое действие. Одно право
//! «организатор может все» матрицу не описывало, а прятало: половина мутаций
//! системы стояла в снимке одной строкой.
//!
//! Область, которую по Правилам ведут несколько ролей, описывается
//! составным правом [`Compound`] («любое из») и тоже живет здесь.
//! Дизъюнкция в обработчике (`require(A).is_ok() || require(B).is_ok()`)
//! запрещена: снимок такой проверки не видит, и матрица доказывает меньше,
//! чем кажется.
//!
//! Матрица - действия и составные права - генерируется тестом в снапшот
//! (`tests/policy_matrix.rs`): изменение прав видно в диффе и требует
//! аппрува инженера (защищенный путь). Что новое разведение прав никому
//! не расширило доступ, стережет `tests/policy_no_widening.rs`.

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
    /// Жизненный цикл самого тендера (FR-301, FR-70x): создание и правка
    /// черновика, изменение объявления, отмена тендера и лота, повторный
    /// тендер после несостоявшегося, видимость неопубликованного.
    ///
    /// Только про тендер. Договоры, акты, участки, особый порядок,
    /// публикации и инвестиционные договоры имеют собственные действия -
    /// раньше все это стояло здесь и в матрице выглядело одной строкой.
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
    /// Ведение договора найма (FR-901–FR-903, п. 108–125): составление по
    /// итогам торгов, сверка п. 113, продвижение состояния, регистрация,
    /// допсоглашения (п. 125), признание уклонения (п. 116) и применение
    /// льготной схемы к договору (п. 95–96).
    ContractManage,
    /// Акты приема-передачи и возврата (FR-904, Прил. 7–8): с даты передачи
    /// начисляется плата (п. 128–129), возврат закрывает договор. Отдельно
    /// от [`Action::ContractManage`]: акт меняет состояние аренды и объекта,
    /// а не условия договора.
    ActManage,
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
    /// Рассмотрение заявки особого порядка уполномоченным подразделением
    /// (FR-1202, п. 89): проверка комплектности и заключение, после которого
    /// вопрос идет Правлению. Решение принимает Правление
    /// ([`Action::BoardDecide`]), а не обладатель этого права.
    SpecialReview,
    /// Инвестиционный договор (FR-1204, п. 91–94): составление по
    /// удовлетворенной заявке и приложения проекта (п. 91). Отдельно от
    /// [`Action::ContractManage`]: договор найма и инвестиционный договор
    /// заключаются по разным основаниям и разными процедурами.
    InvestmentManage,
    /// Публикация сведений о решениях особого порядка (FR-1403, п. 97):
    /// рабочий список ожидающего публикации и сама публикация материала.
    /// Отдельно от [`Action::TenderPublish`]: то - объявление о торгах
    /// (п. 5–6), это - раскрытие решений и обоснований ставок.
    RecordPublish,
    // М14: земельные участки (раздел 14)
    /// Земельные участки (FR-1801, п. 104–107): характеристики участка и их
    /// публикация, договор с особыми условиями (INV-105). Заявки на участки
    /// рассматривает Правление - см. [`Compound::LAND_APPLICATION_REVIEW`].
    LandManage,
    // М13: уведомления
    NotificationReadOwn,
    // М15: администрирование
    UserManage,
    RoleGrant,
    RefdataManage,
    /// Чтение закрытых перечней из Правил (основания отклонения п. 52,
    /// основания возврата п. 26, категории особого порядка п. 87, приложения
    /// п. 91, льготные схемы п. 95–96, производственный календарь).
    ///
    /// Право намеренно широкое - им обладает любая роль: перечни нужны
    /// заявителю и участнику, чтобы заполнить форму и понять отказ, а сами
    /// Правила - открытый документ. Действие заведено, чтобы «достаточно
    /// быть вошедшим» было написано в коде и стояло в матрице, а не
    /// выглядело забытой проверкой.
    RefdataRead,
    // М16: аудит
    AuditRead,
}

impl Action {
    pub const ALL: [Action; 37] = [
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
        Action::ContractManage,
        Action::ActManage,
        Action::FeeConfirm,
        Action::LedgerRead,
        Action::CommissionManage,
        Action::MeetingManage,
        Action::CoiDeclare,
        Action::VoteCast,
        Action::BoardDecide,
        Action::SpecialReview,
        Action::InvestmentManage,
        Action::RecordPublish,
        Action::LandManage,
        Action::NotificationReadOwn,
        Action::UserManage,
        Action::RoleGrant,
        Action::RefdataManage,
        Action::RefdataRead,
        Action::AuditRead,
    ];
}

/// Составное право «любое из действий»: область, которую по Правилам ведут
/// несколько ролей сразу, и ни одно одиночное действие не описывает доступ
/// к ней целиком.
///
/// Живет в домене и попадает в снимок матрицы. Раньше такие места стояли в
/// обработчиках россыпью `require(A).is_ok() || require(B).is_ok()`, и
/// снимок их не видел: матрица выглядела полным описанием прав, оставаясь
/// неполной ровно там, где право сложнее всего.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Compound {
    /// Имя для снимка матрицы и сообщений.
    pub name: &'static str,
    /// Достаточно любого из этих действий.
    pub any_of: &'static [Action],
}

impl Compound {
    /// Тендерный процесс целиком: организатор ведет тендер, секретарь и
    /// комиссия - заявки по нему. Досье тендера (FR-1602, п. 16) и причина
    /// с последствием несостоявшегося (FR-802, п. 82) нужны обеим сторонам:
    /// повторный тендер объявляет организатор, а решение принимала комиссия.
    pub const TENDER_PROCESS_READ: Self = Self {
        name: "TenderProcessRead",
        any_of: &[Action::TenderManage, Action::ApplicationReadAll],
    };

    /// Чужой договор и его допсоглашения: ведет их организатор, а секретарь
    /// и комиссия видят как продолжение материалов торгов. Свой договор
    /// наниматель читает участием в нем, а не этим правом.
    pub const CONTRACT_PROCESS_READ: Self = Self {
        name: "ContractProcessRead",
        any_of: &[Action::ContractManage, Action::ApplicationReadAll],
    };

    /// Реестр договоров (FR-1601): его ведет организатор, а финансы читают
    /// как основание поступлений.
    pub const CONTRACT_REGISTRY_READ: Self = Self {
        name: "ContractRegistryRead",
        any_of: &[Action::ContractManage, Action::LedgerRead],
    };

    /// Заявка особого порядка и решение по ней: заключение готовит
    /// уполномоченное подразделение (п. 89), решает Правление (п. 90).
    /// Обе стороны видят рабочий список, саму заявку, досье решения
    /// (FR-1206, п. 97) и реестр решений (FR-1601).
    pub const SPECIAL_DECISION_ACCESS: Self = Self {
        name: "SpecialDecisionAccess",
        any_of: &[Action::SpecialReview, Action::BoardDecide],
    };

    /// Заявки на земельные участки (п. 105–107): решение принимает
    /// Правление, договор по решению ведет организатор.
    pub const LAND_APPLICATION_REVIEW: Self = Self {
        name: "LandApplicationReview",
        any_of: &[Action::LandManage, Action::BoardDecide],
    };

    /// Льгота по договору (FR-1205, п. 95–96): применяет ее тот, кто ведет
    /// договор, а видят и те, кто ведет особый порядок - льгота следствие
    /// категории заявки.
    pub const BENEFIT_READ: Self = Self {
        name: "BenefitRead",
        any_of: &[Action::ContractManage, Action::BoardDecide],
    };

    /// Инвестиционный договор (п. 91–94, A-072): составляет организатор,
    /// продление оформляет Правление, приемку - секретарь комиссии.
    pub const INVESTMENT_CONTRACT_READ: Self = Self {
        name: "InvestmentContractRead",
        any_of: &[
            Action::InvestmentManage,
            Action::BoardDecide,
            Action::ProtocolGenerate,
        ],
    };

    /// Величина МРП и коэффициенты Прил. 4 (FR-201): их читает всякий, кто
    /// вправе считать ставку, и тот, кто ведет справочник. Значение
    /// показателя закрытым сведением не является.
    pub const RATE_REFDATA_READ: Self = Self {
        name: "RateRefdataRead",
        any_of: &[Action::RateCalculate, Action::RefdataManage],
    };

    pub const ALL: [Compound; 8] = [
        Compound::TENDER_PROCESS_READ,
        Compound::CONTRACT_PROCESS_READ,
        Compound::CONTRACT_REGISTRY_READ,
        Compound::SPECIAL_DECISION_ACCESS,
        Compound::LAND_APPLICATION_REVIEW,
        Compound::BENEFIT_READ,
        Compound::INVESTMENT_CONTRACT_READ,
        Compound::RATE_REFDATA_READ,
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
                | A::RefdataRead
        ),

        // Юридическая служба (п. 2.4): организатор тендера и одновременно
        // уполномоченное подразделение особого порядка (A-068). Ведет тендер,
        // договоры, акты, участки и публикацию решений - каждая область
        // отдельным правом, чтобы объем доступа был виден в матрице.
        Role::Organizer => matches!(
            action,
            A::ObjectRead
                | A::ObjectManage
                | A::RateCalculate
                | A::TenderRead
                | A::TenderManage
                | A::TenderPublish
                | A::ContractRead
                | A::ContractManage
                | A::ActManage
                | A::LandManage
                | A::SpecialReview
                | A::InvestmentManage
                | A::RecordPublish
                | A::AuctionWatch
                | A::NotificationReadOwn
                | A::RefdataRead
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
                | A::RefdataRead
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
                | A::RefdataRead
        ),

        // Правление: особый порядок, спорные случаи (раздел 12, контур 3)
        Role::Board => matches!(
            action,
            A::ObjectRead
                | A::TenderRead
                | A::ContractRead
                | A::BoardDecide
                | A::NotificationReadOwn
                | A::RefdataRead
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
                | A::RefdataRead
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
                | A::RefdataRead
        ),
    }
}

/// Право «любое из перечисленных» (INV-POL-01): единственный разрешенный
/// способ выразить дизъюнкцию прав. Обработчик спрашивает не набор действий,
/// а именованное составное право [`Compound`] - чтобы проверка была видна
/// в снимке матрицы и имела в Правилах обоснование, а не только в коде.
pub fn is_allowed_any(role: Role, actions: &[Action]) -> bool {
    actions.iter().any(|action| is_allowed(role, *action))
}

/// Разрешает ли роли составное право (см. [`Compound`]).
pub fn is_compound_allowed(role: Role, compound: Compound) -> bool {
    is_allowed_any(role, compound.any_of)
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
