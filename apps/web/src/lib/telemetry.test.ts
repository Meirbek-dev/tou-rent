import { describe, expect, it } from "vite-plus/test"

import {
  REDACTED,
  sanitizeProperties,
  scrubBreadcrumb,
  scrubEvent,
  scrubUrl,
} from "../../telemetry.mjs"

/**
 * Очистка телеметрии от ПДн (T71, NFR-07, NFR-16).
 *
 * Утечка во внешний сервис необратима, поэтому проверяется не «функция
 * что-то поменяла», а отсутствие конкретной строки в результате: имени,
 * адреса почты, поискового запроса.
 */

const NAME = "Иванов Иван"
const EMAIL = "ivanov@example.test"
const IIN = "990101300123"

describe("scrubUrl", () => {
  it("убирает строку запроса — там поисковый ввод посетителя", () => {
    const scrubbed = scrubUrl(
      `https://rent.tou.edu.kz/tenders?q=${encodeURIComponent(NAME)}&status=accepting`
    )
    expect(scrubbed).toBe(`https://rent.tou.edu.kz/tenders?${REDACTED}`)
    expect(String(scrubbed)).not.toContain("q=")
  })

  it("убирает фрагмент", () => {
    expect(scrubUrl("/objects#area=40")).toBe(`/objects?${REDACTED}`)
  })

  it("оставляет путь целиком: идентификатор записи нужен для разбора", () => {
    const url =
      "/app/participant/applications/019fe14b-28c0-75ac-a52d-5e5999fc71b4"
    expect(scrubUrl(url)).toBe(url)
  })

  it("не трогает то, что адресом не является", () => {
    expect(scrubUrl(undefined)).toBeUndefined()
    expect(scrubUrl("")).toBe("")
    expect(scrubUrl(42)).toBe(42)
  })
})

describe("scrubBreadcrumb", () => {
  it("выбрасывает крошки, подписанные текстом элемента", () => {
    for (const category of ["ui.click", "ui.input", "console"]) {
      expect(
        scrubBreadcrumb({ category, message: `${NAME} ${EMAIL}` })
      ).toBeNull()
    }
  })

  it("чистит адреса у сетевых и навигационных крошек", () => {
    const crumb = scrubBreadcrumb({
      category: "fetch",
      data: {
        url: `/api/v1/objects?q=${NAME}`,
        method: "GET",
        status_code: 200,
      },
    })
    expect(crumb?.data?.["url"]).toBe(`/api/v1/objects?${REDACTED}`)
    expect(crumb?.data?.["method"]).toBe("GET")
    expect(JSON.stringify(crumb)).not.toContain("Иванов")
  })

  it("крошку без данных пропускает как есть", () => {
    const crumb = { category: "navigation" }
    expect(scrubBreadcrumb(crumb)).toEqual(crumb)
    expect(scrubBreadcrumb(null)).toBeNull()
  })
})

describe("scrubEvent", () => {
  it("снимает тело, cookie и заголовки запроса, чистит адрес", () => {
    const event = scrubEvent({
      request: {
        url: `https://rent.tou.edu.kz/tenders?q=${NAME}`,
        method: "POST",
        data: { applicant_details: { name: NAME, id_number: IIN } },
        cookies: { tou_session: "s%3Asecret" },
        headers: { cookie: "tou_session=s%3Asecret", "user-agent": "curl" },
      },
    })

    expect(event?.["request"]).toEqual({
      url: `https://rent.tou.edu.kz/tenders?${REDACTED}`,
      method: "POST",
    })
    const serialized = JSON.stringify(event)
    for (const secret of [NAME, IIN, "tou_session"]) {
      expect(serialized).not.toContain(secret)
    }
  })

  it("от пользователя оставляет только идентификатор", () => {
    const event = scrubEvent({
      user: {
        id: "019fe14b",
        email: EMAIL,
        username: NAME,
        ip_address: "10.0.0.1",
      },
    })
    expect(event?.["user"]).toEqual({ id: "019fe14b" })
  })

  it("пользователя без идентификатора убирает целиком", () => {
    expect(scrubEvent({ user: { email: EMAIL } })?.["user"]).toBeUndefined()
  })

  it("прогоняет крошки теми же правилами", () => {
    const event = scrubEvent({
      breadcrumbs: [
        { category: "ui.click", message: NAME },
        { category: "fetch", data: { url: `/objects?q=${NAME}` } },
        {
          category: "navigation",
          data: { from: "/", to: `/tenders?q=${NAME}` },
        },
      ],
    })

    expect(event?.["breadcrumbs"]).toHaveLength(2)
    expect(JSON.stringify(event)).not.toContain("Иванов")
  })

  it("не падает на пустом событии", () => {
    expect(scrubEvent(null)).toBeNull()
    expect(scrubEvent({})).toEqual({})
  })
})

describe("sanitizeProperties", () => {
  it("выбрасывает разметку автозахвата и чистит адреса", () => {
    const sanitized = sanitizeProperties({
      $current_url: `https://rent.tou.edu.kz/objects?q=${NAME}`,
      $referrer: `https://rent.tou.edu.kz/tenders?q=${EMAIL}`,
      $elements: [{ text: NAME }],
      $el_text: EMAIL,
      $pathname: "/objects",
      tender_id: "019fe14b",
    })

    expect(sanitized["$elements"]).toBeUndefined()
    expect(sanitized["$el_text"]).toBeUndefined()
    expect(sanitized["$pathname"]).toBe("/objects")
    expect(sanitized["tender_id"]).toBe("019fe14b")
    const serialized = JSON.stringify(sanitized)
    for (const secret of [NAME, EMAIL]) {
      expect(serialized).not.toContain(secret)
    }
  })

  it("не трогает то, что объектом не является", () => {
    expect(sanitizeProperties(null)).toBeNull()
  })
})
