import { queryOptions } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"

import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type LandPlot = components["schemas"]["LandPlotDto"]
export type LandApplication = components["schemas"]["LandApplicationDto"]
export type LandRefdata = components["schemas"]["LandRefdataResponse"]

async function mutate<T>(promise: Promise<{ data?: T; error?: unknown }>) {
  const { data, error } = await promise
  if (error !== undefined || data === undefined) {
    throw error ?? new Error("request failed")
  }
  return data
}

/** Участки портала (FR-1801, п. 104); организатор видит и неопубликованные. */
export const landPlotsQuery = queryOptions({
  queryKey: ["land-plots"],
  queryFn: () => mutate(api.GET("/api/v1/land-plots")),
})

/** Справочники раздела 14: назначения (п. 104) и особые условия (п. 107). */
export const landRefdataQuery = queryOptions({
  queryKey: ["refdata", "land"],
  queryFn: () => mutate(api.GET("/api/v1/refdata/land")),
})

/** Заявки инвестора (п. 105). */
export const myLandApplicationsQuery = queryOptions({
  queryKey: ["land-applications", "my"],
  queryFn: () => mutate(api.GET("/api/v1/land-applications/my")),
})

/** Рабочий список заявок Правления и организатора (п. 105–107). */
export const landApplicationsQuery = queryOptions({
  queryKey: ["land-applications"],
  queryFn: () => mutate(api.GET("/api/v1/land-applications")),
})

/** Публикация характеристик участка (п. 104). */
export const publishLandPlot = (plotId: string) =>
  mutate(
    api.POST("/api/v1/land-plots/{id}/publish", {
      params: { path: { id: plotId } },
    })
  )

/** Решение Правления по заявке (п. 106). */
export const decideLandApplication = (
  applicationId: string,
  decision: string,
  rationale: string
) =>
  mutate(
    api.POST("/api/v1/land-applications/{id}/decision", {
      params: { path: { id: applicationId } },
      body: { decision, rationale },
    })
  )

/** Состояние заявки на участок (п. 105–106) - подпись для кабинета. */
export function landStatusLabel(status: string): string {
  switch (status) {
    case "submitted":
      return m.land_status_submitted()
    case "granted":
      return m.land_status_granted()
    case "refused":
      return m.land_status_refused()
    case "withdrawn":
      return m.land_status_withdrawn()
    default:
      return status
  }
}

/** Решение Правления по заявке (п. 106). */
export function landDecisionLabel(decision: string): string {
  return decision === "grant"
    ? m.land_decision_grant()
    : m.land_decision_refuse()
}
