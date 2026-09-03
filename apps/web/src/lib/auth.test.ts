import { describe, expect, it } from "vite-plus/test"

import {
  problemCode,
  problemDetail,
  problemMessage,
  safeRedirectPath,
} from "./auth"

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

/**
 * Адрес возврата после входа (FR-1501, G-074).
 *
 * Параметр `?redirect=` приезжает из чужой ссылки, и страница входа - самое
 * подходящее место для открытого перенаправителя: письмо с адресом портала
 * уводило бы на чужой домен уже после ввода пароля. Проверка одна и здесь же
 * закреплена: наружу проходит только путь этого же сайта.
 */
describe("safeRedirectPath (FR-1501: возврат после входа)", () => {
  it("пропускает путь внутри портала вместе с запросом", () => {
    expect(safeRedirectPath("/app/participant/contracts")).toBe(
      "/app/participant/contracts"
    )
    expect(safeRedirectPath("/tenders?status=accepting")).toBe(
      "/tenders?status=accepting"
    )
    expect(safeRedirectPath("/")).toBe("/")
  })

  it("отвергает чужой адрес в любом написании", () => {
    for (const value of [
      "https://evil.example/app",
      "http://evil.example",
      "//evil.example",
      "/\\evil.example",
      "javascript:alert(1)",
      "app/participant",
      "",
    ]) {
      expect(safeRedirectPath(value)).toBeUndefined()
    }
  })

  it("отвергает управляющие символы: браузер выбрасывает их из адреса", () => {
    // после выброса табуляции остается «//evil.example» - снова чужой хост
    expect(safeRedirectPath("/\t/evil.example")).toBeUndefined()
    expect(safeRedirectPath("/app\n")).toBeUndefined()
  })

  it("не возвращает на сам вход: это замкнутая петля", () => {
    expect(safeRedirectPath("/auth/login")).toBeUndefined()
    expect(safeRedirectPath("/auth/register")).toBeUndefined()
  })

  it("отвергает не-строку", () => {
    for (const value of [undefined, null, 42, {}, ["/app"]]) {
      expect(safeRedirectPath(value)).toBeUndefined()
    }
  })
})

/**
 * Код отказа читается отдельно от текста (G-080, G-081): один и тот же
 * `invalid_credentials` значит «email или пароль» на входе и ровно «текущий
 * пароль» на смене пароля, а `email_taken` называет поле, которое надо
 * пометить.
 */
describe("problemCode (NFR-08: машинный код отказа)", () => {
  it("достает код из problem+json", () => {
    expect(problemCode({ code: "invalid_credentials" })).toBe(
      "invalid_credentials"
    )
    expect(problemCode({ code: "id_number_taken", status: 409 })).toBe(
      "id_number_taken"
    )
  })

  it("на неразбираемом отказе кода нет", () => {
    for (const value of [
      new Error("failed to load contracts"),
      null,
      undefined,
      "502",
      {},
      { code: 401 },
    ]) {
      expect(problemCode(value)).toBeUndefined()
    }
  })
})
