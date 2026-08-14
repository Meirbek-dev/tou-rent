import { getLocale } from "#/paraglide/runtime"

/**
 * Относительное время сроков («через 2 дня», «3 часа назад»).
 *
 * «Сейчас» приходит параметром, а не берется из `Date.now()` внутри: тогда
 * функция остается чистой и проверяемой, а вызывающий сам решает, откуда
 * взять момент отсчета. В клиентском дереве кабинетов это `Date.now()`,
 * в SSR-ветке - серверная метка из загрузчика, иначе разметка сервера и
 * браузера расходятся на первом же кадре.
 */

const MINUTE = 60_000
const HOUR = 60 * MINUTE
const DAY = 24 * HOUR
const MONTH = 30 * DAY
const YEAR = 365 * DAY

/** Порог «скоро»: трое суток - шаг сроков Правил измеряется днями. */
const SOON_MS = 72 * HOUR

/** Пустая строка для непарсящейся метки: подпись срока - не место для NaN. */
export function formatRelative(iso: string, nowMs: number): string {
  const target = new Date(iso).getTime()
  if (!Number.isFinite(target)) return ""

  const diff = target - nowMs
  const distance = Math.abs(diff)
  const format = new Intl.RelativeTimeFormat(getLocale(), { numeric: "auto" })

  if (distance < HOUR) return format.format(Math.round(diff / MINUTE), "minute")
  if (distance < DAY) return format.format(Math.round(diff / HOUR), "hour")
  if (distance < MONTH) return format.format(Math.round(diff / DAY), "day")
  if (distance < YEAR) return format.format(Math.round(diff / MONTH), "month")
  return format.format(Math.round(diff / YEAR), "year")
}

export type DeadlineUrgency = "overdue" | "soon" | "normal"

/**
 * Насколько горит срок. Тон интерфейса берется отсюда, а не из сравнения
 * дат на месте вызова: порог «скоро» обязан быть один на весь портал.
 */
export function deadlineUrgency(iso: string, nowMs: number): DeadlineUrgency {
  const target = new Date(iso).getTime()
  if (!Number.isFinite(target)) return "normal"

  const diff = target - nowMs
  if (diff < 0) return "overdue"
  if (diff <= SOON_MS) return "soon"
  return "normal"
}
