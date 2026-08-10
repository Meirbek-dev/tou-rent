import { queryOptions } from "@tanstack/react-query"

import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type FailureStateDto = components["schemas"]["FailureStateDto"]

/** Состояние тендера по п. 81–83: наступило ли основание и что из него следует. */
export const failureStateQuery = (tenderId: string) =>
  queryOptions({
    queryKey: ["failure", tenderId],
    queryFn: async () => {
      const { data, error } = await api.GET("/api/v1/tenders/{id}/failure", {
        params: { path: { id: tenderId } },
      })
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load failure state")
      }
      return data
    },
  })

async function mutate<T>(promise: Promise<{ data?: T; error?: unknown }>) {
  const { data, error } = await promise
  if (error !== undefined || data === undefined) {
    throw error ?? new Error("request failed")
  }
  return data
}

/** Признание несостоявшимся (FR-801): основание выводит сервер. */
export const declareFailed = (tenderId: string) =>
  mutate(
    api.POST("/api/v1/tenders/{id}/declare-failed", {
      params: { path: { id: tenderId } },
    })
  )

/** Протокол о несостоявшемся (FR-802, п. 82). */
export const generateFailedProtocol = (tenderId: string) =>
  mutate(
    api.POST("/api/v1/tenders/{id}/failed-protocol", {
      params: { path: { id: tenderId } },
    })
  )

/** Повторный тендер (п. 82): черновик с теми же лотами. */
export const repeatTender = (tenderId: string) =>
  mutate(
    api.POST("/api/v1/tenders/{id}/repeat", {
      params: { path: { id: tenderId } },
    })
  )
