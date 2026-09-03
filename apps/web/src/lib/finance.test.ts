import { describe, expect, it } from "vite-plus/test"

import { codeLabel, matchesLedgerAccount } from "./finance"

import type { CodeLabelSource, LedgerAccountRow } from "./finance"

/**
 * Гаунтлет G-126, G-129, G-130.
 *
 * Книга на тридцати счетах шла одной лентой без фильтра (G-129), проводка
 * возврата не называла основания (G-126), а реестр уклонистов - основания
 * уклонения (G-130). Оба основания приходят кодом закрытого перечня, и
 * подпись к нему берется из справочника, а не из Paraglide.
 */

const REASONS: CodeLabelSource[] = [
  {
    code: "not_admitted",
    label_ru: "Заявка не допущена",
    label_kk: "Өтінім жіберілмеді",
    label_en: "Application not admitted",
  },
  { code: "tender_failed", label_ru: "Тендер не состоялся" },
]

const ACCOUNT: LedgerAccountRow = {
  kind: "participant_fee",
  owner_name: "ТОО «Демо-Участник»",
  tender_title: "Аренда аудитории 42 м²",
}

const DEPOSIT: LedgerAccountRow = {
  kind: "contract_deposit",
  owner_name: "ИП Сериков",
  tender_title: null,
}

describe("подпись кода справочника", () => {
  it("берется из справочника по коду", () => {
    expect(codeLabel("not_admitted", REASONS)).toBe("Заявка не допущена")
  })

  it("кода нет - строку рисовать нечем", () => {
    expect(codeLabel(null, REASONS)).toBeNull()
    expect(codeLabel("", REASONS)).toBeNull()
  })

  it("справочник не загрузился - остается код, а не пустота", () => {
    expect(codeLabel("not_admitted", undefined)).toBe("not_admitted")
  })

  it("код вне справочника контракт обгоняет экран - показывается сам код", () => {
    expect(codeLabel("unknown_ground", REASONS)).toBe("unknown_ground")
  })
})

describe("отбор счета депозитной книги", () => {
  it("без условий проходит любой счет", () => {
    expect(matchesLedgerAccount(ACCOUNT, { kind: "", query: "" })).toBe(true)
  })

  it("вид счета отбирает взносы от депозитов", () => {
    expect(
      matchesLedgerAccount(ACCOUNT, { kind: "participant_fee", query: "" })
    ).toBe(true)
    expect(
      matchesLedgerAccount(ACCOUNT, { kind: "contract_deposit", query: "" })
    ).toBe(false)
  })

  it("подстрока ищется и по владельцу, и по тендеру, без учета регистра", () => {
    expect(matchesLedgerAccount(ACCOUNT, { kind: "", query: "демо" })).toBe(
      true
    )
    expect(
      matchesLedgerAccount(ACCOUNT, { kind: "", query: "АУДИТОРИИ" })
    ).toBe(true)
    expect(matchesLedgerAccount(ACCOUNT, { kind: "", query: "склад" })).toBe(
      false
    )
  })

  it("счет без тендера ищется по одному владельцу", () => {
    expect(matchesLedgerAccount(DEPOSIT, { kind: "", query: "сериков" })).toBe(
      true
    )
    expect(matchesLedgerAccount(DEPOSIT, { kind: "", query: "аренда" })).toBe(
      false
    )
  })

  it("пробелы вокруг запроса не отсекают строки", () => {
    expect(matchesLedgerAccount(ACCOUNT, { kind: "", query: "  " })).toBe(true)
    expect(matchesLedgerAccount(ACCOUNT, { kind: "", query: " демо " })).toBe(
      true
    )
  })
})
