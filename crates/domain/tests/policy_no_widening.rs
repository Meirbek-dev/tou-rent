//! Разведение `TenderManage` по областям не расширило доступ ни одной роли
//! (W-08). Главный риск задачи: 41 место проверки, и любое из них легко
//! закрыть не тем правом - тест сравнивает новое распределение с прежним
//! поведением построчно, по каждой роли.
//!
//! Прежнее поведение зафиксировано здесь таблицей, а не берется из кода:
//! до задачи договоры, акты, участки, особый порядок, инвестиционные
//! договоры и публикации охранялись одним `TenderManage`, а шесть
//! справочников - одним фактом входа в систему.

use tou_domain::policy::{Action, Compound, is_allowed, is_allowed_any};
use tou_domain::role::Role;

/// Роль анонимного портала: сессии у нее нет, в `core.role_grants` она не
/// хранится - до обработчика, требующего `CurrentUser`, гость не доходит.
const ANONYMOUS: Role = Role::Guest;

/// Действия, заведенные под области, которые до W-08 охранялись одним
/// `TenderManage`.
const SPLIT_OFF_FROM_TENDER_MANAGE: [Action; 6] = [
    Action::ContractManage,
    Action::ActManage,
    Action::LandManage,
    Action::SpecialReview,
    Action::InvestmentManage,
    Action::RecordPublish,
];

#[test]
fn split_actions_repeat_former_tender_manage_exactly() {
    for action in SPLIT_OFF_FROM_TENDER_MANAGE {
        for role in Role::ALL {
            assert_eq!(
                is_allowed(role, action),
                is_allowed(role, Action::TenderManage),
                "{role:?}: {action:?} должно совпадать с прежним TenderManage - \
                 задача в том, чтобы доступ стал видимым, а не изменился"
            );
        }
    }
}

/// Обратная сторона того же: разведение не должно и отнять. Роль, которая
/// вела область через `TenderManage`, обязана сохранить все шесть прав.
#[test]
fn tender_manage_holder_keeps_every_area() {
    for role in Role::ALL {
        if !is_allowed(role, Action::TenderManage) {
            continue;
        }
        for action in SPLIT_OFF_FROM_TENDER_MANAGE {
            assert!(
                is_allowed(role, action),
                "{role:?} вел {action:?} через TenderManage и не должен был это потерять"
            );
        }
    }
}

/// Справочники п. 26, 52, 87, 91, 95–96 и производственный календарь раньше
/// открывались любому вошедшему. `RefdataRead` это записывает, а не сужает:
/// им обладает каждая роль, кроме анонимного гостя, которого экстрактор
/// `CurrentUser` не пропускал и раньше.
#[test]
fn refdata_read_matches_former_authenticated_only() {
    for role in Role::ALL {
        assert_eq!(
            is_allowed(role, Action::RefdataRead),
            role != ANONYMOUS,
            "{role:?}: справочники Правил были открыты любому вошедшему"
        );
    }
}

/// Составные права повторяют дизъюнкции, которые до W-08 стояли в
/// обработчиках. Пары «составное право - прежнее выражение» перечислены
/// явно: если новое право соберут из других действий, набор ролей разойдется
/// с прежним, и тест это покажет.
#[test]
fn compounds_repeat_former_handler_disjunctions() {
    // Прежние выражения дословно, в терминах действий, существовавших до W-08.
    let former: [(Compound, &[Action]); 8] = [
        // publications.rs: досье тендера; failure.rs: состояние несостоявшегося
        (
            Compound::TENDER_PROCESS_READ,
            &[Action::TenderManage, Action::ApplicationReadAll],
        ),
        // contract_amendments.rs: чужой договор и допсоглашения
        (
            Compound::CONTRACT_PROCESS_READ,
            &[Action::TenderManage, Action::ApplicationReadAll],
        ),
        // reports.rs: реестр договоров
        (
            Compound::CONTRACT_REGISTRY_READ,
            &[Action::TenderManage, Action::LedgerRead],
        ),
        // special.rs, publications.rs, reports.rs: заявки и решения особого порядка
        (
            Compound::SPECIAL_DECISION_ACCESS,
            &[Action::BoardDecide, Action::TenderManage],
        ),
        // land.rs: рабочий список заявок на участки
        (
            Compound::LAND_APPLICATION_REVIEW,
            &[Action::BoardDecide, Action::TenderManage],
        ),
        // benefit.rs: льгота по договору
        (
            Compound::BENEFIT_READ,
            &[Action::TenderManage, Action::BoardDecide],
        ),
        // investment.rs: инвестиционный договор
        (
            Compound::INVESTMENT_CONTRACT_READ,
            &[
                Action::TenderManage,
                Action::BoardDecide,
                Action::ProtocolGenerate,
            ],
        ),
        // refdata.rs: величина МРП и коэффициенты Прил. 4
        (
            Compound::RATE_REFDATA_READ,
            &[Action::RateCalculate, Action::RefdataManage],
        ),
    ];

    assert_eq!(
        former.len(),
        Compound::ALL.len(),
        "каждое составное право обязано иметь запись о прежнем поведении"
    );

    for (compound, before) in former {
        for role in Role::ALL {
            assert_eq!(
                is_allowed_any(role, compound.any_of),
                is_allowed_any(role, before),
                "{role:?} и {}: набор ролей разошелся с прежней дизъюнкцией",
                compound.name
            );
        }
    }
}

/// Составное право осмысленно: собрано минимум из двух действий (иначе это
/// просто действие) и ни одна его часть не мертва - у каждой есть носитель.
/// Мертвая часть - признак того, что дизъюнкцию перенесли не тем действием:
/// набор ролей при этом мог и совпасть, а смысл проверки - нет.
#[test]
fn compound_parts_are_meaningful() {
    for compound in Compound::ALL {
        assert!(
            compound.any_of.len() >= 2,
            "{}: составное право из одного действия - это действие",
            compound.name
        );
        for action in compound.any_of {
            assert!(
                Role::ALL.into_iter().any(|role| is_allowed(role, *action)),
                "{}: часть {action:?} не принадлежит ни одной роли",
                compound.name
            );
        }
    }
}

/// Гость не приобрел ничего: до W-08 ему были открыты объекты и тендеры
/// (п. 5–6), новых прав разведение не дает.
#[test]
fn split_gave_the_public_nothing() {
    for action in Action::ALL {
        assert_eq!(
            is_allowed(ANONYMOUS, action),
            matches!(action, Action::ObjectRead | Action::TenderRead),
            "гость и {action:?}"
        );
    }
    for compound in Compound::ALL {
        assert!(
            !is_allowed_any(ANONYMOUS, compound.any_of),
            "гость и {}",
            compound.name
        );
    }
}

/// Участник торгов не приобрел ничего, кроме права читать перечни Правил,
/// которое у него и так было: ни договоры, ни акты, ни участки, ни особый
/// порядок в его сторону разведением не открылись.
#[test]
fn split_gave_the_participant_nothing_new() {
    for action in SPLIT_OFF_FROM_TENDER_MANAGE {
        assert!(
            !is_allowed(Role::Participant, action),
            "участник и {action:?}"
        );
    }
    for compound in Compound::ALL {
        assert!(
            !is_allowed_any(Role::Participant, compound.any_of),
            "участник и {}",
            compound.name
        );
    }
}
