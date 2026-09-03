import { describe, expect, it } from "vite-plus/test"

import {
  matchesTenderFilter,
  nearestTenderDate,
  splitSecretaryWorkList,
} from "./organizer"

/**
 * Гаунтлет G-148 и G-154: рабочий список секретаря шел от самого старого
 * тендера вместе с завершенными и отмененными - живое заседание оказывалось
 * тридцать восьмой строкой; реестр организатора не имел ни фильтра, ни
 * поиска.
 */

const tender = (
  status: string,
  opening: string | null,
  title = "Аренда помещения",
  titleKk = "Үй-жайды жалға алу"
) => ({
  status,
  title,
  title_kk: titleKk,
  opening_at: opening,
  submission_deadline: null,
})

describe("рабочий список секретаря", () => {
  it("просроченное вскрытие идет выше будущего", () => {
    const { active } = splitSecretaryWorkList([
      tender("accepting", "2026-12-01T09:00:00Z"),
      tender("qualification", "2026-01-10T09:00:00Z"),
      tender("accepting", "2026-06-01T09:00:00Z"),
    ])
    expect(active.map((t) => t.opening_at)).toEqual([
      "2026-01-10T09:00:00Z",
      "2026-06-01T09:00:00Z",
      "2026-12-01T09:00:00Z",
    ])
  })

  it("завершенные уезжают из работы в историю", () => {
    const { active, completed } = splitSecretaryWorkList([
      tender("cancelled", "2020-01-01T09:00:00Z"),
      tender("contracted", "2021-01-01T09:00:00Z"),
      tender("failed", "2022-01-01T09:00:00Z"),
      tender("accepting", "2026-06-01T09:00:00Z"),
    ])
    expect(active).toHaveLength(1)
    expect(completed.map((t) => t.status)).toEqual([
      "cancelled",
      "contracted",
      "failed",
    ])
  })

  it("черновик в кабинет секретаря не попадает вовсе", () => {
    const { active, completed } = splitSecretaryWorkList([
      tender("draft", "2026-06-01T09:00:00Z"),
    ])
    expect(active).toHaveLength(0)
    expect(completed).toHaveLength(0)
  })

  it("тендер без сроков уходит вниз, а не наверх", () => {
    expect(
      nearestTenderDate({ opening_at: null, submission_deadline: null })
    ).toBe("9999")
    const { active } = splitSecretaryWorkList([
      tender("accepting", null),
      tender("accepting", "2026-06-01T09:00:00Z"),
    ])
    expect(active[0]?.opening_at).toBe("2026-06-01T09:00:00Z")
  })
})

describe("фильтр реестра тендеров", () => {
  it("пустой фильтр пропускает всё", () => {
    expect(
      matchesTenderFilter(tender("draft", null), { status: "", query: "" })
    ).toBe(true)
  })

  it("статус отбирается точным совпадением", () => {
    const row = tender("announced", null)
    expect(matchesTenderFilter(row, { status: "announced", query: "" })).toBe(
      true
    )
    expect(matchesTenderFilter(row, { status: "draft", query: "" })).toBe(false)
  })

  it("поиск идет по обоим названиям и не зависит от регистра", () => {
    const row = tender("draft", null, "Аренда буфета", "Буфетті жалға алу")
    expect(matchesTenderFilter(row, { status: "", query: "БУФЕТА" })).toBe(true)
    expect(matchesTenderFilter(row, { status: "", query: "жалға" })).toBe(true)
    expect(matchesTenderFilter(row, { status: "", query: "склад" })).toBe(false)
  })

  it("пробелы вокруг запроса не отсекают совпадение", () => {
    expect(
      matchesTenderFilter(tender("draft", null, "Аренда буфета"), {
        status: "",
        query: "  буфета  ",
      })
    ).toBe(true)
  })
})
