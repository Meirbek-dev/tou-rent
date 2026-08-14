import { queryOptions } from "@tanstack/react-query"

import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type Registry = components["schemas"]["RegistryDto"]
export type RegistrySummary = components["schemas"]["RegistrySummaryDto"]

async function mutate<T>(promise: Promise<{ data?: T; error?: unknown }>) {
  const { data, error } = await promise
  if (error !== undefined || data === undefined) {
    throw error ?? new Error("request failed")
  }
  return data
}

/** Реестры, доступные ролям пользователя (арх. § 9). */
export const registriesQuery = queryOptions({
  queryKey: ["reports"],
  queryFn: () => mutate(api.GET("/api/v1/reports")),
})

/** Реестр за период: колонки и строки приходят с сервера. */
export const registryQuery = (registry: string, from: string, to: string) =>
  queryOptions({
    queryKey: ["reports", registry, from, to],
    queryFn: () =>
      mutate(
        api.GET("/api/v1/reports/{registry}", {
          params: {
            path: { registry },
            // Пустая граница периода не отправляется вовсе, а не отправляется
            // со значением undefined: в контракте `from`/`to` необязательны,
            // и «ключа нет» - это именно «границы нет»
            query: { ...(from ? { from } : {}), ...(to ? { to } : {}) },
          },
        })
      ),
  })

/** Ссылка на выгрузку реестра (CSV) с тем же периодом. */
export function registryCsvHref(
  registry: string,
  from: string,
  to: string
): string {
  const params = new URLSearchParams()
  if (from) params.set("from", from)
  if (to) params.set("to", to)
  const query = params.toString()
  return `/api/v1/reports/${registry}/export.csv${query ? `?${query}` : ""}`
}
