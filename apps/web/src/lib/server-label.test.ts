import { readFileSync, readdirSync } from "node:fs"
import { join, relative } from "node:path"

import { describe, expect, it } from "vite-plus/test"

/**
 * Справочники сервера показываются через `serverLabel` (NFR-01).
 *
 * Подписи справочников приходят тремя колонками (`*_ru` / `*_kk` / `*_en`),
 * и правило отката живет в одном месте - `serverLabel`. Разметка, читающая
 * `label_ru` напрямую, это правило обходит: на /en/ и /kk/ список оснований
 * возврата и таблица коэффициентов оставались русскими при заполненных
 * kk/en-колонках.
 *
 * Проверка текстовая: она читает разметку, не поднимая React. Ловится
 * именно вывод - `{item.label_ru}` в JSX, - а не передача колонки в тело
 * запроса, где ru-подпись и должна остаться ru-подписью.
 */

const SRC = join(import.meta.dirname, "..")

/**
 * Колонки без пары kk/en в контракте: подпись праздника заводит админ
 * одним полем (`HolidayDto`), переводить нечего.
 */
const ALLOWED = new Set(["holiday.label_ru"])

function markup(dir: string = SRC): string[] {
  const found: string[] = []
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) {
      if (entry.name === "paraglide") continue
      found.push(...markup(path))
    } else if (entry.name.endsWith(".tsx")) {
      found.push(path)
    }
  }
  return found
}

/** Выражения JSX вида `{что-то.колонка_ru}` - строка начинается со скобки. */
function directRenders(): string[] {
  const hits: string[] = []
  for (const file of markup()) {
    const lines = readFileSync(file, "utf8").split("\n")
    lines.forEach((line, index) => {
      const match = /^\s*\{.*?(\w+\.\w*_ru)\b/.exec(line)
      const field = match?.[1]
      if (field !== undefined && !ALLOWED.has(field)) {
        hits.push(`${relative(SRC, file)}:${index + 1} ${field}`)
      }
    })
  }
  return hits
}

describe("подписи справочников сервера", () => {
  it("разметка не читает колонку *_ru в обход serverLabel", () => {
    expect(directRenders()).toEqual([])
  })

  it("проверка вообще смотрит на разметку", () => {
    expect(markup().length).toBeGreaterThan(50)
  })
})
