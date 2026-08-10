import { readFileSync } from "node:fs"
import { join } from "node:path"

import { describe, expect, it } from "vite-plus/test"

import { deadlineLabel } from "./obligation-labels"

/**
 * Паритет подписей сроков с каталогом Правил (FR-1702).
 *
 * Каталог закрыт enum'ом в `domain::obligation::ObligationAction`, а до
 * интерфейса доезжает строкой контракта (`ObligationDto.action: string`) -
 * компилятор такую пару не сверит. Поэтому сверяет тест: он читает сам
 * каталог и требует подпись для каждого его кода. Без этого пропущенное
 * действие молча показывалось машинным кодом - так в дашборде «мои сроки»
 * четырнадцать сроков из восемнадцати выглядели как `contract_draft`.
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
  "obligation.rs"
)

/** Коды действий из `ObligationAction::as_str`. */
function catalogCodes(): string[] {
  const source = readFileSync(CATALOG, "utf8")
  const body = source.slice(source.indexOf("pub fn as_str"))
  const arm = /ObligationAction::\w+ => "(\w+)"/g
  return [...body.matchAll(arm)].map((match) => match[1] as string)
}

describe("каталог сроков", () => {
  it("читается из домена и не пуст", () => {
    const codes = catalogCodes()
    expect(codes.length).toBeGreaterThanOrEqual(18)
    expect(new Set(codes).size).toBe(codes.length)
  })

  it("каждое действие каталога имеет подпись", () => {
    const untranslated = catalogCodes().filter(
      (code) => deadlineLabel(code) === code
    )
    expect(untranslated).toEqual([])
  })

  it("неизвестное действие не теряется, а показывается кодом", () => {
    expect(deadlineLabel("не_из_каталога")).toBe("не_из_каталога")
  })
})
