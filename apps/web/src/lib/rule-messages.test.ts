import { readFileSync } from "node:fs"
import { join } from "node:path"

import { describe, expect, it } from "vite-plus/test"

import { ruleMessage } from "./rule-messages"

/**
 * Полнота текстов причин отказа (NFR-01, W-09).
 *
 * Перечень причин закрыт enum'ом в `domain::rule::RuleViolation`, а до
 * интерфейса доезжает строкой контракта - компилятор такую пару не сверит.
 * Пока сверки не было, объяснением отказа служило сообщение триггера БД:
 * русский текст с именем инварианта, одинаковый для всех трех локалей.
 *
 * Тест текстовый намеренно: он читает сам каталог домена, не поднимая ни
 * Rust, ни React.
 */

const CATALOG = join(
  import.meta.dirname,
  "..",
  "..",
  "..",
  "..",
  "crates",
  "domain",
  "src",
  "rule.rs"
)

/** Коды причин из `RuleViolation::as_str`. */
function catalogRules(): string[] {
  const source = readFileSync(CATALOG, "utf8")
  const body = source.slice(source.indexOf("pub fn as_str"))
  const arm = /RuleViolation::\w+ => "(\w+)"/g
  return [...body.matchAll(arm)].map((match) => match[1] as string)
}

describe("каталог причин отказа", () => {
  it("читается из домена и не пуст", () => {
    const rules = catalogRules()
    expect(rules.length).toBeGreaterThanOrEqual(50)
    expect(new Set(rules).size).toBe(rules.length)
  })

  it("у каждой причины каталога есть свой текст", () => {
    const fallback = ruleMessage("заведомо_неизвестная_причина")
    const untranslated = catalogRules().filter(
      (rule) => ruleMessage(rule) === fallback
    )
    expect(untranslated).toEqual([])
  })

  it("тексты причин не повторяются - иначе отказ ничего не объясняет", () => {
    const texts = catalogRules().map((rule) => ruleMessage(rule))
    expect(new Set(texts).size).toBe(texts.length)
  })

  it("неизвестная причина объясняется словами, а не кодом и не пустотой", () => {
    const fallback = ruleMessage("rule_from_the_future")
    expect(fallback).not.toBe("")
    expect(fallback).not.toContain("rule_from_the_future")
  })
})
