import { describe, expect, it } from "vite-plus/test"

import {
  compareObligations,
  deadlineUrgency,
  formatRelative,
  obligationUrgency,
} from "./relative-time"

import { overwriteGetLocale } from "#/paraglide/runtime"

/**
 * Устойчивость относительных сроков к составу ICU браузера (NFR-03, G-090).
 *
 * Подпись значка сроков собиралась `Intl.RelativeTimeFormat`. Данных `kk`
 * ни Chrome, ни Edge не везут: внутри казахской фразы печаталось «+9 d»,
 * а «вчера» на /kk/ и /en/ выходило одинаковым «yesterday». Node с полным
 * ICU (то есть SSR) печатал верно - подмену видел только читатель.
 *
 * Тест сторожит вывод: фраза каждой локали своя, а формы числительного
 * русского («день», «дня», «дней») различаются. Раннер идет под node с
 * полным ICU - тем самым составом, при котором прежний код выглядел
 * исправным, - поэтому проверяется форма строки, а не отсутствие бага ICU.
 */

const LOCALES = ["ru", "kk", "en"] as const

const NOW = Date.parse("2026-09-02T12:00:00Z")
const DAY = 86_400_000

function withLocale<T>(locale: (typeof LOCALES)[number], body: () => T): T {
  overwriteGetLocale(() => locale)
  try {
    return body()
  } finally {
    overwriteGetLocale(() => "ru")
  }
}

/** Момент, отстоящий от `NOW` на столько-то миллисекунд. */
function at(offsetMs: number): string {
  return new Date(NOW + offsetMs).toISOString()
}

function inEveryLocale(iso: string): string[] {
  return LOCALES.map((locale) =>
    withLocale(locale, () => formatRelative(iso, NOW))
  )
}

describe("относительное время", () => {
  it("девять дней вперед названы на языке интерфейса", () => {
    expect(inEveryLocale(at(9 * DAY))).toStrictEqual([
      "через 9 дней",
      "9 күннен кейін",
      "in 9 days",
    ])
  })

  it("соседние сутки названы словом", () => {
    expect(inEveryLocale(at(-DAY))).toStrictEqual([
      "вчера",
      "кеше",
      "yesterday",
    ])
    expect(inEveryLocale(at(DAY))).toStrictEqual([
      "завтра",
      "ертең",
      "tomorrow",
    ])
  })

  it("прошедшее время читается назад", () => {
    expect(inEveryLocale(at(-3 * DAY))).toStrictEqual([
      "3 дня назад",
      "3 күн бұрын",
      "3 days ago",
    ])
  })

  it("русский различает три формы числительного", () => {
    const days = [1, 2, 5, 21, 22, 25, 11, 12].map((count) =>
      withLocale("ru", () => formatRelative(at(count * DAY), NOW))
    )

    expect(days).toStrictEqual([
      "завтра",
      "через 2 дня",
      "через 5 дней",
      "через 21 день",
      "через 22 дня",
      "через 25 дней",
      "через 11 дней",
      "через 12 дней",
    ])
  })

  it("английский берет единственное число только при единице", () => {
    const hours = [1, 2, 21].map((count) =>
      withLocale("en", () => formatRelative(at(count * 3_600_000), NOW))
    )

    expect(hours).toStrictEqual(["in 1 hour", "in 2 hours", "in 21 hours"])
  })

  it("казахский не меняет слово после числа", () => {
    const hours = [1, 5, 21].map((count) =>
      withLocale("kk", () => formatRelative(at(count * 3_600_000), NOW))
    )

    expect(hours).toStrictEqual([
      "1 сағаттан кейін",
      "5 сағаттан кейін",
      "21 сағаттан кейін",
    ])
  })

  it("лестница единиц: минуты, часы, дни, месяцы, годы", () => {
    const ru = (offset: number) =>
      withLocale("ru", () => formatRelative(at(offset), NOW))

    expect(ru(5 * 60_000)).toBe("через 5 минут")
    expect(ru(5 * 3_600_000)).toBe("через 5 часов")
    expect(ru(5 * DAY)).toBe("через 5 дней")
    expect(ru(60 * DAY)).toBe("через 2 месяца")
    expect(ru(400 * DAY)).toBe("через 1 год")
  })

  it("полминуты в обе стороны - «сейчас», а не «через 0 минут»", () => {
    expect(inEveryLocale(at(10_000))).toStrictEqual(["сейчас", "қазір", "now"])
    expect(inEveryLocale(at(-10_000))).toStrictEqual(["сейчас", "қазір", "now"])
  })

  it("ни в одной локали не остается корневого «+9 d»", () => {
    const rendered = LOCALES.flatMap((locale) =>
      withLocale(locale, () => [
        formatRelative(at(9 * DAY), NOW),
        formatRelative(at(-DAY), NOW),
      ])
    )

    for (const text of rendered) {
      expect(text).not.toMatch(/^[+-]\d/u)
    }
  })

  it("английские слова не попадают в русскую и казахскую фразы", () => {
    const foreign = (["ru", "kk"] as const).flatMap((locale) =>
      withLocale(locale, () => [
        formatRelative(at(9 * DAY), NOW),
        formatRelative(at(-DAY), NOW),
      ])
    )

    for (const text of foreign) {
      expect(text).not.toMatch(/yesterday|days?/u)
    }
  })

  it("непарсящаяся метка не печатает NaN", () => {
    expect(formatRelative("не дата", NOW)).toBe("")
  })
})

describe("накал срока", () => {
  it("прошедший срок просрочен, ближайшие трое суток горят", () => {
    expect(deadlineUrgency(at(-1), NOW)).toBe("overdue")
    expect(deadlineUrgency(at(DAY), NOW)).toBe("soon")
    expect(deadlineUrgency(at(4 * DAY), NOW)).toBe("normal")
  })

  it("непарсящаяся метка не красит подпись", () => {
    expect(deadlineUrgency("не дата", NOW)).toBe("normal")
  })
})

/**
 * Накал и порядок сроков Правил (G-084).
 *
 * Значок сроков сравнивал `due_at` с часами браузера и о пяти просроченных
 * обязательствах из шести сообщал «через 9 дней»: просрочку решает движок
 * обязательств по производственному календарю, и `due_at` у просроченного
 * срока вполне стоит в будущем.
 */
describe("накал обязательства", () => {
  it("статус overdue сильнее любых дат", () => {
    expect(
      obligationUrgency({ due_at: at(9 * DAY), status: "overdue" }, NOW)
    ).toBe("overdue")
  })

  it("сервер не считает срок просроченным - и мы не считаем", () => {
    expect(
      obligationUrgency({ due_at: at(-DAY), status: "pending" }, NOW)
    ).toBe("soon")
  })

  it("открытый срок красится по близости", () => {
    expect(obligationUrgency({ due_at: at(DAY), status: "pending" }, NOW)).toBe(
      "soon"
    )
    expect(
      obligationUrgency({ due_at: at(9 * DAY), status: "pending" }, NOW)
    ).toBe("normal")
  })

  it("просроченное - выше открытого, дальше по близости срока", () => {
    const items = [
      { id: "late-open", due_at: at(5 * DAY), status: "pending" },
      { id: "overdue-far", due_at: at(9 * DAY), status: "overdue" },
      { id: "soon-open", due_at: at(DAY), status: "pending" },
      { id: "overdue-near", due_at: at(2 * DAY), status: "overdue" },
    ]
    expect(items.toSorted(compareObligations).map((item) => item.id)).toEqual([
      "overdue-near",
      "overdue-far",
      "soon-open",
      "late-open",
    ])
  })
})
