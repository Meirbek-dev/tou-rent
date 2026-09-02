import { describe, expect, it } from "vite-plus/test"

import { problemDetail, problemMessage } from "./auth"

/**
 * Сообщение об отказе - всегда строка интерфейса (NFR-01, NFR-08).
 *
 * Последней веткой `problemMessage` возвращал литерал `"unknown_error"`:
 * не ключ Paraglide, а служебное слово. Ветка достижима на всем, что не
 * разбирается в problem+json - обрыв сети, 502 от прокси, HTML-страница
 * ошибки, собственные броски слоя данных (`new Error("failed to load ...")`),
 * - а отфильтрован литерал был ровно в одном месте из тридцати восьми,
 * сравнением строк. Здесь проверяется само свойство: наружу не уходит
 * машинный код ни в одной ветке.
 */

/** `unknown_error`, `failed_to_load` и прочее в snake_case - это код, не текст. */
const MACHINE_TOKEN = /^[a-z0-9]+(_[a-z0-9]+)+$/

/** Ошибки, которые сервер не объяснил: у них нет ни `rule`, ни `detail`, ни `title`. */
const UNPARSEABLE: unknown[] = [
  new Error("failed to load auth providers"),
  new TypeError("Failed to fetch"),
  null,
  undefined,
  "<html>502 Bad Gateway</html>",
  {},
  { detail: "" },
  { title: "" },
]

describe("сообщение об отказе", () => {
  it("неразобранная ошибка объясняется словами, а не служебным словом", () => {
    for (const error of UNPARSEABLE) {
      const text = problemMessage(error)
      expect(text).not.toBe("")
      expect(text).not.toMatch(MACHINE_TOKEN)
    }
  })

  it("у неразобранной ошибки нет подробности - показывать нечего", () => {
    for (const error of UNPARSEABLE) {
      expect(problemDetail(error)).toBeNull()
    }
  })

  it("отказ по правилу переводится по каталогу причин", () => {
    const detail = problemDetail({ rule: "bid_below_minimum" })
    expect(detail).not.toBeNull()
    expect(detail).not.toContain("bid_below_minimum")
    expect(problemMessage({ rule: "bid_below_minimum" })).toBe(detail)
  })

  it("detail и title сервера доходят до пользователя как есть", () => {
    expect(problemDetail({ detail: "Файл больше 20 МБ" })).toBe(
      "Файл больше 20 МБ"
    )
    expect(problemDetail({ title: "Доступ закрыт" })).toBe("Доступ закрыт")
    // detail важнее title: он про конкретный отказ, а не про класс ответа
    expect(problemDetail({ detail: "Срок истек", title: "Conflict" })).toBe(
      "Срок истек"
    )
  })
})
