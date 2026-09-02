import { describe, expect, it } from "vite-plus/test"

import { rateOptionLabel, rateOptionLabelOf } from "./organizer"

import type { RateOption } from "./organizer"

/**
 * Гаунтлет G-057: опции коэффициентов Прил. 4 называются словами (FR-202).
 *
 * Справочник `refdata.rate_coefficients` ведет названия опций, но контракт
 * их не отдавал, и калькулятор организатора показывал машинный код -
 * «default (1.0000)». Код остается запасным: строка справочника без
 * названия все же лучше пустого пункта списка.
 */

const OPTION: RateOption = {
  coefficient: "ksots",
  option_code: "social",
  label_ru: "Социально значимая деятельность",
  label_kk: "Әлеуметтік маңызы бар қызмет",
  label_en: "Socially significant activity",
  value: "0.5000",
}

const NAMELESS: RateOption = {
  coefficient: "kt",
  option_code: "default",
  label_ru: "",
  label_kk: null,
  label_en: null,
  value: "1.0000",
}

describe("подпись опции коэффициента", () => {
  it("берется из справочника, а не из кода опции", () => {
    expect(rateOptionLabel(OPTION)).toBe("Социально значимая деятельность")
  })

  it("без названия остается код - пункт списка не бывает пустым", () => {
    expect(rateOptionLabel(NAMELESS)).toBe("default")
  })

  it("расшифровка расчета находит опцию по паре «множитель + код»", () => {
    expect(rateOptionLabelOf([OPTION, NAMELESS], "ksots", "social")).toBe(
      "Социально значимая деятельность"
    )
  })

  it("опции нет в каталоге - показывается код снимка (FR-202)", () => {
    expect(rateOptionLabelOf([OPTION], "kt", "default")).toBe("default")
  })
})
