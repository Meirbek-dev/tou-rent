// `#/` вместо `@/`: этот псевдоним разрешает и раннер тестов (см. `lib/auth.ts`)
import { serverLabel } from "#/lib/server-label"

/**
 * Отбор строк кабинета финансов: депозитная книга и реестр уклонистов.
 *
 * Чистые функции живут отдельно от экрана, потому что проверять их надо
 * без браузера: книга на тридцати счетах была лентой без фильтра, и
 * ошибка в отборе - это счет, которого оператор не нашел.
 */

/** Виды счетов книги (FR-1001): взнос участника и депозит договора. */
export const LEDGER_ACCOUNT_KINDS = [
  "participant_fee",
  "contract_deposit",
] as const

/** Строка справочника с подписями в трех колонках (NFR-01). */
export type CodeLabelSource = {
  code: string
  label_ru: string
  label_kk?: string | null
  label_en?: string | null
}

/**
 * Подпись кода из справочника: основание возврата (п. 26), основание
 * уклонения (п. 116).
 *
 * Кода нет - показывать нечего (`null`, а не пустая строка: у вызывающего
 * это «строку не рисуем»). Справочник не загрузился или код в нем не нашелся -
 * остается сам код: машинное слово лучше пустоты, по нему хотя бы понятно,
 * что основание у записи есть.
 */
export function codeLabel(
  code: string | null | undefined,
  items: readonly CodeLabelSource[] | undefined
): string | null {
  if (code == null || code === "") return null
  const known = items?.find((item) => item.code === code)
  return known === undefined ? code : serverLabel(known)
}

/** Счет книги в том объеме, в каком его отбирает фильтр. */
export type LedgerAccountRow = {
  kind: string
  owner_name: string
  tender_title?: string | null
}

/**
 * Отбор счета по виду и подстроке.
 *
 * Ищется и по владельцу, и по названию тендера: оператор помнит счет либо
 * по нанимателю из банковской выписки, либо по процедуре, на которой взнос
 * внесен. Регистр не важен - имена в выписке набраны как попало.
 */
export function matchesLedgerAccount(
  account: LedgerAccountRow,
  filter: { kind: string; query: string }
): boolean {
  if (filter.kind !== "" && account.kind !== filter.kind) return false

  const needle = filter.query.trim().toLocaleLowerCase()
  if (needle === "") return true

  return [account.owner_name, account.tender_title ?? ""].some((field) =>
    field.toLocaleLowerCase().includes(needle)
  )
}
