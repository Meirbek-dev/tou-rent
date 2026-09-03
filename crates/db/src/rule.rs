//! Разбор отказа PostgreSQL в причину перечня [`RuleViolation`].
//!
//! Каждый модуль этого слоя решает сам, какие SQLSTATE считает отказом по
//! правилу, а какие - поломкой (у журнала обязательств нет пересекающихся
//! периодов, у договоров есть). Общей стала только вторая половина работы -
//! перевод опознанного отказа в причину: раньше на ее месте стояло
//! `db_err.message().to_owned()`, и русский текст триггера ехал до экрана.

use tou_domain::rule::{RuleRejection, RuleViolation};

/// Причина отказа по сообщению триггера и коду SQLSTATE.
///
/// Порядок важен: сообщение точнее кода. `INV-063: ставка ... ниже
/// минимально допустимой` приходит с ERRCODE `check_violation`, и по одному
/// коду отличить его от нарушения любого другого CHECK нельзя.
pub(crate) fn rejection(db_err: &dyn sqlx::error::DatabaseError) -> RuleRejection {
    let message = db_err.message();
    // Имя ограничения - раньше SQLSTATE и позже сообщения: у CHECK таблицы
    // текста правила нет вовсе, и имя остается единственным его признаком
    // (`deadline_before_opening` вместо безымянного `23514`).
    let constraint = db_err.constraint();
    let rule = RuleViolation::from_message(message)
        .or_else(|| constraint.and_then(RuleViolation::from_constraint))
        .or_else(|| RuleViolation::from_sqlstate(db_err.code().as_deref().unwrap_or_default()))
        .unwrap_or(RuleViolation::OtherRule);

    // Отказ по CHECK формулирует сам PostgreSQL, и в тексте остается одно
    // имя ограничения: «violates check constraint "opening_before_trading"».
    // Дежурному этого мало - какие именно две отметки разошлись, из имени
    // читается не сразу (G-100). Пара называется словами здесь, в описании
    // для телеметрии; причина перечня при этом остается одна на все четыре
    // ограничения порядка - наружу уходит она, а не этот текст (NFR-07).
    match constraint.and_then(schedule_pair) {
        Some(pair) => RuleRejection::new(rule, format!("{message} ({pair})")),
        None => RuleRejection::new(rule, message),
    }
}

/// Пара сроков, разошедшаяся в нарушенном ограничении (FR-303, п. 5–6, 36, 50, 59).
fn schedule_pair(constraint: &str) -> Option<&'static str> {
    match constraint {
        "deadline_before_opening" => Some("окончание приема заявок позже вскрытия конвертов"),
        "opened_not_before_meeting" => Some("вскрытие раньше назначенного заседания"),
        "announced_before_deadline" => {
            Some("публикация объявления не раньше окончания приема заявок")
        }
        "opening_before_trading" => Some("вскрытие конвертов позже торгов"),
        _ => None,
    }
}
