import { m } from "#/paraglide/messages"

import type { components } from "@tou/api-client"

/** Причина отказа по правилу из problem+json (поле `rule`). */
export type Rule = components["schemas"]["Rule"]

/**
 * Тексты причин отказа (NFR-01).
 *
 * До этой таблицы объяснением отказа служил `detail` из problem+json, а туда
 * попадало сообщение `RAISE EXCEPTION` триггера БД: единственная строка
 * интерфейса, существовавшая только по-русски и мимо Paraglide. Участник
 * с локалью kk или en получал русский текст ровно там, где причина ему
 * нужнее всего, - и вместе с ней имя инварианта и значения полей.
 *
 * Теперь сервер присылает машинную причину из закрытого перечня
 * (`domain::rule::RuleViolation`), а текст выбирается здесь. Полноту таблицы
 * относительно перечня держит тест `rule-messages.test.ts`: он читает сам
 * каталог домена - компилятор эту пару не сверит, строка контракта для него
 * просто строка.
 */
const RULE_MESSAGES: Record<string, () => string> = {
  tender_status_transition: m.rule_tender_status_transition,
  tender_publication_terms: m.rule_tender_publication_terms,
  tender_schedule_order: m.rule_tender_schedule_order,
  tender_documentation_change: m.rule_tender_documentation_change,
  tender_cancellation: m.rule_tender_cancellation,
  tender_failure_ground: m.rule_tender_failure_ground,
  application_intake_closed: m.rule_application_intake_closed,
  application_deadline_passed: m.rule_application_deadline_passed,
  application_already_submitted: m.rule_application_already_submitted,
  application_not_pending: m.rule_application_not_pending,
  sealed_price_key_missing: m.rule_sealed_price_key_missing,
  commission_composition: m.rule_commission_composition,
  commission_meeting: m.rule_commission_meeting,
  commission_vote: m.rule_commission_vote,
  admission_notice: m.rule_admission_notice,
  auction_not_running: m.rule_auction_not_running,
  auction_start_price_missing: m.rule_auction_start_price_missing,
  bid_below_minimum: m.rule_bid_below_minimum,
  auction_timer: m.rule_auction_timer,
  auction_turn_order: m.rule_auction_turn_order,
  auction_announcement: m.rule_auction_announcement,
  auction_result_mismatch: m.rule_auction_result_mismatch,
  result_protocol: m.rule_result_protocol,
  protocol_publication: m.rule_protocol_publication,
  publication_retention: m.rule_publication_retention,
  public_record_link: m.rule_public_record_link,
  dossier_immutable: m.rule_dossier_immutable,
  contract_conclusion: m.rule_contract_conclusion,
  contract_stage_order: m.rule_contract_stage_order,
  contract_terms_immutable: m.rule_contract_terms_immutable,
  winner_evasion: m.rule_winner_evasion,
  act_order: m.rule_act_order,
  contract_registration: m.rule_contract_registration,
  contract_amendment: m.rule_contract_amendment,
  document_check_incomplete: m.rule_document_check_incomplete,
  contract_deposit: m.rule_contract_deposit,
  guarantee_deposit: m.rule_guarantee_deposit,
  deposit_refund_reason: m.rule_deposit_refund_reason,
  ledger_entry: m.rule_ledger_entry,
  ledger_balance_negative: m.rule_ledger_balance_negative,
  special_order_application: m.rule_special_order_application,
  special_order_transition: m.rule_special_order_transition,
  special_order_competition: m.rule_special_order_competition,
  board_decision: m.rule_board_decision,
  board_decision_without_opinion: m.rule_board_decision_without_opinion,
  investment_contract: m.rule_investment_contract,
  investment_documents_missing: m.rule_investment_documents_missing,
  benefit_scheme: m.rule_benefit_scheme,
  benefit_approval_missing: m.rule_benefit_approval_missing,
  spinoff_teaching_quota: m.rule_spinoff_teaching_quota,
  special_publication: m.rule_special_publication,
  land_application: m.rule_land_application,
  land_contract_terms_missing: m.rule_land_contract_terms_missing,
  object_in_use: m.rule_object_in_use,
  status_not_allowed: m.rule_status_not_allowed,
  append_only_table: m.rule_append_only_table,
  overlapping_period: m.rule_overlapping_period,
  duplicate_record: m.rule_duplicate_record,
  related_record_missing: m.rule_related_record_missing,
  other_rule: m.rule_other_rule,
}

/**
 * Текст причины отказа. Неизвестное значение - не пустота и не машинный код:
 * сервер обновляется раньше страницы, и новая причина обязана хотя бы
 * объяснить, что отказ пришел по правилу.
 */
export function ruleMessage(rule: string): string {
  return RULE_MESSAGES[rule]?.() ?? m.rule_unknown()
}
