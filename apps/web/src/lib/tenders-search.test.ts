import { describe, expect, it } from "vite-plus/test"

import { PUBLIC_STATUSES, validateTendersSearch } from "./tenders-search"

describe("validateTendersSearch (FR-1401: фильтры реестра в URL)", () => {
  it("пропускает каждый публичный статус", () => {
    for (const status of PUBLIC_STATUSES) {
      expect(validateTendersSearch({ status }).status).toBe(status)
    }
  })

  it("отбрасывает неизвестный статус и draft", () => {
    expect(validateTendersSearch({ status: "nonsense" }).status).toBeUndefined()
    // draft в публичном фильтре не предлагается: API его гостю не отдает
    expect(validateTendersSearch({ status: "draft" }).status).toBeUndefined()
  })

  it("отбрасывает пустой и пробельный поисковый запрос", () => {
    expect(validateTendersSearch({ q: "" }).q).toBeUndefined()
    expect(validateTendersSearch({ q: "   " }).q).toBeUndefined()
    expect(validateTendersSearch({ q: 42 }).q).toBeUndefined()
    expect(validateTendersSearch({ q: "склад" }).q).toBe("склад")
  })

  it("нормализует курсор пагинации", () => {
    expect(validateTendersSearch({ after: "" }).after).toBeUndefined()
    expect(validateTendersSearch({ after: "abc" }).after).toBe("abc")
  })

  it("пустой запрос дает пустые фильтры", () => {
    expect(validateTendersSearch({})).toEqual({
      status: undefined,
      q: undefined,
      after: undefined,
    })
  })
})
