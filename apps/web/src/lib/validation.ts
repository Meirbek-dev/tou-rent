import * as v from "valibot"
import { m } from "#/paraglide/messages"

/**
 * Схемы форм (арх. v3 § 7): одна схема на форму, те же правила, что и у
 * `garde` на входе api.
 *
 * Зачем паритет. Реквизиты заявителя из Прил. 2 и Прил. 3 - это не поля
 * анкеты: по ИИН/БИН сторона опознается в договоре и в реестре уклонившихся,
 * по телефону ее извещают. Ошибка в них обнаруживалась не при подаче, а при
 * печати договора - то есть после того, как заявка попала в журнал
 * регистрации и стала частью доказательной базы. Проверка обязана стоять
 * в форме, до отправки, и совпадать с серверной: разойдись они - участник
 * получит либо отказ там, где форма разрешила, либо запрет там, где api
 * принял бы.
 *
 * Источник правил - домен (`crates/domain/src/identity.rs`), там же названы
 * границы проверки ИИН/БИН и то, что она намеренно не делает. Общий перечень
 * примеров лежит в `crates/domain/src/identity_samples.json`: его читают
 * и доменные тесты, и `validation.test.ts`.
 */

/** Число разрядов ИИН и БИН. */
export const ID_NUMBER_LENGTH = 12

/** Веса контрольного разряда: первый проход - порядковый номер разряда. */
const ID_NUMBER_WEIGHTS_FIRST = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11] as const

/** Веса второго прохода - когда первый дал остаток 10. */
const ID_NUMBER_WEIGHTS_SECOND = [3, 4, 5, 6, 7, 8, 9, 10, 11, 1, 2] as const

export const PHONE_MIN_DIGITS = 10
export const PHONE_MAX_DIGITS = 15

export const APPLICANT_NAME_MAX_LENGTH = 300
export const ADDRESS_MAX_LENGTH = 500
export const EMAIL_MAX_LENGTH = 254
export const FULL_NAME_MAX_LENGTH = 200
export const PASSWORD_MIN_LENGTH = 12
export const PASSWORD_MAX_LENGTH = 128

/**
 * Потолок на один прикладываемый файл и перечень форматов досье - копия
 * ограничений `crates/http/src/upload.rs`. Числа держатся здесь, а не в
 * формах: иначе следующая форма загрузки заведет свои.
 */
export const MAX_UPLOAD_BYTES = 10 * 1024 * 1024
export const MAX_UPLOAD_MB = MAX_UPLOAD_BYTES / (1024 * 1024)

/** Расширения белого списка досье (PDF, JPEG, PNG, TIFF). */
const UPLOAD_EXTENSIONS = ["pdf", "jpg", "jpeg", "png", "tif", "tiff"] as const

/** Значение атрибута `accept` - отсекает лишнее уже в диалоге выбора файла. */
export const UPLOAD_ACCEPT = UPLOAD_EXTENSIONS.map(
  (extension) => `.${extension}`
).join(",")

/**
 * Контрольный разряд по одиннадцати старшим цифрам; двенадцатая в расчет
 * не входит. `undefined` - номера с такими старшими разрядами не существует
 * (остаток 10 в обоих проходах).
 */
export function controlDigit(digits: readonly number[]): number | undefined {
  const weighted = (weights: readonly number[]): number =>
    weights.reduce(
      (sum, weight, index) => sum + (digits[index] ?? 0) * weight,
      0
    ) % 11

  const first = weighted(ID_NUMBER_WEIGHTS_FIRST)
  const remainder = first === 10 ? weighted(ID_NUMBER_WEIGHTS_SECOND) : first
  return remainder === 10 ? undefined : remainder
}

/** ИИН либо БИН: 12 цифр и сошедшийся контрольный разряд. */
export function isValidIdNumber(value: string): boolean {
  // \d в JS - только ASCII-цифры, как и char::to_digit(10) в домене
  if (!/^\d{12}$/u.test(value)) return false
  // Разбор по индексу, а не разворотом строки: после проверки выше в ней
  // ровно двенадцать однобайтовых цифр
  const digits = Array.from({ length: ID_NUMBER_LENGTH }, (_, index) =>
    Number(value[index])
  )
  return controlDigit(digits) === digits[ID_NUMBER_LENGTH - 1]
}

/** Разделители, которыми номер принято разбивать на группы. */
const PHONE_ALLOWED = /^\+?[\d ()-]*$/u

/** Что именно не так с номером телефона; `undefined` - номер годится. */
export function phoneProblem(
  value: string
): "charset" | "short" | "long" | undefined {
  const trimmed = value.trim()
  if (!PHONE_ALLOWED.test(trimmed)) return "charset"

  const digits = trimmed.match(/\d/gu)?.length ?? 0
  if (digits < PHONE_MIN_DIGITS) return "short"
  if (digits > PHONE_MAX_DIGITS) return "long"
  return undefined
}

/**
 * Сообщения - функциями, а не готовыми строками: Paraglide отдает перевод
 * текущей локали в момент вызова, а схема создается один раз при загрузке
 * модуля. Вычисли их сразу - переключение языка перестало бы менять текст
 * ошибки.
 */
export const idNumberSchema: v.GenericSchema<string> = v.pipe(
  v.string(),
  v.nonEmpty(() => m.validation_required()),
  v.check(isValidIdNumber, () => m.validation_id_number())
)

export const phoneSchema: v.GenericSchema<string> = v.pipe(
  v.string(),
  v.nonEmpty(() => m.validation_required()),
  v.check(
    (value) => phoneProblem(value) !== "charset",
    () => m.validation_phone_charset()
  ),
  v.check(
    (value) =>
      phoneProblem(value) === "charset" || phoneProblem(value) === undefined,
    () =>
      m.validation_phone_length({
        min: PHONE_MIN_DIGITS,
        max: PHONE_MAX_DIGITS,
      })
  )
)

export const applicantNameSchema: v.GenericSchema<string> = v.pipe(
  v.string(),
  v.nonEmpty(() => m.validation_required()),
  v.maxLength(APPLICANT_NAME_MAX_LENGTH, () => m.validation_too_long())
)

export const addressSchema: v.GenericSchema<string> = v.pipe(
  v.string(),
  v.nonEmpty(() => m.validation_required()),
  v.maxLength(ADDRESS_MAX_LENGTH, () => m.validation_too_long())
)

export const emailSchema: v.GenericSchema<string> = v.pipe(
  v.string(),
  v.nonEmpty(() => m.validation_required()),
  v.email(() => m.validation_email()),
  v.maxLength(EMAIL_MAX_LENGTH, () => m.validation_too_long())
)

/** Почта заявителя необязательна (Прил. 2): пустое поле - не ошибка. */
export const optionalEmailSchema: v.GenericSchema<string> = v.pipe(
  v.string(),
  v.check(
    (value) => value === "" || v.safeParse(emailSchema, value).success,
    () => m.validation_email()
  )
)

export const fullNameSchema: v.GenericSchema<string> = v.pipe(
  v.string(),
  v.nonEmpty(() => m.validation_required()),
  v.maxLength(FULL_NAME_MAX_LENGTH, () => m.validation_too_long())
)

export const passwordSchema: v.GenericSchema<string> = v.pipe(
  v.string(),
  v.minLength(PASSWORD_MIN_LENGTH, () =>
    m.auth_password_rule({ min: PASSWORD_MIN_LENGTH })
  ),
  v.maxLength(PASSWORD_MAX_LENGTH, () => m.validation_too_long())
)

/** Сведения заявителя Прил. 2 и Прил. 3 - одна схема на обе формы. */
export const applicantDetailsSchema = v.object({
  name: applicantNameSchema,
  id_number: idNumberSchema,
  address: addressSchema,
  phone: phoneSchema,
  email: optionalEmailSchema,
})

export type ApplicantDetailsInput = v.InferInput<typeof applicantDetailsSchema>

export type ApplicantDetailsErrors = Partial<
  Record<keyof ApplicantDetailsInput, string>
>

/** Первая ошибка каждого поля сведений о заявителе. */
export function applicantDetailsErrors(
  input: ApplicantDetailsInput
): ApplicantDetailsErrors {
  const result = v.safeParse(applicantDetailsSchema, input)
  if (result.success) return {}

  const errors: ApplicantDetailsErrors = {}
  for (const issue of result.issues) {
    const key = issue.path?.[0]?.key
    if (typeof key === "string" && !(key in errors)) {
      errors[key as keyof ApplicantDetailsInput] = issue.message
    }
  }
  return errors
}

/** Сообщение об ошибке поля; `undefined` - значение годится. */
export function fieldError(
  schema: v.GenericSchema<string>,
  value: string
): string | undefined {
  const result = v.safeParse(schema, value)
  return result.success ? undefined : result.issues[0]?.message
}

/**
 * Проверка выбранного файла теми же правилами, что и `upload.rs`: белый
 * список форматов и потолок 10 МБ. Без нее участник узнавал об отказе только
 * после того, как файл целиком ушел на сервер.
 *
 * Расширение проверяется только когда оно есть - как и на сервере: файл без
 * расширения там опознается по сигнатуре содержимого.
 */
export function uploadError(file: File | undefined): string | undefined {
  if (file === undefined) return m.file_not_selected()

  const dot = file.name.lastIndexOf(".")
  const extension = dot > 0 ? file.name.slice(dot + 1).toLowerCase() : ""
  if (
    extension !== "" &&
    !UPLOAD_EXTENSIONS.includes(extension as (typeof UPLOAD_EXTENSIONS)[number])
  ) {
    return m.validation_file_type()
  }

  if (file.size > MAX_UPLOAD_BYTES) {
    return m.validation_file_too_large({ max: MAX_UPLOAD_MB })
  }
  return undefined
}
