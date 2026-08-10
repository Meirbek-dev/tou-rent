import { readFileSync } from "node:fs"
import { join } from "node:path"

import { describe, expect, it } from "vite-plus/test"

/**
 * Полнота веток центра уведомлений (FR-1301).
 *
 * Виды событий закрыты enum'ом в `domain::notification::NotificationKind`,
 * а до интерфейса доезжают строкой контракта - компилятор такую пару не
 * сверит. Пока сверки не было, из семи видов колокольчик знал два, и
 * участник видел в списке `protocol_published` вместо человеческого текста.
 *
 * Тест текстовый намеренно: он читает каталог и компонент, не поднимая
 * React, - иначе понадобился бы браузерный рантайм ради проверки, которая
 * состоит в одном сравнении множеств.
 */

const ROOT = join(import.meta.dirname, "..", "..", "..", "..")
const CATALOG = join(ROOT, "crates", "domain", "src", "notification.rs")
const BELL = join(
  ROOT,
  "apps",
  "web",
  "src",
  "components",
  "notification-bell.tsx"
)

/** Коды видов из `NotificationKind::as_str`. */
function catalogKinds(): string[] {
  const source = readFileSync(CATALOG, "utf8")
  const body = source.slice(source.indexOf("pub fn as_str"))
  const arm = /NotificationKind::\w+ => "(\w+)"/g
  return [...body.matchAll(arm)].map((match) => match[1] as string)
}

/** Виды, у которых в колокольчике есть своя ветка. */
function renderedKinds(): string[] {
  const source = readFileSync(BELL, "utf8")
  return [...source.matchAll(/case "(\w+)":/g)].map(
    (match) => match[1] as string
  )
}

describe("центр уведомлений", () => {
  it("каталог видов читается и не пуст", () => {
    const kinds = catalogKinds()
    expect(kinds.length).toBeGreaterThanOrEqual(8)
    expect(new Set(kinds).size).toBe(kinds.length)
  })

  it("каждый вид события имеет свой текст", () => {
    const rendered = new Set(renderedKinds())
    const missing = catalogKinds().filter((kind) => !rendered.has(kind))
    expect(missing).toEqual([])
  })

  it("лишних веток нет: рендерится только то, что есть в каталоге", () => {
    const catalog = new Set(catalogKinds())
    const extra = renderedKinds().filter((kind) => !catalog.has(kind))
    expect(extra).toEqual([])
  })
})
