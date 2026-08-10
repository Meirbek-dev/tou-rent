//! Типы событий центра уведомлений (М13, FR-1301).
//!
//! Каждое процессуальное уведомление - вариант enum'а без catch-all:
//! новое событие получает явное имя на проводе (оно же в
//! `core.notifications.kind`), компилятор требует обработки всех веток
//! при рендере. Каналы доставки - отдельный enum БД
//! `core.notification_channel` (контур 1 - только `in_app`, арх. § 5).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    /// FR-504: приглашение допущенного на второй этап - дата/время/место
    /// торгов и стартовая ставка (= максимум первоначальных предложений
    /// допущенных по лоту, п. 57–59, 62)
    AuctionInvitation,
    /// FR-502, FR-1302 (п. 52, 56): по заявке принято решение комиссии -
    /// участник извещается и об отклонении, и об основании.
    ///
    /// Допущенный узнает о результате приглашением на торги
    /// ([`NotificationKind::AuctionInvitation`]), а отклоненный не узнавал
    /// ничего: решение существовало только в протоколе.
    ApplicationRejected,
    /// FR-1702: срок Правил истек - эскалация исполнителю (п. 54, 57, 73, 75)
    ObligationOverdue,
    /// FR-903: победитель уклонился - договор предлагается участнику № 2;
    /// уведомление не позднее следующего рабочего дня (п. 117–118)
    RunnerUpOffer,
    /// FR-304: опубликована новая редакция документации, срок приема продлен;
    /// участник вправе отказаться с возвратом взноса (п. 26.5, 27)
    TenderAmended,
    /// FR-305: тендер или лот отменен - извещение участников (п. 78–79)
    TenderCancelled,
    /// FR-702, FR-703: протокол опубликован - копия доступна участнику
    /// в кабинете (п. 56, 75)
    ProtocolPublished,
    /// FR-1202: по заявке особого порядка принято решение Правления -
    /// заявитель извещается вместе с обоснованием (п. 90, 97)
    SpecialDecided,
}

impl NotificationKind {
    /// Имя на проводе и в `core.notifications.kind` (паритет с serde -
    /// закреплен тестом).
    pub fn as_str(self) -> &'static str {
        match self {
            NotificationKind::AuctionInvitation => "auction_invitation",
            NotificationKind::ApplicationRejected => "application_rejected",
            NotificationKind::ObligationOverdue => "obligation_overdue",
            NotificationKind::RunnerUpOffer => "runner_up_offer",
            NotificationKind::TenderAmended => "tender_amended",
            NotificationKind::TenderCancelled => "tender_cancelled",
            NotificationKind::ProtocolPublished => "protocol_published",
            NotificationKind::SpecialDecided => "special_decided",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_wire_parity(kind: NotificationKind) {
        assert_eq!(
            serde_json::to_value(kind).unwrap(),
            serde_json::Value::String(kind.as_str().to_owned())
        );
    }

    #[test]
    fn wire_name_matches_serde() {
        // Новый вариант enum'а - новая строка здесь
        assert_wire_parity(NotificationKind::AuctionInvitation);
        assert_wire_parity(NotificationKind::ApplicationRejected);
        assert_wire_parity(NotificationKind::ObligationOverdue);
        assert_wire_parity(NotificationKind::RunnerUpOffer);
        assert_wire_parity(NotificationKind::TenderAmended);
        assert_wire_parity(NotificationKind::TenderCancelled);
        assert_wire_parity(NotificationKind::ProtocolPublished);
        assert_wire_parity(NotificationKind::SpecialDecided);
    }
}
