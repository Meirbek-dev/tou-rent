import { queryOptions } from "@tanstack/react-query"

import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type ObligationDto = components["schemas"]["ObligationDto"]
export type HolidayDto = components["schemas"]["HolidayDto"]

/** «Мои сроки» (FR-1702): открытые обязательства ролей пользователя. */
export const myObligationsQuery = queryOptions({
  queryKey: ["obligations", "my"],
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/obligations/my")
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load obligations")
    }
    return data
  },
  // Сроки меряются днями - частый опрос не нужен
  staleTime: 60_000,
})

/** Производственный календарь (FR-1701): его ведет админ. */
export const holidaysQuery = queryOptions({
  queryKey: ["refdata", "holidays"],
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/refdata/holidays")
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load holidays")
    }
    return data
  },
})
