import { readFileSync } from "node:fs"
import { join } from "node:path"

import { describe, expect, it } from "vite-plus/test"

import { problemDetail } from "./auth"
import { errorCodeMessage } from "./error-codes"

/**
 * Полнота текстов кодов отказа (NFR-01, NFR-08).
 *
 * Перечень `ErrorCode` закрыт схемой контракта, а до интерфейса доезжает
 * строкой - компилятор такую пару не сверит. Пока сверки не было, из
 * семнадцати кодов свой текст был у нуля: отказ объяснялся `title` или
 * `detail` сервера, то есть русским делопроизводственным текстом на /en/
 * и /kk/ (у `validation_failed` - английским текстом garde).
 *
 * Тест текстовый намеренно: он читает сам контракт, не поднимая ни Rust,
 * ни React.
 */

const CONTRACT = join(
  import.meta.dirname,
  "..",
  "..",
  "..",
  "..",
  "packages",
  "api-client",
  "openapi.json"
)

/** Коды из `components.schemas.ErrorCode.enum`. */
function contractCodes(): string[] {
  const source = readFileSync(CONTRACT, "utf8")
  const schema = JSON.parse(source) as {
    components: { schemas: { ErrorCode: { enum: string[] } } }
  }
  return schema.components.schemas.ErrorCode.enum
}

describe("коды отказа контракта", () => {
  it("перечень читается из контракта и не пуст", () => {
    const codes = contractCodes()
    expect(codes.length).toBeGreaterThanOrEqual(15)
    expect(new Set(codes).size).toBe(codes.length)
  })

  it("у каждого кода контракта есть свой текст", () => {
    const untranslated = contractCodes().filter(
      (code) => errorCodeMessage(code) === null
    )
    expect(untranslated).toEqual([])
  })

  it("тексты кодов не повторяются - иначе отказ ничего не объясняет", () => {
    const texts = contractCodes().map((code) => errorCodeMessage(code))
    expect(new Set(texts).size).toBe(texts.length)
  })

  it("код вне перечня текста не имеет", () => {
    expect(errorCodeMessage("code_from_the_future")).toBeNull()
  })

  it("текст кода важнее русского title сервера", () => {
    const problem = {
      type: "urn:tou-rent:error:invalid_credentials",
      title: "Неверный email или пароль",
      status: 401,
      code: "invalid_credentials",
    }
    expect(problemDetail(problem)).toBe(errorCodeMessage("invalid_credentials"))
  })

  it("validation_failed оставляет detail сервера второй строкой", () => {
    const detail = "email: not a valid email"
    const shown = problemDetail({
      type: "urn:tou-rent:error:validation_failed",
      title: "Validation failed",
      status: 422,
      code: "validation_failed",
      detail,
    })
    expect(shown).toContain(errorCodeMessage("validation_failed") ?? "")
    expect(shown).toContain(detail)
  })

  it("отказ по правилу остается за каталогом причин", () => {
    const shown = problemDetail({
      type: "urn:tou-rent:error:rule_violation",
      title: "Rule violation",
      status: 409,
      code: "rule_violation",
      rule: "bid_below_minimum",
    })
    expect(shown).not.toBe(errorCodeMessage("rule_violation"))
    expect(shown).not.toBeNull()
  })
})
