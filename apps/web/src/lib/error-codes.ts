import { m } from "#/paraglide/messages"

import type { components } from "@tou/api-client"

/** Машинный код отказа из problem+json (RFC 9457, NFR-08). */
export type ErrorCode = components["schemas"]["ErrorCode"]

/**
 * Тексты кодов отказа (NFR-01).
 *
 * До этой таблицы объяснением служил `title` (или `detail`) сервера: он
 * составлен по-русски для делопроизводства и никогда не переводился, поэтому
 * на /en/ и /kk/ отказ входа читался «Неверный email или пароль», а отказ
 * валидации - английским текстом garde. Из семнадцати кодов контракта свой
 * текст был ровно у одного - `unknown`, и тот вне перечня.
 *
 * Перечень закрыт схемой `ErrorCode`, а до интерфейса доезжает строкой -
 * компилятор такую пару не сверит. `satisfies` держит таблицу полной
 * относительно типа контракта, а `error-codes.test.ts` - относительно
 * самой схемы: новый код в контракте не должен молча получить серверный
 * русский текст.
 */
const ERROR_MESSAGES = {
  unauthorized: m.error_unauthorized,
  forbidden: m.error_forbidden,
  invalid_credentials: m.error_invalid_credentials,
  email_not_confirmed: m.error_email_not_confirmed,
  email_taken: m.error_email_taken,
  id_number_taken: m.error_id_number_taken,
  verification_failed: m.error_verification_failed,
  validation_failed: m.error_validation_failed,
  csrf_rejected: m.error_csrf_rejected,
  not_found: m.error_not_found,
  rule_violation: m.error_rule_violation,
  provider_unavailable: m.error_provider_unavailable,
  too_many_requests: m.error_too_many_requests,
  idempotency_in_flight: m.error_idempotency_in_flight,
  idempotency_key_reuse: m.error_idempotency_key_reuse,
  method_not_allowed: m.error_method_not_allowed,
  timeout: m.error_timeout,
  internal: m.error_internal,
} satisfies Record<ErrorCode, () => string>

/**
 * Поиск идет по чужой строке: код приходит из сети, а не из типа. Отдельный
 * псевдоним вместо приведения на месте - чтобы `satisfies` выше продолжал
 * ловить недостающую ветку.
 */
const LOOKUP: Record<string, (() => string) | undefined> = ERROR_MESSAGES

/** Текст кода; `null` - код вне перечня контракта (показывать нечего). */
export function errorCodeMessage(code: string): string | null {
  return LOOKUP[code]?.() ?? null
}
