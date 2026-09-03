import { m } from "#/paraglide/messages"
import { getLocale } from "#/paraglide/runtime"

/**
 * Относительное время сроков («через 2 дня», «3 часа назад»).
 *
 * «Сейчас» приходит параметром, а не берется из `Date.now()` внутри: тогда
 * функция остается чистой и проверяемой, а вызывающий сам решает, откуда
 * взять момент отсчета. В клиентском дереве кабинетов это `Date.now()`,
 * в SSR-ветке - серверная метка из загрузчика, иначе разметка сервера и
 * браузера расходятся на первом же кадре.
 *
 * Фразы - ключи Paraglide, а не `Intl.RelativeTimeFormat`, по той же
 * причине, что дата и сумма в `lib/format.ts`: у `kk` в Chrome и Edge нет
 * своих данных, и внутри казахской фразы печаталось «+9 d», а на /en/ и
 * /kk/ одинаково выходило «yesterday». Node с полным ICU (то есть SSR)
 * давал верный текст, поэтому подмену видел только читатель.
 */

const MINUTE = 60_000
const HOUR = 60 * MINUTE
const DAY = 24 * HOUR
const MONTH = 30 * DAY
const YEAR = 365 * DAY

/** Порог «скоро»: трое суток - шаг сроков Правил измеряется днями. */
const SOON_MS = 72 * HOUR

/** Единицы, в которых называется расстояние до срока. */
type Unit = "minute" | "hour" | "day" | "month" | "year"

/**
 * Форма числительного. Русский различает три (день / дня / дней),
 * английский - две, казахский после числа не меняет слово вовсе. Отбор
 * идет по правилу локали, а не по одному общему: «21 день» и «in 21 day»
 * - разные ошибки одного и того же упрощения.
 */
type PluralForm = "one" | "few" | "many"

const MS: Record<Unit, number> = {
  minute: MINUTE,
  hour: HOUR,
  day: DAY,
  month: MONTH,
  year: YEAR,
}

type Phrase = (inputs: { count: number }) => string

const FUTURE: Record<Unit, Record<PluralForm, Phrase>> = {
  minute: {
    one: m.relative_in_minute_one,
    few: m.relative_in_minute_few,
    many: m.relative_in_minute_many,
  },
  hour: {
    one: m.relative_in_hour_one,
    few: m.relative_in_hour_few,
    many: m.relative_in_hour_many,
  },
  day: {
    one: m.relative_in_day_one,
    few: m.relative_in_day_few,
    many: m.relative_in_day_many,
  },
  month: {
    one: m.relative_in_month_one,
    few: m.relative_in_month_few,
    many: m.relative_in_month_many,
  },
  year: {
    one: m.relative_in_year_one,
    few: m.relative_in_year_few,
    many: m.relative_in_year_many,
  },
}

const PAST: Record<Unit, Record<PluralForm, Phrase>> = {
  minute: {
    one: m.relative_ago_minute_one,
    few: m.relative_ago_minute_few,
    many: m.relative_ago_minute_many,
  },
  hour: {
    one: m.relative_ago_hour_one,
    few: m.relative_ago_hour_few,
    many: m.relative_ago_hour_many,
  },
  day: {
    one: m.relative_ago_day_one,
    few: m.relative_ago_day_few,
    many: m.relative_ago_day_many,
  },
  month: {
    one: m.relative_ago_month_one,
    few: m.relative_ago_month_few,
    many: m.relative_ago_month_many,
  },
  year: {
    one: m.relative_ago_year_one,
    few: m.relative_ago_year_few,
    many: m.relative_ago_year_many,
  },
}

/**
 * Форма числительного по правилам локали. Вне русского второй формы нет:
 * английский берет единственное только при единице, казахский - никогда,
 * и обе локали приходят в `many`.
 */
function pluralForm(count: number, locale: string): PluralForm {
  if (locale !== "ru") return count === 1 ? "one" : "many"

  const mod10 = count % 10
  const mod100 = count % 100
  if (mod10 === 1 && mod100 !== 11) return "one"
  if (mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14)) return "few"
  return "many"
}

/** Единица, в которой называется расстояние: та же лестница, что и была. */
function unitFor(distance: number): Unit {
  if (distance < HOUR) return "minute"
  if (distance < DAY) return "hour"
  if (distance < MONTH) return "day"
  if (distance < YEAR) return "month"
  return "year"
}

/** Пустая строка для непарсящейся метки: подпись срока - не место для NaN. */
export function formatRelative(iso: string, nowMs: number): string {
  const target = new Date(iso).getTime()
  if (!Number.isFinite(target)) return ""

  const diff = target - nowMs
  const unit = unitFor(Math.abs(diff))
  const signed = Math.round(diff / MS[unit])
  const count = Math.abs(signed)

  // Полминуты в обе стороны - это «сейчас», а не «через 0 минут»
  if (count === 0) return m.relative_now()
  // Соседние сутки называются словом, как и у `Intl` с `numeric: "auto"`:
  // «вчера» читается быстрее, чем «1 день назад»
  if (unit === "day" && count === 1) {
    return signed > 0 ? m.relative_tomorrow() : m.relative_yesterday()
  }

  const phrases = signed > 0 ? FUTURE : PAST
  return phrases[unit][pluralForm(count, getLocale())]({ count })
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

/**
 * Насколько горит срок Правил; о просрочке спрашивают сервер, а не часы.
 *
 * Срок считается по производственному календарю (FR-1701), и `due_at` -
 * не тот момент, с которым надо сравнивать `Date.now()`: движок обязательств
 * поднимает `status = overdue` по своим правилам, и у просроченного срока
 * `due_at` вполне может стоять в будущем. Значок сроков сравнивал даты сам
 * и о пяти просроченных из шести сообщал «через 9 дней».
 *
 * Обратная сторона отдана датам: `pending` со сроком через час - это «скоро»,
 * и сервер такого признака не отдает.
 */
export function obligationUrgency(
  obligation: { due_at: string; status: string },
  nowMs: number
): DeadlineUrgency {
  if (obligation.status === "overdue") return "overdue"
  const byDate = deadlineUrgency(obligation.due_at, nowMs)
  // Срок еще не просрочен по мнению сервера - значит, и мы этого не решаем
  return byDate === "overdue" ? "soon" : byDate
}

/**
 * Порядок сроков в списках: просроченное - выше, дальше по близости срока.
 *
 * Одна сортировка на значок и на дашборд: два разных порядка на одних и тех
 * же данных читались бы как два разных перечня.
 */
export function compareObligations(
  left: { due_at: string; status: string },
  right: { due_at: string; status: string }
): number {
  return (
    overdueRank(left.status) - overdueRank(right.status) ||
    Date.parse(left.due_at) - Date.parse(right.due_at)
  )
}

function overdueRank(status: string): number {
  return status === "overdue" ? 0 : 1
}
