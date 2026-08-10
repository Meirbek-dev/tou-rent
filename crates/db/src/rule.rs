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
    let rule = RuleViolation::from_message(message)
        .or_else(|| RuleViolation::from_sqlstate(db_err.code().as_deref().unwrap_or_default()))
        .unwrap_or(RuleViolation::OtherRule);
    RuleRejection::new(rule, message)
}
