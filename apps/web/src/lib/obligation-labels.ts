import { m } from "#/paraglide/messages"

/**
 * Подписи сроков (FR-1702). Перечень повторяет каталог
 * `domain::obligation::ObligationAction`: там он закрыт enum'ом, а до
 * интерфейса доезжает строкой контракта, поэтому паритет держит таблица,
 * а ее полноту - тест `obligation-labels.test.ts` (он читает сам каталог).
 * Раньше подписей было четыре из восемнадцати, и остальные сроки
 * показывались в дашборде машинным кодом (`contract_draft`).
 *
 * Модуль намеренно не зависит ни от чего, кроме переводов: так его берет
 * и тест, и любой кабинет.
 */
const DEADLINE_LABELS: Record<string, () => string> = {
  admission_protocol: m.deadline_admission_protocol,
  notify_admitted: m.deadline_notify_admitted,
  results_protocol: m.deadline_results_protocol,
  publish_results: m.deadline_publish_results,
  fee_refund: m.deadline_fee_refund,
  failed_protocol: m.deadline_failed_protocol,
  contract_draft: m.deadline_contract_draft,
  tenant_sign: m.deadline_tenant_sign,
  tenant_documents: m.deadline_tenant_documents,
  landlord_sign: m.deadline_landlord_sign,
  contract_handover: m.deadline_contract_handover,
  winner2_protocol: m.deadline_winner2_protocol,
  notify_runner_up: m.deadline_notify_runner_up,
  notify_amendment: m.deadline_notify_amendment,
  notify_cancellation: m.deadline_notify_cancellation,
  special_review: m.deadline_special_review,
  special_decision: m.deadline_special_decision,
  special_publish: m.deadline_special_publish,
  deposit_payment: m.deadline_deposit_payment,
  deposit_top_up: m.deadline_deposit_top_up,
  deposit_refund: m.deadline_deposit_refund,
}

/** Читаемая подпись срока; неизвестный код остается кодом, а не пустотой. */
export function deadlineLabel(action: string): string {
  return DEADLINE_LABELS[action]?.() ?? action
}
