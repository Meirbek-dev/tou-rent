import { getLocale } from "#/paraglide/runtime"

// NFR-03: юридически значимое время - серверное (UTC), отображение - Asia/Almaty.
const DISPLAY_TZ = "Asia/Almaty"

export function formatDateTime(iso: string | null | undefined): string | null {
  if (!iso) return null
  return new Intl.DateTimeFormat(getLocale(), {
    dateStyle: "long",
    timeStyle: "short",
    timeZone: DISPLAY_TZ,
  }).format(new Date(iso))
}

/** Дата без времени: сроки хранения и прочие «до такого-то числа». */
export function formatDate(iso: string | null | undefined): string | null {
  if (!iso) return null
  return new Intl.DateTimeFormat(getLocale(), {
    dateStyle: "long",
    timeZone: DISPLAY_TZ,
  }).format(new Date(iso))
}

/** Убирает незначащие нули десятичной строки Decimal ("0.500000" → "0.5"). */
export function trimZeros(value: string): string {
  if (!value.includes(".")) return value
  return value.replace(/0+$/, "").replace(/\.$/, "")
}

/** Денежные суммы контракта приходят строками ("21000"). */
export function formatTenge(amount: string): string {
  const value = Number(amount)
  if (!Number.isFinite(value)) return amount
  return new Intl.NumberFormat(getLocale(), {
    style: "currency",
    currency: "KZT",
    maximumFractionDigits: 2,
  }).format(value)
}
