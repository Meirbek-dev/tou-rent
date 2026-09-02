import { queryOptions } from "@tanstack/react-query"

import { api } from "@/lib/api"
import { formatDateTime } from "@/lib/format"
import { serverLabel } from "@/lib/server-label"

import type { components } from "@tou/api-client"

export type Registry = components["schemas"]["RegistryDto"]
export type RegistrySummary = components["schemas"]["RegistrySummaryDto"]
export type RegistryCell = components["schemas"]["RegistryCellDto"]

/** Реестр отчетности: перечень закрыт контрактом (арх. § 9). */
export type ReportRegistry = components["schemas"]["ReportRegistryDto"]

/**
 * Текст ячейки реестра в текущей локали.
 *
 * Момент времени печатает кабинет, а не сервер (NFR-03): с провода приходит
 * ISO 8601, зону отображения и ее подпись добавляет `formatDateTime` - тот
 * же, что и на остальных экранах. Значения закрытых перечней приходят тремя
 * колонками, как прочие серверные справочники (`serverLabel`), остальное -
 * одинаково во всех локалях и лежит в `text_ru`.
 */
export function registryCellText(cell: RegistryCell): string {
  const moments = cell.timestamps
    .map((iso) => formatDateTime(iso))
    .filter((text): text is string => text !== null)
  if (moments.length > 0) return moments.join(" - ")
  return serverLabel(cell, "text")
}

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

/**
 * Реестр за период: колонки и строки приходят с сервера.
 *
 * Пустой реестр - это «еще не выбран» (перечень доступных приходит отдельным
 * запросом), и запрос в этом случае выключен вызывающим. Проверка все равно
 * стоит здесь: без нее `registry` пришлось бы приводить к перечню контракта
 * приведением типа, то есть обещанием, которого код не проверяет.
 */
export const registryQuery = (
  registry: ReportRegistry | "",
  from: string,
  to: string
) =>
  queryOptions({
    queryKey: ["reports", registry, from, to],
    queryFn: () => {
      if (registry === "") throw new Error("registry is not selected")
      return mutate(
        api.GET("/api/v1/reports/{registry}", {
          params: {
            path: { registry },
            // Пустая граница периода не отправляется вовсе, а не отправляется
            // со значением undefined: в контракте `from`/`to` необязательны,
            // и «ключа нет» - это именно «границы нет»
            query: { ...(from ? { from } : {}), ...(to ? { to } : {}) },
          },
        })
      )
    },
  })

/** Ссылка на выгрузку реестра (CSV) с тем же периодом. */
export function registryCsvHref(
  registry: ReportRegistry | "",
  from: string,
  to: string
): string {
  const params = new URLSearchParams()
  if (from) params.set("from", from)
  if (to) params.set("to", to)
  const query = params.toString()
  return `/api/v1/reports/${registry}/export.csv${query ? `?${query}` : ""}`
}
