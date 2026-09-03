import { infiniteQueryOptions, queryOptions } from "@tanstack/react-query"

import { api } from "#/lib/api"
import { serverLabel } from "#/lib/server-label"

import type { components } from "@tou/api-client"

export type ObjectDto = components["schemas"]["ObjectDto"]
export type ObjectKind = components["schemas"]["ObjectKindDto"]
export type ObjectRequest = components["schemas"]["ObjectRequest"]
export type RateOptions = components["schemas"]["RateOptionsDto"]
export type RateOptionsCatalog = components["schemas"]["RateOptionsCatalog"]
export type RateCalculation = components["schemas"]["RateCalculationDto"]
export type RateOption = components["schemas"]["RateOptionDto"]

/**
 * Подпись опции коэффициента Прил. 4 (FR-202).
 *
 * Справочник ведет названия в трех колонках, и до этого фронт их не читал:
 * в селектах калькулятора стояло «default (1.0000)» - машинный код вместо
 * условия, по которому опция и выбирается. Код остается запасным: строка
 * справочника без названия все же лучше пустого пункта списка.
 */
export function rateOptionLabel(option: RateOption): string {
  return serverLabel(option) || option.option_code
}

/**
 * Подпись выбранной опции по каталогу: снимок расчета несет только код
 * (`FactorDto.option_code`) - он заморожен в лоте (FR-202) и названия
 * справочника в себе не держит.
 */
export function rateOptionLabelOf(
  options: readonly RateOption[],
  coefficient: string,
  optionCode: string
): string {
  const known = options.find(
    (option) =>
      option.coefficient === coefficient && option.option_code === optionCode
  )
  return known === undefined ? optionCode : rateOptionLabel(known)
}

/** Порядок множителей формулы FR-201 (обозначения Прил. 4 - предметные, вне i18n). */
export const COEFFICIENTS = [
  ["kt", "Кт"],
  ["kk", "Кк"],
  ["ksk", "Кск"],
  ["kr", "Кр"],
  ["kvd", "Квд"],
  ["kopf", "Копф"],
  ["kfu", "Кфу"],
  ["ksots", "Ксоц"],
  ["k", "К"],
  ["kn", "Кн"],
  ["kv", "Кв"],
] as const satisfies readonly (readonly [keyof RateOptions, string])[]

export function defaultRateOptions(): RateOptions {
  return {
    kt: "default",
    kk: "default",
    ksk: "default",
    kr: "default",
    kvd: "default",
    kopf: "default",
    kfu: "default",
    ksots: "default",
    k: "default",
    kn: "default",
    kv: "default",
  }
}

export const objectsQuery = queryOptions({
  queryKey: ["objects"],
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/objects", {
      params: { query: { limit: 100 } },
    })
    if (error !== undefined || data === undefined) {
      throw error ?? new Error("failed to load objects")
    }
    return data
  },
})

/** Справочник калькулятора: МРП и действующие опции Прил. 4 (FR-202). */
export const rateOptionsQuery = queryOptions({
  queryKey: ["rate-options"],
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/rates/options")
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load rate options")
    }
    return data
  },
})

/**
 * Правка и удаление объекта (FR-101).
 *
 * `PUT` отвечает телом, `DELETE` - 204 без тела, поэтому успех проверяется
 * по отсутствию `error`, а не по наличию `data` (та же ошибка стоила
 * отмене тендера ложного «не удалось», гаунтлет G-098).
 */
export async function updateObject(
  id: string,
  body: ObjectRequest
): Promise<ObjectDto> {
  const { data, error } = await api.PUT("/api/v1/objects/{id}", {
    params: { path: { id } },
    body,
  })
  if (error !== undefined) throw error
  return data
}

export async function deleteObject(id: string): Promise<void> {
  const { error } = await api.DELETE("/api/v1/objects/{id}", {
    params: { path: { id } },
  })
  if (error !== undefined) throw error
}

/**
 * Поля объекта, хранящие коды опций Прил. 4, и коэффициенты, к которым эти
 * коды относятся (`core.objects.*_coefficient` - генерируемые колонки, FK
 * на `refdata.rate_options`).
 *
 * Таблица перечисляет поля поименно: справочник опций общий с калькулятором,
 * и без нее селекты формы объекта пришлось бы связывать с коэффициентами
 * строками по месту.
 */
export const OBJECT_OPTION_FIELDS = [
  ["premises_type_code", "kt"],
  ["premises_kind_code", "ksk"],
  ["comfort_code", "kk"],
  ["location_code", "kr"],
] as const satisfies readonly (readonly [keyof ObjectRequest, string])[]

export type ObjectOptionField = (typeof OBJECT_OPTION_FIELDS)[number][0]

/** Реестр глазами организатора (видит черновики) - ключ отдельный от публичного. */
export const organizerTendersQuery = queryOptions({
  queryKey: ["tenders", "organizer"],
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/tenders", {
      params: { query: { limit: 100 } },
    })
    if (error !== undefined || data === undefined) {
      throw error ?? new Error("failed to load tenders")
    }
    return data
  },
})

/** Размер страницы реестра организатора: столько строк читается за раз. */
export const TENDERS_PAGE_SIZE = 20

/**
 * Реестр организатора страницами (FR-301).
 *
 * Список отдавался одним запросом на сто записей, и `next_after` контракта
 * никто не читал: на пятидесяти семи тендерах экран был сплошной лентой без
 * конца. Ключ - продолжение `organizerTendersQuery`, поэтому инвалидация
 * реестра после публикации или отмены задевает и его.
 */
export const organizerTendersPagesQuery = infiniteQueryOptions({
  queryKey: ["tenders", "organizer", "pages"],
  queryFn: async ({ pageParam }) => {
    const { data, error } = await api.GET("/api/v1/tenders", {
      params: {
        query: {
          limit: TENDERS_PAGE_SIZE,
          ...(pageParam === null ? {} : { after: pageParam }),
        },
      },
    })
    if (error !== undefined || data === undefined) {
      throw error ?? new Error("failed to load tenders")
    }
    return data
  },
  initialPageParam: null as string | null,
  getNextPageParam: (last) => last.next_after ?? null,
})

/**
 * Отбор строки реестра по фильтру статуса и подстроке названия.
 *
 * Названия ведутся в двух колонках (ru/kk), и искать нужно по обеим: реестр
 * организатора один на обе локали, а тендер заведен под тем названием, под
 * которым его помнят.
 */
export function matchesTenderFilter(
  tender: {
    status: string
    title: string
    title_kk: string
  },
  filter: { status: string; query: string }
): boolean {
  if (filter.status !== "" && tender.status !== filter.status) return false
  const needle = filter.query.trim().toLocaleLowerCase()
  if (needle === "") return true
  return (
    tender.title.toLocaleLowerCase().includes(needle) ||
    tender.title_kk.toLocaleLowerCase().includes(needle)
  )
}

/**
 * Статусы, после которых секретарю на тендере делать нечего.
 *
 * Договор заключен, тендер отменен или признан несостоявшимся - работа
 * закрыта. В общем списке такие тендеры занимали верх (даты у них самые
 * старые) и отодвигали живое заседание на тридцать восьмую строку.
 */
export const TERMINAL_TENDER_STATUSES = [
  "contracted",
  "cancelled",
  "failed",
] as const

/** Дальняя дата: тендер без сроков уходит вниз, а не выдает себя за срочный. */
const FAR_FUTURE = "9999"

/**
 * Ближайшая из двух дат тендера: заседание по вскрытию и окончание приема
 * заявок. Именно они и есть работа секретаря, а порядок выдачи реестра
 * (по созданию) к ней отношения не имеет. Сравниваются строки ISO, а не
 * `Date.now()`: порядок обязан совпасть на сервере и в браузере.
 */
export function nearestTenderDate(tender: {
  opening_at?: string | null | undefined
  submission_deadline?: string | null | undefined
}): string {
  const dates = [tender.opening_at, tender.submission_deadline].filter(
    (value): value is string => value != null
  )
  return dates.length === 0 ? FAR_FUTURE : (dates.toSorted()[0] ?? FAR_FUTURE)
}

/**
 * Рабочий список секретаря: сперва то, что требует действия.
 *
 * Порядок по возрастанию ближайшей даты сам ставит просроченное наверх -
 * прошедшая дата меньше будущей. Завершенные уезжают в отдельный список:
 * они не работа, а история, и сворачиваются на экране.
 */
export function splitSecretaryWorkList<
  T extends {
    status: string
    opening_at?: string | null | undefined
    submission_deadline?: string | null | undefined
  },
>(items: readonly T[]): { active: T[]; completed: T[] } {
  const byNearest = (left: T, right: T) =>
    nearestTenderDate(left).localeCompare(nearestTenderDate(right))
  const relevant = items.filter((tender) => tender.status !== "draft")
  const terminal = (tender: T) =>
    (TERMINAL_TENDER_STATUSES as readonly string[]).includes(tender.status)

  return {
    active: relevant.filter((tender) => !terminal(tender)).toSorted(byNearest),
    completed: relevant.filter(terminal).toSorted(byNearest),
  }
}

// NFR-03: ввод и отображение дат кабинета - Asia/Almaty (фиксированный UTC+5)

export function toAlmatyInput(iso: string | null | undefined): string {
  if (!iso) return ""
  const utc = new Date(iso)
  const shifted = new Date(utc.getTime() + 5 * 3_600_000)
  return shifted.toISOString().slice(0, 16)
}

export function fromAlmatyInput(value: string): string | null {
  return value === "" ? null : `${value}:00+05:00`
}
