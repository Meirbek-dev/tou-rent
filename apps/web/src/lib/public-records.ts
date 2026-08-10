import { queryOptions } from "@tanstack/react-query"

import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type PublicRecord = components["schemas"]["PublicRecordDto"]
export type PendingPublication = components["schemas"]["PendingPublicationDto"]

async function mutate<T>(promise: Promise<{ data?: T; error?: unknown }>) {
  const { data, error } = await promise
  if (error !== undefined || data === undefined) {
    throw error ?? new Error("request failed")
  }
  return data
}

/** Реестр публикаций особого порядка (FR-1403, п. 90, 92, 97) - портал. */
export const publicRecordsQuery = queryOptions({
  queryKey: ["public-records"],
  queryFn: () => mutate(api.GET("/api/v1/public-records")),
})

/** Что ждет публикации (п. 97): рабочий список подразделения. */
export const pendingPublicationsQuery = queryOptions({
  queryKey: ["public-records", "pending"],
  queryFn: () => mutate(api.GET("/api/v1/public-records/pending")),
})

/** Публикация материала на портале (FR-1403, п. 97). */
export const publishRecord = (kind: string, sourceId: string) =>
  mutate(
    api.POST("/api/v1/public-records", {
      body: { kind, source_id: sourceId },
    })
  )
