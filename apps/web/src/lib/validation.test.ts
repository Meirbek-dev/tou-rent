import { readFileSync } from "node:fs"
import { join } from "node:path"

import { describe, expect, it } from "vite-plus/test"

import {
  MAX_UPLOAD_BYTES,
  PHONE_MAX_DIGITS,
  PHONE_MIN_DIGITS,
  UPLOAD_ACCEPT,
  applicantDetailsErrors,
  controlDigit,
  fieldError,
  idNumberSchema,
  isValidIdNumber,
  optionalEmailSchema,
  passwordSchema,
  phoneSchema,
  uploadError,
} from "./validation"

/**
 * Паритет схем Valibot с `garde` на входе api (W-13, арх. v3 § 7).
 *
 * Правила живут в домене (`crates/domain/src/identity.rs`), схемы - здесь,
 * а сверяет их общий перечень примеров: тот же файл читает доменный тест.
 * Разойдись правила - один и тот же пример даст разный ответ по разные
 * стороны сети, и участник получит либо отказ там, где форма разрешила,
 * либо запрет там, где api принял бы.
 */
const SAMPLES = JSON.parse(
  readFileSync(
    join(
      import.meta.dirname,
      "..",
      "..",
      "..",
      "..",
      "crates",
      "domain",
      "src",
      "identity_samples.json"
    ),
    "utf8"
  )
) as Record<string, string[]>

const samples = (field: string): string[] => SAMPLES[field] ?? []

/** Цифры номера по индексу - так же, как их разбирает сама проверка. */
const asDigits = (value: string): number[] =>
  Array.from({ length: 12 }, (_, index) => Number(value[index]))

const repeatDigit = (count: number): string => "7".repeat(count)

/** Файл нужен формам только именем и размером - остального в jsdom нет. */
const fakeFile = (name: string, size: number): File =>
  ({ name, size }) as unknown as File

describe("общий перечень примеров: Valibot ↔ garde", () => {
  it("перечень найден и не пуст", () => {
    for (const field of [
      "id_number_valid",
      "id_number_invalid",
      "phone_valid",
      "phone_invalid",
    ]) {
      expect(samples(field).length).toBeGreaterThan(0)
    }
  })

  it("ИИН/БИН из перечня принимаются", () => {
    const refused = samples("id_number_valid").filter(
      (value) => fieldError(idNumberSchema, value) !== undefined
    )
    expect(refused).toEqual([])
  })

  it("ИИН/БИН вне перечня отклоняются", () => {
    const passed = samples("id_number_invalid").filter(
      (value) => fieldError(idNumberSchema, value) === undefined
    )
    expect(passed).toEqual([])
  })

  it("телефоны из перечня принимаются", () => {
    const refused = samples("phone_valid").filter(
      (value) => fieldError(phoneSchema, value) !== undefined
    )
    expect(refused).toEqual([])
  })

  it("телефоны вне перечня отклоняются", () => {
    const passed = samples("phone_invalid").filter(
      (value) => fieldError(phoneSchema, value) === undefined
    )
    expect(passed).toEqual([])
  })
})

describe("контрольный разряд ИИН/БИН", () => {
  const FIRST_WEIGHTS = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]

  /// Первый проход дал остаток 10 - разряд считается вторым набором весов
  it("второй проход дает действительный номер", () => {
    for (const value of ["810203415845", "890904412915", "100650878412"]) {
      const digits = asDigits(value)
      const first =
        FIRST_WEIGHTS.reduce(
          (sum, weight, index) => sum + (digits[index] ?? 0) * weight,
          0
        ) % 11
      expect(first).toBe(10)
      expect(isValidIdNumber(value)).toBe(true)
    }
  })

  /// Остаток 10 в обоих проходах - номера не существует ни с какой
  /// двенадцатой цифрой
  it("номер без контрольного разряда отклоняется при любом хвосте", () => {
    for (const prefix of ["62080837541", "65100245280"]) {
      for (let tail = 0; tail <= 9; tail += 1) {
        const value = `${prefix}${tail}`
        expect(controlDigit(asDigits(value))).toBeUndefined()
        expect(isValidIdNumber(value)).toBe(false)
      }
    }
  })

  /// Граница проверки названа вслух и в домене: арифметика не доказывает,
  /// что номер кому-то выдан
  it("арифметически верный несуществующий номер проходит", () => {
    expect(isValidIdNumber("000000000000")).toBe(true)
  })
})

describe("границы телефона", () => {
  it("крайние длины принимаются, соседние - нет", () => {
    expect(
      fieldError(phoneSchema, repeatDigit(PHONE_MIN_DIGITS))
    ).toBeUndefined()
    expect(
      fieldError(phoneSchema, repeatDigit(PHONE_MAX_DIGITS))
    ).toBeUndefined()
    expect(
      fieldError(phoneSchema, repeatDigit(PHONE_MIN_DIGITS - 1))
    ).toBeDefined()
    expect(
      fieldError(phoneSchema, repeatDigit(PHONE_MAX_DIGITS + 1))
    ).toBeDefined()
  })
})

describe("прочие поля форм", () => {
  it("почта заявителя необязательна, но проверяется, когда заполнена", () => {
    expect(fieldError(optionalEmailSchema, "")).toBeUndefined()
    expect(fieldError(optionalEmailSchema, "user@tou.test")).toBeUndefined()
    expect(fieldError(optionalEmailSchema, "user@")).toBeDefined()
  })

  /// Парольная политика api - не короче 12 символов
  it("короткий пароль отклоняется до отправки", () => {
    expect(fieldError(passwordSchema, "12345678901")).toBeDefined()
    expect(fieldError(passwordSchema, "123456789012")).toBeUndefined()
  })

  it("ошибки сведений о заявителе разложены по полям", () => {
    const errors = applicantDetailsErrors({
      name: "",
      id_number: "123",
      address: "г. Павлодар",
      phone: "+7 701 123 45 67",
      email: "",
    })
    expect(errors.name).toBeDefined()
    expect(errors.id_number).toBeDefined()
    expect(errors.address).toBeUndefined()
    expect(errors.phone).toBeUndefined()
    expect(errors.email).toBeUndefined()
  })
})

describe("ограничения загрузки досье (upload.rs)", () => {
  it("accept перечисляет белый список форматов", () => {
    expect(UPLOAD_ACCEPT).toBe(".pdf,.jpg,.jpeg,.png,.tif,.tiff")
  })

  it("файл сверх потолка отклоняется до отправки", () => {
    expect(uploadError(fakeFile("скан.pdf", MAX_UPLOAD_BYTES))).toBeUndefined()
    expect(
      uploadError(fakeFile("скан.pdf", MAX_UPLOAD_BYTES + 1))
    ).toBeDefined()
  })

  it("формат вне белого списка отклоняется", () => {
    expect(uploadError(fakeFile("досье.zip", 1024))).toBeDefined()
    expect(uploadError(fakeFile("скан.TIFF", 1024))).toBeUndefined()
    // Имя без расширения решает сервер по сигнатуре содержимого
    expect(uploadError(fakeFile("attachment", 1024))).toBeUndefined()
  })

  it("несделанный выбор файла - тоже ошибка формы", () => {
    expect(uploadError(undefined)).toBeDefined()
  })
})
