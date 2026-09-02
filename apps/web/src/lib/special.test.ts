import { describe, expect, it } from "vite-plus/test"

import { specialCategoryLabel } from "./special"

/**
 * Гаунтлет G-013: категория в уведомлении о решении Правления (FR-1202,
 * п. 90) приходит кодом справочника, а подпись к нему выбирает кабинет.
 *
 * До правки сервер клал в payload готовую русскую строку: она оставалась
 * русской и в казахском кабинете, хотя `refdata.special_categories` держит
 * label_kk и label_en. Уведомления, разосланные до правки, кода не несут -
 * их текст обязан остаться читаемым.
 */

const CATEGORIES = [
  {
    code: "cat_4",
    label_ru: "Категория № 4",
    label_kk: "№ 4 санат",
    label_en: "Category No. 4",
  },
]

describe("подпись категории уведомления", () => {
  it("берется из справочника по коду", () => {
    expect(specialCategoryLabel({ category_code: "cat_4" }, CATEGORIES)).toBe(
      "Категория № 4"
    )
  })

  it("справочник не загружен - остается текст самого уведомления", () => {
    expect(
      specialCategoryLabel({
        category_code: "cat_4",
        category_ru: "Категория № 4 (п. 87.4)",
      })
    ).toBe("Категория № 4 (п. 87.4)")
  })

  it("уведомление, разосланное до правки, читается по прежнему полю", () => {
    expect(
      specialCategoryLabel({ category: "Категория № 1 (п. 87.1)" }, CATEGORIES)
    ).toBe("Категория № 1 (п. 87.1)")
  })

  it("код, которого нет в справочнике, не превращается в пустоту", () => {
    expect(specialCategoryLabel({ category_code: "cat_9" }, CATEGORIES)).toBe(
      "cat_9"
    )
  })
})
