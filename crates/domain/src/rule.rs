//! Причины отказа по правилам предметной области - закрытый перечень (NFR-01).
//!
//! Отказ по Правилам университета - самый частый ответ системы, и до сих пор
//! единственным его носителем был текст `RAISE EXCEPTION` из триггера БД. Он
//! существует только по-русски, мимо Paraglide: участник с локалью kk или en
//! получал русскую строку ровно там, где ему важнее всего понять причину, -
//! а вместе с ней наружу уезжали имена инвариантов и значения полей (NFR-07).
//!
//! Поэтому причина отказа - не строка, а вариант [`RuleViolation`]: он уходит
//! клиенту отдельным полем problem+json, интерфейс подставляет по нему свой
//! перевод, а исходное сообщение БД остается в телеметрии дежурному.
//!
//! Перечень закрыт: catch-all один и назван честно - [`RuleViolation::OtherRule`],
//! «прочее правило». Он существует не для удобства, а потому что триггеров
//! больше сотни и не у каждого сообщения есть машинный признак; каждое
//! попадание в него - повод завести правилу собственный вариант.

use serde::{Deserialize, Serialize};

/// Причина, по которой операция отклонена правилом (ответ 409, `rule_violation`).
///
/// Имена вариантов - предметные, а не номера пунктов: клиент подставляет по
/// ним перевод, и `bid_below_minimum` читается и в контракте, и в каталоге
/// сообщений, тогда как `inv_063` не читается нигде.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleViolation {
    /// INV-021: переход статуса тендера запрещен таблицей переходов (п. 5–11)
    TenderStatusTransition,
    /// FR-303: тендер публикуется с лотами и сроками, вскрытие - не раньше
    /// чем через 10 календарных дней
    TenderPublicationTerms,
    /// FR-304: документация и сроки приема заявок изменяются не всегда (п. 27)
    TenderDocumentationChange,
    /// FR-305: тендер или лот отменяется до заключения договора (п. 78)
    TenderCancellation,
    /// FR-801, FR-802: тендер признается несостоявшимся по основанию п. 81
    TenderFailureGround,
    /// Прием заявок по тендеру не открыт (п. 36)
    ApplicationIntakeClosed,
    /// INV-037: срок приема заявок истек (п. 37–39)
    ApplicationDeadlinePassed,
    /// Заявка на этот лот от участника уже подана (п. 22)
    ApplicationAlreadySubmitted,
    /// Заявка не в статусе «подана»: отзывать или решать по ней нечего (FR-404)
    ApplicationNotPending,
    /// INV-040: ценовое предложение не записывается без ключа шифрования (п. 40)
    SealedPriceKeyMissing,
    /// FR-1101: состав комиссии - председатель, заместитель, нечетное число
    /// голосующих от семи (п. 9, 16–17)
    CommissionComposition,
    /// FR-1102: заседание комиссии, кворум, председательствующий (п. 12)
    CommissionMeeting,
    /// FR-1103, FR-1104: право голоса и порядок голосования (п. 13, 15)
    CommissionVote,
    /// FR-504: извещение допущенных - после протокола допуска и однократно
    AdmissionNotice,
    /// Торги не идут: аукцион не в статусе running (п. 60–70)
    AuctionNotRunning,
    /// INV-062: стартовая ставка не определена - нет допущенных заявок
    /// с ценовым предложением (п. 62)
    AuctionStartPriceMissing,
    /// INV-063: ставка ниже максимума плюс шаг (п. 63)
    BidBelowMinimum,
    /// INV-066: таймер торгов - время истекло либо продление уже было (п. 66, 68)
    AuctionTimer,
    /// FR-604: сейчас ход другого участника либо участник выбыл (п. 65)
    AuctionTurnOrder,
    /// FR-605: оглашение предложения возможно не всегда (п. 70)
    AuctionAnnouncement,
    /// FR-606: победитель или второе место не относятся к этому аукциону
    AuctionResultMismatch,
    /// FR-701: протокол итогов оформляется по завершенным торгам лота
    ResultProtocol,
    /// FR-702: публикация протокола (п. 75)
    ProtocolPublication,
    /// INV-076: срок публичного доступа - 6 месяцев, снятие необратимо (п. 76)
    PublicationRetention,
    /// FR-306: ссылка на запись в реестре вносится после итогов торгов (п. 72)
    PublicRecordLink,
    /// INV-042: материалы досье не переписываются и не отвязываются (п. 16.15, 42)
    DossierImmutable,
    /// Договор составляется по завершенным торгам и победителю лота (п. 74, 108)
    ContractConclusion,
    /// FR-902: шаги договорного конвейера идут по порядку (п. 110–115)
    ContractStageOrder,
    /// FR-901: существенные условия договора неизменяемы (п. 108, 125)
    ContractTermsImmutable,
    /// FR-903: признание уклонения и договор с участником № 2 (п. 110–117)
    WinnerEvasion,
    /// FR-904: порядок актов приема-передачи и возврата (п. 126, 129)
    ActOrder,
    /// FR-905: регистрация договора - по подписи обеих сторон и с номером
    /// в журнале (п. 126)
    ContractRegistration,
    /// FR-906: допсоглашение к зарегистрированному договору, неизменяемо (п. 125)
    ContractAmendment,
    /// INV-115: сверка документов не завершена (п. 113, 115)
    DocumentCheckIncomplete,
    /// Депозит по договору: срок внесения, размер, возврат (п. 132, 136)
    ContractDeposit,
    /// FR-405: подтверждение и возврат гарантийного взноса заявки (п. 43–45)
    GuaranteeDeposit,
    /// FR-1002, FR-1003: возврат взноса - только по основанию из перечня п. 26
    DepositRefundReason,
    /// Проводка депозитной книги некорректна (знак, вид операции)
    LedgerEntry,
    /// INV-DB-05: проводка уводит баланс счета в минус
    LedgerBalanceNegative,
    /// FR-1203: состав заявки особого порядка (п. 87–88, 97)
    SpecialOrderApplication,
    /// FR-1201: переход статуса заявки особого порядка запрещен (п. 88–90)
    SpecialOrderTransition,
    /// INV-086: по категории есть конкурирующие заявки - общий порядок (п. 86)
    SpecialOrderCompetition,
    /// FR-1202: решение Правления - по вынесенной заявке и неизменяемо (п. 90)
    BoardDecision,
    /// INV-090: решение Правления невозможно без заключения подразделения (п. 89–90)
    BoardDecisionWithoutOpinion,
    /// FR-1204: инвестиционный договор, акт приемки, продление (п. 90–93)
    InvestmentContract,
    /// INV-091: не приложены обязательные документы проекта (п. 91)
    InvestmentDocumentsMissing,
    /// FR-1205: условия льготной схемы (п. 95)
    BenefitScheme,
    /// INV-095: льгота применяется по согласованию Ученого совета (п. 95)
    BenefitApprovalMissing,
    /// INV-096: спин-офф не закрывает учебную квоту (п. 96)
    SpinoffTeachingQuota,
    /// FR-1403: публикации особого порядка (п. 92, 97)
    SpecialPublication,
    /// FR-1801: заявка на земельный участок и решение по ней (п. 104–107)
    LandApplication,
    /// INV-105: в договоре на участок не закреплены особые условия (п. 107)
    LandContractTermsMissing,
    /// FR-101: объект занят лотами или договорами
    ObjectInUse,
    /// Действие недоступно в текущем статусе записи
    StatusNotAllowed,
    /// Таблица доступна только на добавление (журналы, цепочка аудита)
    AppendOnlyTable,
    /// INV-DB-02: период пересекается с уже существующим (EXCLUDE, 23P01)
    OverlappingPeriod,
    /// Такая запись уже есть (UNIQUE, 23505)
    DuplicateRecord,
    /// Связанной записи нет либо она относится к другому объекту (FK, 23503)
    RelatedRecordMissing,
    /// Прочее правило: отказ опознан как правило домена, но своего варианта
    /// в перечне у него пока нет. Не «неизвестная ошибка» - именно правило.
    OtherRule,
}

impl RuleViolation {
    /// Все причины перечня - источник схемы контракта и полноты каталога
    /// переводов. Новый вариант обязан появиться и здесь (тест ниже следит).
    pub const ALL: &'static [RuleViolation] = &[
        RuleViolation::TenderStatusTransition,
        RuleViolation::TenderPublicationTerms,
        RuleViolation::TenderDocumentationChange,
        RuleViolation::TenderCancellation,
        RuleViolation::TenderFailureGround,
        RuleViolation::ApplicationIntakeClosed,
        RuleViolation::ApplicationDeadlinePassed,
        RuleViolation::ApplicationAlreadySubmitted,
        RuleViolation::ApplicationNotPending,
        RuleViolation::SealedPriceKeyMissing,
        RuleViolation::CommissionComposition,
        RuleViolation::CommissionMeeting,
        RuleViolation::CommissionVote,
        RuleViolation::AdmissionNotice,
        RuleViolation::AuctionNotRunning,
        RuleViolation::AuctionStartPriceMissing,
        RuleViolation::BidBelowMinimum,
        RuleViolation::AuctionTimer,
        RuleViolation::AuctionTurnOrder,
        RuleViolation::AuctionAnnouncement,
        RuleViolation::AuctionResultMismatch,
        RuleViolation::ResultProtocol,
        RuleViolation::ProtocolPublication,
        RuleViolation::PublicationRetention,
        RuleViolation::PublicRecordLink,
        RuleViolation::DossierImmutable,
        RuleViolation::ContractConclusion,
        RuleViolation::ContractStageOrder,
        RuleViolation::ContractTermsImmutable,
        RuleViolation::WinnerEvasion,
        RuleViolation::ActOrder,
        RuleViolation::ContractRegistration,
        RuleViolation::ContractAmendment,
        RuleViolation::DocumentCheckIncomplete,
        RuleViolation::ContractDeposit,
        RuleViolation::GuaranteeDeposit,
        RuleViolation::DepositRefundReason,
        RuleViolation::LedgerEntry,
        RuleViolation::LedgerBalanceNegative,
        RuleViolation::SpecialOrderApplication,
        RuleViolation::SpecialOrderTransition,
        RuleViolation::SpecialOrderCompetition,
        RuleViolation::BoardDecision,
        RuleViolation::BoardDecisionWithoutOpinion,
        RuleViolation::InvestmentContract,
        RuleViolation::InvestmentDocumentsMissing,
        RuleViolation::BenefitScheme,
        RuleViolation::BenefitApprovalMissing,
        RuleViolation::SpinoffTeachingQuota,
        RuleViolation::SpecialPublication,
        RuleViolation::LandApplication,
        RuleViolation::LandContractTermsMissing,
        RuleViolation::ObjectInUse,
        RuleViolation::StatusNotAllowed,
        RuleViolation::AppendOnlyTable,
        RuleViolation::OverlappingPeriod,
        RuleViolation::DuplicateRecord,
        RuleViolation::RelatedRecordMissing,
        RuleViolation::OtherRule,
    ];

    /// Имя на проводе (паритет с serde закреплен тестом).
    pub fn as_str(self) -> &'static str {
        match self {
            RuleViolation::TenderStatusTransition => "tender_status_transition",
            RuleViolation::TenderPublicationTerms => "tender_publication_terms",
            RuleViolation::TenderDocumentationChange => "tender_documentation_change",
            RuleViolation::TenderCancellation => "tender_cancellation",
            RuleViolation::TenderFailureGround => "tender_failure_ground",
            RuleViolation::ApplicationIntakeClosed => "application_intake_closed",
            RuleViolation::ApplicationDeadlinePassed => "application_deadline_passed",
            RuleViolation::ApplicationAlreadySubmitted => "application_already_submitted",
            RuleViolation::ApplicationNotPending => "application_not_pending",
            RuleViolation::SealedPriceKeyMissing => "sealed_price_key_missing",
            RuleViolation::CommissionComposition => "commission_composition",
            RuleViolation::CommissionMeeting => "commission_meeting",
            RuleViolation::CommissionVote => "commission_vote",
            RuleViolation::AdmissionNotice => "admission_notice",
            RuleViolation::AuctionNotRunning => "auction_not_running",
            RuleViolation::AuctionStartPriceMissing => "auction_start_price_missing",
            RuleViolation::BidBelowMinimum => "bid_below_minimum",
            RuleViolation::AuctionTimer => "auction_timer",
            RuleViolation::AuctionTurnOrder => "auction_turn_order",
            RuleViolation::AuctionAnnouncement => "auction_announcement",
            RuleViolation::AuctionResultMismatch => "auction_result_mismatch",
            RuleViolation::ResultProtocol => "result_protocol",
            RuleViolation::ProtocolPublication => "protocol_publication",
            RuleViolation::PublicationRetention => "publication_retention",
            RuleViolation::PublicRecordLink => "public_record_link",
            RuleViolation::DossierImmutable => "dossier_immutable",
            RuleViolation::ContractConclusion => "contract_conclusion",
            RuleViolation::ContractStageOrder => "contract_stage_order",
            RuleViolation::ContractTermsImmutable => "contract_terms_immutable",
            RuleViolation::WinnerEvasion => "winner_evasion",
            RuleViolation::ActOrder => "act_order",
            RuleViolation::ContractRegistration => "contract_registration",
            RuleViolation::ContractAmendment => "contract_amendment",
            RuleViolation::DocumentCheckIncomplete => "document_check_incomplete",
            RuleViolation::ContractDeposit => "contract_deposit",
            RuleViolation::GuaranteeDeposit => "guarantee_deposit",
            RuleViolation::DepositRefundReason => "deposit_refund_reason",
            RuleViolation::LedgerEntry => "ledger_entry",
            RuleViolation::LedgerBalanceNegative => "ledger_balance_negative",
            RuleViolation::SpecialOrderApplication => "special_order_application",
            RuleViolation::SpecialOrderTransition => "special_order_transition",
            RuleViolation::SpecialOrderCompetition => "special_order_competition",
            RuleViolation::BoardDecision => "board_decision",
            RuleViolation::BoardDecisionWithoutOpinion => "board_decision_without_opinion",
            RuleViolation::InvestmentContract => "investment_contract",
            RuleViolation::InvestmentDocumentsMissing => "investment_documents_missing",
            RuleViolation::BenefitScheme => "benefit_scheme",
            RuleViolation::BenefitApprovalMissing => "benefit_approval_missing",
            RuleViolation::SpinoffTeachingQuota => "spinoff_teaching_quota",
            RuleViolation::SpecialPublication => "special_publication",
            RuleViolation::LandApplication => "land_application",
            RuleViolation::LandContractTermsMissing => "land_contract_terms_missing",
            RuleViolation::ObjectInUse => "object_in_use",
            RuleViolation::StatusNotAllowed => "status_not_allowed",
            RuleViolation::AppendOnlyTable => "append_only_table",
            RuleViolation::OverlappingPeriod => "overlapping_period",
            RuleViolation::DuplicateRecord => "duplicate_record",
            RuleViolation::RelatedRecordMissing => "related_record_missing",
            RuleViolation::OtherRule => "other_rule",
        }
    }

    /// Причина по тексту отказа БД.
    ///
    /// Триггеры уже пишут идентификатор правила первым словом сообщения
    /// (`INV-063: ставка ... ниже минимально допустимой ...`), и это
    /// единственный машинный признак, который у примененных миграций есть:
    /// переписывать их ради нового поля нельзя (контрольная сумма sqlx),
    /// а идентификатор для разбора годится не хуже.
    ///
    /// Одному варианту перечня отвечает несколько идентификаторов там, где
    /// правила говорят об одном и том же (FR-1103 и FR-1104 - про голос,
    /// FR-1002 и FR-1003 - про основание возврата взноса).
    pub fn from_message(message: &str) -> Option<RuleViolation> {
        // Запрет мутации append-only таблицы опознается раньше идентификатора:
        // `core.forbid_mutation` подставляет первым словом идентификатор самой
        // таблицы (`INV-037` у журнала заявок, `INV-063` у ставок), и по
        // одному этому слову отказ не отличить от нарушения того же правила
        // при вставке - а это разные отказы и разные подсказки пользователю.
        if message.contains("append-only") {
            return Some(RuleViolation::AppendOnlyTable);
        }

        // Разделитель ищется в начале строки: дальше двоеточие встречается
        // и внутри подставленных значений («(текущий: running)»).
        let token = message.split(':').next()?;
        let rule = match token {
            "INV-021" => RuleViolation::TenderStatusTransition,
            "FR-303" => RuleViolation::TenderPublicationTerms,
            "FR-304" => RuleViolation::TenderDocumentationChange,
            "FR-305" => RuleViolation::TenderCancellation,
            "FR-801" | "FR-802" => RuleViolation::TenderFailureGround,
            "INV-037" => RuleViolation::ApplicationDeadlinePassed,
            "INV-040" => RuleViolation::SealedPriceKeyMissing,
            "FR-1101" => RuleViolation::CommissionComposition,
            "FR-1102" => RuleViolation::CommissionMeeting,
            "FR-1103" | "FR-1104" => RuleViolation::CommissionVote,
            "FR-504" => RuleViolation::AdmissionNotice,
            "INV-062" => RuleViolation::AuctionStartPriceMissing,
            "INV-063" => RuleViolation::BidBelowMinimum,
            "INV-066" => RuleViolation::AuctionTimer,
            "FR-604" => RuleViolation::AuctionTurnOrder,
            "FR-605" => RuleViolation::AuctionAnnouncement,
            "FR-606" => RuleViolation::AuctionResultMismatch,
            "FR-701" => RuleViolation::ResultProtocol,
            "FR-702" => RuleViolation::ProtocolPublication,
            "INV-076" => RuleViolation::PublicationRetention,
            "FR-306" => RuleViolation::PublicRecordLink,
            "INV-042" => RuleViolation::DossierImmutable,
            "FR-902" => RuleViolation::ContractStageOrder,
            "FR-901" => RuleViolation::ContractTermsImmutable,
            "FR-903" => RuleViolation::WinnerEvasion,
            "FR-904" => RuleViolation::ActOrder,
            "FR-905" => RuleViolation::ContractRegistration,
            "FR-906" => RuleViolation::ContractAmendment,
            "INV-115" => RuleViolation::DocumentCheckIncomplete,
            "FR-405" => RuleViolation::GuaranteeDeposit,
            "FR-1002" | "FR-1003" => RuleViolation::DepositRefundReason,
            "INV-DB-05" => RuleViolation::LedgerBalanceNegative,
            "FR-1203" => RuleViolation::SpecialOrderApplication,
            "FR-1201" => RuleViolation::SpecialOrderTransition,
            "INV-086" => RuleViolation::SpecialOrderCompetition,
            "FR-1202" => RuleViolation::BoardDecision,
            "INV-090" => RuleViolation::BoardDecisionWithoutOpinion,
            "FR-1204" => RuleViolation::InvestmentContract,
            "INV-091" => RuleViolation::InvestmentDocumentsMissing,
            "FR-1205" => RuleViolation::BenefitScheme,
            "INV-095" => RuleViolation::BenefitApprovalMissing,
            "INV-096" => RuleViolation::SpinoffTeachingQuota,
            "FR-1403" => RuleViolation::SpecialPublication,
            "FR-1801" => RuleViolation::LandApplication,
            "INV-105" => RuleViolation::LandContractTermsMissing,
            "FR-101" => RuleViolation::ObjectInUse,
            // Триггер ставок писался до того, как у сообщений появился
            // машинный признак, и идентификатора у него нет. Опознается по
            // началу текста: строка литеральная и поднимается в одном месте.
            "ставка отклонена" => RuleViolation::AuctionNotRunning,
            _ => return None,
        };
        Some(rule)
    }

    /// Причина по имени нарушенного ограничения.
    ///
    /// CHECK таблицы, в отличие от триггера, сообщения не пишет: PostgreSQL
    /// формулирует отказ сам и называет в нем только имя ограничения. По
    /// одному коду `23514` такой отказ не отличить ни от какого другого, и
    /// пользователь получал «прочее правило» там, где причина названа прямо
    /// в имени. Перечень открыт для пополнения - имя ограничения и есть его
    /// машинный признак.
    pub fn from_constraint(constraint: &str) -> Option<RuleViolation> {
        match constraint {
            // Вскрытие не раньше окончания приема заявок (FR-303, п. 27):
            // организатор, перепутавший две даты, обязан узнать об этом
            "deadline_before_opening" | "opened_not_before_meeting" => {
                Some(RuleViolation::TenderPublicationTerms)
            }
            _ => None,
        }
    }

    /// Причина по коду SQLSTATE - для отказов, у которых текста правила нет
    /// вовсе: их формулирует сам PostgreSQL по нарушенному ограничению.
    pub fn from_sqlstate(code: &str) -> Option<RuleViolation> {
        match code {
            "23503" => Some(RuleViolation::RelatedRecordMissing),
            "23505" => Some(RuleViolation::DuplicateRecord),
            "23P01" => Some(RuleViolation::OverlappingPeriod),
            _ => None,
        }
    }
}

/// Отказ по правилу: причина для клиента и исходное описание для дежурного.
///
/// Два поля, а не одно, потому что у них разные адресаты. `rule` уходит
/// в problem+json и превращается в переведенную строку интерфейса;
/// `internal` не уходит никуда, кроме telemetry (NFR-07): там и имя
/// инварианта, и подставленные значения полей - то, с чем дежурный
/// разбирает инцидент, и то, чего пользователю видеть незачем.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleRejection {
    rule: RuleViolation,
    internal: String,
}

impl RuleRejection {
    pub fn new(rule: RuleViolation, internal: impl Into<String>) -> Self {
        Self {
            rule,
            internal: internal.into(),
        }
    }

    /// Отказ, причина которого выводится из самого текста (сообщение триггера
    /// либо формулировка приложения с тем же идентификатором правила).
    pub fn classify(internal: impl Into<String>) -> Self {
        let internal = internal.into();
        let rule = RuleViolation::from_message(&internal).unwrap_or(RuleViolation::OtherRule);
        Self { rule, internal }
    }

    pub fn rule(&self) -> RuleViolation {
        self.rule
    }

    /// Описание для телеметрии. Наружу не отдается - см. `ApiError::RuleViolation`.
    pub fn internal(&self) -> &str {
        &self.internal
    }
}

impl From<String> for RuleRejection {
    fn from(internal: String) -> Self {
        RuleRejection::classify(internal)
    }
}

impl From<&str> for RuleRejection {
    fn from(internal: &str) -> Self {
        RuleRejection::classify(internal)
    }
}

/// Отказ печатается своим внутренним описанием: единственные его читатели -
/// `tracing` и цепочки `thiserror` (`#[error("{0}")]`).
impl std::fmt::Display for RuleRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_name_matches_serde() {
        for rule in RuleViolation::ALL {
            assert_eq!(
                serde_json::to_value(rule).unwrap(),
                serde_json::Value::String(rule.as_str().to_owned())
            );
        }
    }

    #[test]
    fn catalogue_has_no_duplicates() {
        let mut names: Vec<&str> = RuleViolation::ALL.iter().map(|r| r.as_str()).collect();
        names.sort_unstable();
        let total = names.len();
        names.dedup();
        assert_eq!(names.len(), total);
    }

    /// Сообщения взяты дословно из триггеров (`RAISE EXCEPTION` в миграциях):
    /// разбор обязан работать по тому тексту, который реально приходит.
    #[test]
    fn trigger_messages_are_classified() {
        let cases = [
            (
                "INV-063: ставка 100 ниже минимально допустимой 105 (максимум 100 + шаг 5)",
                RuleViolation::BidBelowMinimum,
            ),
            (
                "INV-066: время торгов истекло (2026-08-10 12:00:00+00)",
                RuleViolation::AuctionTimer,
            ),
            (
                "INV-021: переход статуса тендера draft -> trading запрещен",
                RuleViolation::TenderStatusTransition,
            ),
            (
                "INV-037: прием закрыт - дедлайн 2026-08-01 истек (п. 37–39)",
                RuleViolation::ApplicationDeadlinePassed,
            ),
            (
                "INV-DB-05: операция debit уводит баланс счета 42 в минус (100 - 200)",
                RuleViolation::LedgerBalanceNegative,
            ),
            (
                "FR-1103: голосует только присутствующий член комиссии (п. 13)",
                RuleViolation::CommissionVote,
            ),
            (
                "FR-1104: отведенный член комиссии не голосует по этому лоту (п. 15)",
                RuleViolation::CommissionVote,
            ),
            (
                "ставка отклонена: аукцион не в статусе running (текущий: scheduled)",
                RuleViolation::AuctionNotRunning,
            ),
            // `core.forbid_mutation('INV-037')` подставляет идентификатор
            // таблицы первым словом - запрет мутации важнее его
            (
                "INV-037: таблица journal_entries append-only (изменение и удаление запрещены)",
                RuleViolation::AppendOnlyTable,
            ),
            (
                "append-only: таблица log append-only (изменение и удаление запрещены)",
                RuleViolation::AppendOnlyTable,
            ),
        ];

        for (message, expected) in cases {
            assert_eq!(
                RuleViolation::from_message(message),
                Some(expected),
                "не разобрано: {message}"
            );
        }
    }

    /// Правило, которого в перечне нет, не притворяется знакомым.
    #[test]
    fn unknown_message_is_not_classified() {
        assert_eq!(
            RuleViolation::from_message("add_business_days: количество дней должно быть >= 0"),
            None
        );
        assert_eq!(
            RuleRejection::classify("что-то новое").rule(),
            RuleViolation::OtherRule
        );
    }

    #[test]
    fn sqlstate_covers_constraint_rejections() {
        assert_eq!(
            RuleViolation::from_sqlstate("23503"),
            Some(RuleViolation::RelatedRecordMissing)
        );
        assert_eq!(
            RuleViolation::from_sqlstate("23505"),
            Some(RuleViolation::DuplicateRecord)
        );
        assert_eq!(
            RuleViolation::from_sqlstate("23P01"),
            Some(RuleViolation::OverlappingPeriod)
        );
        // Ошибка проверки без текста правила - «прочее правило», а не догадка
        assert_eq!(RuleViolation::from_sqlstate("23514"), None);
    }

    /// Внутреннее описание сохраняется целиком: телеметрии нужен исходник.
    #[test]
    fn rejection_keeps_internal_message() {
        let rejection = RuleRejection::classify("INV-063: ставка 1 ниже минимально допустимой 2");
        assert_eq!(rejection.rule(), RuleViolation::BidBelowMinimum);
        assert!(rejection.internal().contains("ставка 1"));
        assert_eq!(rejection.to_string(), rejection.internal());
    }
}
