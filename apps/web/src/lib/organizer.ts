import { queryOptions } from "@tanstack/react-query"

import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type ObjectDto = components["schemas"]["ObjectDto"]
export type ObjectKind = components["schemas"]["ObjectKindDto"]
export type ObjectRequest = components["schemas"]["ObjectRequest"]
export type RateOptions = components["schemas"]["RateOptionsDto"]
export type RateOptionsCatalog = components["schemas"]["RateOptionsCatalog"]
export type RateCalculation = components["schemas"]["RateCalculationDto"]

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
