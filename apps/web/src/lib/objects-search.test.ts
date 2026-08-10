import { describe, expect, it } from "vite-plus/test"

import {
  OBJECT_KINDS,
  OBJECT_STATUSES,
  validateObjectsSearch,
} from "./objects-search"

describe("validateObjectsSearch (FR-102: фильтры витрины в URL)", () => {
  it("пропускает каждый статус и вид имущества", () => {
    for (const status of OBJECT_STATUSES) {
      expect(validateObjectsSearch({ status }).status).toBe(status)
    }
    for (const kind of OBJECT_KINDS) {
      expect(validateObjectsSearch({ kind }).kind).toBe(kind)
    }
  })

  it("отбрасывает неизвестные статус и вид", () => {
    expect(validateObjectsSearch({ status: "sold" }).status).toBeUndefined()
    expect(validateObjectsSearch({ kind: "hangar" }).kind).toBeUndefined()
  })

  /// Роутер разбирает числовой параметр в number, форма отдает строку
  it("принимает площадь и строкой, и числом", () => {
    expect(validateObjectsSearch({ area_min: "10" }).area_min).toBe(10)
    expect(validateObjectsSearch({ area_min: 10 }).area_min).toBe(10)
    expect(validateObjectsSearch({ area_max: 42.5 }).area_max).toBe(42.5)
    expect(validateObjectsSearch({ area_min: " 30 " }).area_min).toBe(30)
  })

  it("отбрасывает неположительную и нечисловую площадь", () => {
    expect(validateObjectsSearch({ area_min: "0" }).area_min).toBeUndefined()
    expect(validateObjectsSearch({ area_min: "-5" }).area_min).toBeUndefined()
    expect(
      validateObjectsSearch({ area_max: "много" }).area_max
    ).toBeUndefined()
    expect(validateObjectsSearch({ area_max: "" }).area_max).toBeUndefined()
  })

  it("отбрасывает пустой поисковый запрос и курсор", () => {
    expect(validateObjectsSearch({ q: "   " }).q).toBeUndefined()
    expect(validateObjectsSearch({ q: "киоск" }).q).toBe("киоск")
    expect(validateObjectsSearch({ after: "" }).after).toBeUndefined()
  })

  it("пустой запрос дает пустые фильтры", () => {
    expect(validateObjectsSearch({})).toEqual({
      status: undefined,
      kind: undefined,
      q: undefined,
      area_min: undefined,
      area_max: undefined,
      after: undefined,
    })
  })
})
