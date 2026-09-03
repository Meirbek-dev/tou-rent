import { describe, expect, it } from "vite-plus/test"

import { DECIDED_STATUSES, mergeDecided } from "./board"

/**
 * Гаунтлет G-132: принятое решение в кабинете Правления больше не открыть -
 * заявка уходила из очереди и не появлялась нигде. Контракт фильтрует
 * список по одному состоянию за запрос, поэтому решенные собираются из
 * трех выборок.
 */

const row = (id: string, submitted_at: string) => ({ id, submitted_at })

describe("перечень принятых решений", () => {
  it("состояния решения - закрытый перечень п. 90", () => {
    expect([...DECIDED_STATUSES]).toEqual(["granted", "refused", "redirected"])
  })

  it("собирает выборки в один список, свежие сверху", () => {
    const merged = mergeDecided([
      [row("a", "2026-09-01T10:00:00Z")],
      [row("b", "2026-09-03T10:00:00Z")],
      [row("c", "2026-09-02T10:00:00Z")],
    ])
    expect(merged.map((item) => item.id)).toEqual(["b", "c", "a"])
  })

  it("невыполненный запрос пропускается, а не роняет список", () => {
    const merged = mergeDecided([
      undefined,
      [row("b", "2026-09-03T10:00:00Z")],
      undefined,
    ])
    expect(merged.map((item) => item.id)).toEqual(["b"])
  })

  it("одна заявка не встает в перечень дважды", () => {
    const merged = mergeDecided([
      [row("a", "2026-09-01T10:00:00Z")],
      [row("a", "2026-09-01T10:00:00Z")],
    ])
    expect(merged).toHaveLength(1)
  })

  it("пустые выборки дают пустой перечень", () => {
    expect(mergeDecided([[], undefined])).toEqual([])
  })
})
