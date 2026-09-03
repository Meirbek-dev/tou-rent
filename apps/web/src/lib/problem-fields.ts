import { m } from "#/paraglide/messages"

import type { components } from "@tou/api-client"

type Problem = components["schemas"]["Problem"]

/**
 * Подписи полей формы для разбора отказа: ключ - имя поля контракта либо
 * общая часть имени ограничения БД (`premises_type` для
 * `objects_premises_type_option_fkey`).
 */
export type FieldLabels = Readonly<Record<string, () => string>>

/** Машинные подробности, которые сервер отдает вместо имени поля. */
const OUT_OF_RANGE = "value_out_of_range"
const BLANK_PREFIX = "blank_value:"
const OPTION_PREFIX = "unknown_option_code:"

/**
 * Отказ валидации, разложенный на сообщения по полям.
 *
 * `problemMessage` отвечает на вопрос «что случилось» и на отказе валидации
 * дописывает машинную подробность как есть: «Проверьте заполнение формы
 * (value_out_of_range)», «(name_kk: must not be blank)»,
 * `unknown_option_code:objects_premises_type_option_fkey`. Имя ограничения
 * БД и английский текст garde - это внутренности сервера, и заполняющему
 * форму они не говорят ни какое поле неверно, ни что с ним не так.
 *
 * Здесь те же подробности переводятся в строки «Площадь: значение вне
 * допустимого диапазона». Пустой список значит «подробность незнакомая» -
 * вызывающий показывает общий текст, а не выдумывает поле.
 *
 * `rangeField` - поле, к которому относится переполнение numeric: SQLSTATE
 * 22003 колонку не называет, и знает ее только форма (у объекта числовое
 * поле одно - площадь).
 */
export function problemFieldMessages(
  error: unknown,
  labels: FieldLabels,
  rangeField?: string
): string[] {
  const detail = validationDetail(error)
  if (detail === null) return []

  const messages: string[] = []
  for (const line of detail.split("\n")) {
    const message = lineMessage(line.trim(), labels, rangeField)
    if (message !== null && !messages.includes(message)) messages.push(message)
  }
  return messages
}

/** Подробность именно отказа валидации: остальные коды разбору не подлежат. */
function validationDetail(error: unknown): string | null {
  if (error === null || typeof error !== "object") return null
  const problem = error as Partial<Problem>
  if (problem.code !== "validation_failed") return null
  return typeof problem.detail === "string" && problem.detail !== ""
    ? problem.detail
    : null
}

function lineMessage(
  line: string,
  labels: FieldLabels,
  rangeField: string | undefined
): string | null {
  if (line === "") return null

  if (line === OUT_OF_RANGE) {
    const label = rangeField === undefined ? undefined : labels[rangeField]
    return label === undefined
      ? null
      : m.field_error_out_of_range({ field: label() })
  }

  if (line.startsWith(BLANK_PREFIX)) {
    const label = constraintLabel(line.slice(BLANK_PREFIX.length), labels)
    return label === null ? null : m.field_error_blank({ field: label })
  }

  if (line.startsWith(OPTION_PREFIX)) {
    const label = constraintLabel(line.slice(OPTION_PREFIX.length), labels)
    return label === null
      ? null
      : m.field_error_unknown_option({ field: label })
  }

  // Отчет garde: «<поле>: <английский текст правила>» построчно
  const separator = line.indexOf(":")
  if (separator <= 0) return null
  const label = fieldLabel(line.slice(0, separator).trim(), labels)
  if (label === null) return null
  return line.toLowerCase().includes("blank")
    ? m.field_error_blank({ field: label })
    : m.field_error_invalid({ field: label })
}

/**
 * Поле по имени ограничения: `objects_name_kk_not_blank` -> «Наименование
 * (қазақша)». Имя таблицы отбрасывается по самому длинному совпадению -
 * иначе `objects_address_kk_not_blank` нашло бы `address` раньше, чем
 * `address_kk`.
 */
function constraintLabel(
  constraint: string,
  labels: FieldLabels
): string | null {
  const stem = constraint
    .replace(/_not_blank$/u, "")
    .replace(/_option_fkey$/u, "")
  let best: string | null = null
  let bestLength = 0
  for (const [field, label] of Object.entries(labels)) {
    if (
      (stem === field || stem.endsWith(`_${field}`)) &&
      field.length > bestLength
    ) {
      best = label()
      bestLength = field.length
    }
  }
  return best
}

/** Поле по имени из отчета garde; `_code` у опций Прил. 4 необязателен. */
function fieldLabel(field: string, labels: FieldLabels): string | null {
  const exact = labels[field]
  if (exact !== undefined) return exact()
  const trimmed = labels[field.replace(/_code$/u, "")]
  return trimmed === undefined ? null : trimmed()
}
