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

/**
 * Реестр публикаций особого порядка (FR-1403, п. 90, 92, 97) - портал.
 *
 * TODO-ENGINEER: маршрут отдает страницу `{ items, next_after, truncated }`
 * (ТЗ § 7), сюда возвращаются только строки - страница портала
 * (`routes/special-orders.tsx`) ни признак `truncated`, ни курсор
 * `next_after` пока не показывает.
 */
export const publicRecordsQuery = queryOptions({
  queryKey: ["public-records"],
  queryFn: async () => (await mutate(api.GET("/api/v1/public-records"))).items,
})

/**
 * Что ждет публикации (п. 97): рабочий список подразделения - страницей.
 *
 * Отдается страница целиком: `truncated` показывает панель публикаций,
 * и для рабочего списка это не формальность - поднятый признак значит,
 * что материалов накопилось больше, чем подразделение видит.
 */
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
