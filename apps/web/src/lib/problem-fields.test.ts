import { describe, expect, it } from "vite-plus/test"

import { problemFieldMessages } from "./problem-fields"

import type { FieldLabels } from "./problem-fields"

/**
 * Гаунтлет G-106: форма показывала машинную подробность отказа.
 *
 * «Проверьте заполнение формы (value_out_of_range)», «(name_kk: must not be
 * blank)» и `unknown_option_code:objects_premises_type_option_fkey` - это
 * SQLSTATE, имя ограничения БД и английский текст garde. Заполняющему форму
 * они не говорят ни какое поле неверно, ни что с ним не так.
 */

const LABELS: FieldLabels = {
  name: () => "Наименование (русский)",
  name_kk: () => "Наименование (қазақша)",
  address: () => "Адрес (русский)",
  address_kk: () => "Адрес (қазақша)",
  area_m2: () => "Площадь",
  premises_type: () => "Тип помещения (Кт)",
}

const problem = (detail: string) => ({ code: "validation_failed", detail })

describe("сообщения отказа по полям формы", () => {
  it("переполнение numeric называет поле формы, а не SQLSTATE", () => {
    expect(
      problemFieldMessages(problem("value_out_of_range"), LABELS, "area_m2")
    ).toEqual(["Площадь: значение вне допустимого диапазона"])
  })

  it("без указанного числового поля переполнение остается неразобранным", () => {
    expect(problemFieldMessages(problem("value_out_of_range"), LABELS)).toEqual(
      []
    )
  })

  it("имя CHECK-ограничения разворачивается в подпись поля", () => {
    expect(
      problemFieldMessages(
        problem("blank_value:objects_name_kk_not_blank"),
        LABELS
      )
    ).toEqual([
      "Наименование (қазақша): заполните поле - пустое значение не принимается",
    ])
  })

  it("суффикс поля выбирается по самому длинному совпадению", () => {
    // `objects_address_kk` не должен опознаться как `address`
    expect(
      problemFieldMessages(
        problem("blank_value:objects_address_kk_not_blank"),
        LABELS
      )
    ).toEqual([
      "Адрес (қазақша): заполните поле - пустое значение не принимается",
    ])
  })

  it("неизвестный код опции Прил. 4 отсылает к справочнику", () => {
    expect(
      problemFieldMessages(
        problem("unknown_option_code:objects_premises_type_option_fkey"),
        LABELS
      )
    ).toEqual(["Тип помещения (Кт): выберите значение из справочника"])
  })

  it("отчет garde разбирается построчно, английский текст наружу не идет", () => {
    expect(
      problemFieldMessages(
        problem("name_kk: must not be blank\narea_m2: площадь должна быть > 0"),
        LABELS
      )
    ).toEqual([
      "Наименование (қазақша): заполните поле - пустое значение не принимается",
      "Площадь: значение не принято",
    ])
  })

  it("поле опции узнается и без суффикса _code", () => {
    expect(
      problemFieldMessages(problem("premises_type_code: is too long"), LABELS)
    ).toEqual(["Тип помещения (Кт): значение не принято"])
  })

  it("чужое поле не выдумывается - вызывающий покажет общий текст", () => {
    expect(
      problemFieldMessages(problem("floor_part: is too long"), LABELS)
    ).toEqual([])
  })

  it("отказ не по валидации разбору не подлежит", () => {
    expect(
      problemFieldMessages(
        { code: "rule_violation", rule: "object_in_use", detail: "занят" },
        LABELS,
        "area_m2"
      )
    ).toEqual([])
    expect(problemFieldMessages(new Error("network"), LABELS)).toEqual([])
    expect(problemFieldMessages(null, LABELS)).toEqual([])
  })
})
