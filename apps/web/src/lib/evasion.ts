import { queryOptions } from "@tanstack/react-query"

import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type EvasionDto = components["schemas"]["EvasionDto"]
export type EvaderDto = components["schemas"]["EvaderDto"]
export type EvasionGroundDto = components["schemas"]["EvasionGroundDto"]

async function mutate<T>(promise: Promise<{ data?: T; error?: unknown }>) {
  const { data, error } = await promise
  if (error !== undefined || data === undefined) {
    throw error ?? new Error("request failed")
  }
  return data
}

/** Уклонения тендера (FR-903, п. 116–118). */
export const tenderEvasionsQuery = (tenderId: string) =>
  queryOptions({
    queryKey: ["evasions", "tender", tenderId],
    queryFn: () =>
      mutate(
        api.GET("/api/v1/tenders/{id}/evasions", {
          params: { path: { id: tenderId } },
        })
      ),
  })

/** Закрытый перечень оснований уклонения (п. 116). */
export const evasionGroundsQuery = queryOptions({
  queryKey: ["evasion-grounds"],
  queryFn: () => mutate(api.GET("/api/v1/evasion-grounds")),
})

/** Реестр уклонистов (FR-505, п. 52.4, 120). */
export const evaderRegistryQuery = queryOptions({
  queryKey: ["evaders"],
  queryFn: () => mutate(api.GET("/api/v1/evaders")),
})

/** Признание уклонения от подписания договора (п. 116). */
export const declareEvasion = (
  contractId: string,
  ground: string,
  note?: string
) =>
  mutate(
    api.POST("/api/v1/contracts/{id}/evasion", {
      params: { path: { id: contractId } },
      body: { ground, note: note ?? null },
    })
  )

/** Протокол о победителе № 2 и уведомление участника № 2 (п. 117–118). */
export const generateWinner2Protocol = (tenderId: string) =>
  mutate(
    api.POST("/api/v1/tenders/{id}/winner2-protocol", {
      params: { path: { id: tenderId } },
    })
  )
