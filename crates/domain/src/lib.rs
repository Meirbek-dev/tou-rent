//! Чистое ядро TOU.Rent (арх. § 4): типы, деньги, роли, время.
//!
//! Правила слоя: никакого IO, никаких паник (G2), время - только через
//! абстракцию [`clock::Clock`] (регламент А.5). Каждый инвариант ТЗ
//! закрепляется на самом нижнем достижимом уровне: тип → constraint БД → тест.

pub mod act;
pub mod amendment;
pub mod auction;
pub mod benefit;
pub mod calendar;
pub mod clock;
pub mod commission;
pub mod contract;
pub mod evasion;
pub mod failure;
pub mod identity;
pub mod ids;
pub mod investment;
pub mod land;
pub mod ledger;
pub mod money;
pub mod notification;
pub mod obligation;
pub mod policy;
pub mod publication;
pub mod rates;
pub mod redacted;
pub mod report;
pub mod role;
pub mod rule;
pub mod signing;
pub mod special;
pub mod tender;
pub mod turn;
