import { describe, expect, it } from "vite-plus/test"

import { formatDate, formatDateTime } from "./format"

import { overwriteGetLocale } from "#/paraglide/runtime"

/**
 * Устойчивость печати дат к составу ICU браузера (NFR-03, G-010).
 *
 * Сроки подачи заявок, вскрытия и торгов юридически значимы, и печатались они
 * длинным шаблоном локали. У `kk` такого шаблона нет ни в Chrome, ни в Edge:
 * оба берут корневой, и «2 қыркүйек 2026 ж.» на экране оказывалось «2026 M09 2»
 * (в Edge - с английским месяцем), тогда как node с полным ICU печатал верно.
 * Один и тот же срок читался на сервере и в браузере по-разному.
 *
 * Тест сторожит вывод, а не реализацию: числовая запись обязана быть одна на
 * три локали, и корневого «M09» в ней быть не может. Раннер идет под node с
 * полным ICU - тот самый состав, при котором прежний код выглядел исправным,
 * - поэтому проверяется не отсутствие бага в ICU, а форма самой строки.
 */

const LOCALES = ["ru", "kk", "en"] as const

/** 2 сентября 2026, 17:52 UTC = 22:52 в зоне отображения (UTC+5). */
const MOMENT = "2026-09-02T17:52:00Z"

function withLocale<T>(locale: (typeof LOCALES)[number], body: () => T): T {
  overwriteGetLocale(() => locale)
  try {
    return body()
  } finally {
    overwriteGetLocale(() => "ru")
  }
}

describe("печать дат", () => {
  it("одинакова во всех локалях и числовая", () => {
    const rendered = LOCALES.map((locale) =>
      withLocale(locale, () => formatDateTime(MOMENT))
    )

    expect(rendered).toStrictEqual([
      "02.09.2026 22:52 GMT+5",
      "02.09.2026 22:52 GMT+5",
      "02.09.2026 22:52 GMT+5",
    ])
  })

  it("не печатает корневой шаблон ICU (M09) ни в одной локали", () => {
    for (const locale of LOCALES) {
      const rendered = withLocale(locale, () => [
        formatDateTime(MOMENT),
        formatDate("2027-03-02"),
      ])

      for (const text of rendered) {
        expect(text).not.toMatch(/M\d\d/)
        expect(text).toMatch(/^\d\d\.\d\d\.\d{4}/)
      }
    }
  })

  it("дата без времени не тянет за собой время и зону", () => {
    expect(withLocale("kk", () => formatDate("2027-03-02"))).toBe("02.03.2027")
  })

  it("час сдвинут в зону отображения, полночь печатается как 00:00", () => {
    // 19:00 UTC = 00:00 следующих суток в UTC+5: и день, и час должны
    // перевалить, а «24:00» не годится ни для одной локали
    expect(formatDateTime("2026-09-02T19:00:00Z")).toBe(
      "03.09.2026 00:00 GMT+5"
    )
  })

  it("пустой момент остается пустым", () => {
    expect(formatDateTime(null)).toBeNull()
    expect(formatDate(undefined)).toBeNull()
  })
})
