//! Депозитная книга (М10, FR-1001–1004): типы операций и оснований.
//!
//! Двойная запись: у каждой проводки заполнена ровно одна сторона -
//! поступление и восполнение идут в кредит, удержание, зачет, возврат и
//! списание - в дебет (INV-DB-05, закреплено CHECK'ами БД). Направление
//! задается типом операции, а не вызывающим кодом: «возврат в кредит»
//! невозможно написать даже по ошибке.

use serde::{Deserialize, Serialize};

/// Тип счета (паритет с enum БД `core.ledger_account_kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    /// Гарантийный взнос участника по лоту (п. 22, 25)
    ParticipantFee,
    /// Депозит по договору = месячная плата (п. 132–136)
    ContractDeposit,
}

impl AccountKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AccountKind::ParticipantFee => "participant_fee",
            AccountKind::ContractDeposit => "contract_deposit",
        }
    }
}

/// Сторона проводки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Деньги пришли на счет
    Credit,
    /// Деньги ушли со счета
    Debit,
}

/// Операция депозитной книги (паритет с enum БД `core.ledger_op`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerOp {
    /// Поступление подтверждено оператором финблока вручную (FR-405, п. 23)
    ReceiptConfirmed,
    /// Удержание взноса (уклонение победителя, п. 116)
    Hold,
    /// Зачет в счет депозита или арендной платы (п. 26.6, 133)
    Offset,
    /// Возврат участнику по основанию п. 26 (FR-1002)
    Refund,
    /// Списание в счет долга по договору (п. 134)
    Writeoff,
    /// Восполнение депозита после списания (п. 135)
    Replenish,
}

impl LedgerOp {
    pub const ALL: [LedgerOp; 6] = [
        LedgerOp::ReceiptConfirmed,
        LedgerOp::Hold,
        LedgerOp::Offset,
        LedgerOp::Refund,
        LedgerOp::Writeoff,
        LedgerOp::Replenish,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            LedgerOp::ReceiptConfirmed => "receipt_confirmed",
            LedgerOp::Hold => "hold",
            LedgerOp::Offset => "offset",
            LedgerOp::Refund => "refund",
            LedgerOp::Writeoff => "writeoff",
            LedgerOp::Replenish => "replenish",
        }
    }

    /// Сторона проводки - свойство операции (CHECK `op_direction` в БД).
    pub fn side(self) -> Side {
        match self {
            LedgerOp::ReceiptConfirmed | LedgerOp::Replenish => Side::Credit,
            LedgerOp::Hold | LedgerOp::Offset | LedgerOp::Refund | LedgerOp::Writeoff => {
                Side::Debit
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестная операция книги: {0}")]
pub struct UnknownOp(pub String);

impl std::str::FromStr for LedgerOp {
    type Err = UnknownOp;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        LedgerOp::ALL
            .into_iter()
            .find(|op| op.as_str() == s)
            .ok_or_else(|| UnknownOp(s.to_owned()))
    }
}

/// Основание возврата гарантийного взноса (FR-1002, п. 26) - закрытый
/// перечень из шести случаев, без catch-all.
///
/// TODO-ENGINEER: формулировки и нумерация подпунктов п. 26 выверяются по
/// Правилам (Q-003); коды и состав перечня зафиксированы по ТЗ («шесть
/// случаев»), тексты в `refdata.refund_reasons` помечены как черновые.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefundReason {
    /// Заявка отозвана до окончания срока приема (п. 43–45)
    ApplicationWithdrawn,
    /// Тендер признан несостоявшимся либо отменен (п. 78–83)
    TenderFailed,
    /// Участник не допущен по итогам первого этапа (п. 52)
    NotAdmitted,
    /// Участник не стал ни победителем, ни вторым местом (п. 74)
    NotWinner,
    /// Условия тендера изменены, участник отказался от участия (п. 26.5, FR-1004)
    TermsChanged,
    /// Договор заключен: взнос возвращается либо засчитывается (п. 26.6)
    ContractSigned,
}

impl RefundReason {
    pub const ALL: [RefundReason; 6] = [
        RefundReason::ApplicationWithdrawn,
        RefundReason::TenderFailed,
        RefundReason::NotAdmitted,
        RefundReason::NotWinner,
        RefundReason::TermsChanged,
        RefundReason::ContractSigned,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            RefundReason::ApplicationWithdrawn => "application_withdrawn",
            RefundReason::TenderFailed => "tender_failed",
            RefundReason::NotAdmitted => "not_admitted",
            RefundReason::NotWinner => "not_winner",
            RefundReason::TermsChanged => "terms_changed",
            RefundReason::ContractSigned => "contract_signed",
        }
    }

    /// Взнос победителя и второго места не возвращается по общим основаниям:
    /// он держится до заключения договора (п. 26, 116) - при уклонении
    /// удерживается, при подписании засчитывается.
    pub fn applies_to_winner(self) -> bool {
        matches!(
            self,
            RefundReason::TenderFailed | RefundReason::ContractSigned
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестное основание возврата: {0}")]
pub struct UnknownRefundReason(pub String);

impl std::str::FromStr for RefundReason {
    type Err = UnknownRefundReason;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        RefundReason::ALL
            .into_iter()
            .find(|reason| reason.as_str() == s)
            .ok_or_else(|| UnknownRefundReason(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_names_round_trip() {
        for op in LedgerOp::ALL {
            assert_eq!(op.as_str().parse::<LedgerOp>(), Ok(op));
            assert_eq!(
                serde_json::to_value(op).unwrap(),
                serde_json::Value::String(op.as_str().to_owned())
            );
        }
        for reason in RefundReason::ALL {
            assert_eq!(reason.as_str().parse::<RefundReason>(), Ok(reason));
        }
    }

    #[test]
    fn receipts_credit_and_payouts_debit() {
        // Направление - свойство операции, а не аргумент вызова (INV-DB-05)
        assert_eq!(LedgerOp::ReceiptConfirmed.side(), Side::Credit);
        assert_eq!(LedgerOp::Replenish.side(), Side::Credit);
        for op in [
            LedgerOp::Hold,
            LedgerOp::Offset,
            LedgerOp::Refund,
            LedgerOp::Writeoff,
        ] {
            assert_eq!(op.side(), Side::Debit, "{op:?}");
        }
    }

    #[test]
    fn refund_list_is_closed_and_sized_by_the_rules() {
        // п. 26 - шесть случаев (ТЗ FR-1002)
        assert_eq!(RefundReason::ALL.len(), 6);
        assert!("не_основание".parse::<RefundReason>().is_err());
    }

    #[test]
    fn winner_fee_is_not_returned_on_common_grounds() {
        assert!(!RefundReason::NotAdmitted.applies_to_winner());
        assert!(!RefundReason::NotWinner.applies_to_winner());
        assert!(RefundReason::ContractSigned.applies_to_winner());
        assert!(RefundReason::TenderFailed.applies_to_winner());
    }
}
