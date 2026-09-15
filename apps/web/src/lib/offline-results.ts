import { queryOptions } from "@tanstack/react-query"
import type { components } from "@tou/api-client"
import { api } from "@/lib/api"

export type OfflineState = components["schemas"]["OfflineResultsState"]
export type OfflineDecision = components["schemas"]["OfflineLotDecision"]
export type OfflineResolution = components["schemas"]["OfflineResolution"]

async function result<T>(promise: Promise<{ data?: T; error?: unknown }>) {
  const { data, error } = await promise
  if (error !== undefined || data === undefined)
    throw error ?? new Error("Request failed")
  return data
}

export const offlineResultsQuery = (id: string) =>
  queryOptions({
    queryKey: ["offline-results", id],
    queryFn: () =>
      result(
        api.GET("/api/v1/tenders/{id}/offline-results", {
          params: { path: { id } },
        })
      ),
  })

export const recordOfflineResults = (
  id: string,
  protocolId: string,
  lots: OfflineDecision[]
) =>
  result(
    api.POST("/api/v1/tenders/{id}/offline-results", {
      params: { path: { id } },
      body: { protocol_id: protocolId, confirmed_signed_protocol: true, lots },
    })
  )
