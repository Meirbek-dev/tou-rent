import { queryOptions } from "@tanstack/react-query"

import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type AuctionDto = components["schemas"]["AuctionDto"]
export type BidDto = components["schemas"]["BidDto"]
export type CircleParticipantDto = components["schemas"]["CircleParticipantDto"]
export type AuctionRoomDto = components["schemas"]["AuctionRoomDto"]

export const auctionRoomKey = (id: string) => ["auction", id] as const

/** Снимок комнаты торгов (FR-601–603): состояние, лента, право торговаться. */
export const auctionRoomQuery = (id: string) =>
  queryOptions({
    queryKey: auctionRoomKey(id),
    queryFn: async () => {
      const { data, error } = await api.GET("/api/v1/auctions/{id}", {
        params: { path: { id } },
      })
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load auction room")
      }
      return data
    },
    // Ленту двигает WS; периодический опрос нужен только как страховка
    // от разорванного сокета
    refetchInterval: 30_000,
  })

/** Комната лота, если она открыта; null - торги по лоту еще не назначены. */
export const lotAuctionQuery = (lotId: string) =>
  queryOptions({
    queryKey: ["auction", "lot", lotId],
    queryFn: async () => {
      const { data, response, error } = await api.GET(
        "/api/v1/lots/{id}/auction",
        { params: { path: { id: lotId } } }
      )
      if (response.status === 404) return null
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load lot auction")
      }
      return data
    },
  })

async function mutate<T>(
  call: Promise<{ data?: T; error?: unknown }>
): Promise<T> {
  const { data, error } = await call
  if (error !== undefined || data === undefined) {
    throw (error as unknown) ?? new Error("auction request failed")
  }
  return data
}

/** Открытие комнаты лота секретарем (INV-062: старт = max предложений). */
export const scheduleAuction = (lotId: string) =>
  mutate(
    api.POST("/api/v1/lots/{id}/auction", { params: { path: { id: lotId } } })
  )

/** Объявление старта председателем - сервер включает таймер (FR-602). */
export const startAuction = (id: string) =>
  mutate(api.POST("/api/v1/auctions/{id}/start", { params: { path: { id } } }))

/** Продление ровно на 15 минут и только один раз (INV-066). */
export const extendAuction = (id: string) =>
  mutate(api.POST("/api/v1/auctions/{id}/extend", { params: { path: { id } } }))

/** Завершение торгов; early - досрочно при общем согласии (п. 67). */
export const finishAuction = (id: string, early: boolean) =>
  mutate(
    api.POST("/api/v1/auctions/{id}/finish", {
      params: { path: { id } },
      body: { early },
    })
  )

/**
 * Ставка участника (FR-601). `bidId` генерирует клиент и повторяет при
 * ретрае: сервер вернет ту же ставку вместо дубля (NFR-05).
 */
export const placeBid = (id: string, amount: string, bidId: string) =>
  mutate(
    api.POST("/api/v1/auctions/{id}/bids", {
      params: { path: { id } },
      body: { id: bidId, amount },
    })
  )

/** Пас участника (FR-604, п. 65): не готов повысить - выбывает из торгов. */
export const passTurn = (id: string) =>
  mutate(api.POST("/api/v1/auctions/{id}/pass", { params: { path: { id } } }))

/** Отметка неявки допущенного (FR-605, п. 70): его предложение оглашается. */
export const markAbsent = (id: string, applicationId: string) =>
  mutate(
    api.POST("/api/v1/auctions/{id}/participants/{application_id}/absent", {
      params: { path: { id, application_id: applicationId } },
    })
  )

/** Факт формирования протокола итогов (FR-701): номер и время. */
export type GeneratedProtocolDto = components["schemas"]["GeneratedProtocolDto"]

/** Протокол итогов тендера (FR-701); null - еще не сформирован. */
export const resultsProtocolQuery = (tenderId: string) =>
  queryOptions({
    queryKey: ["results-protocol", tenderId],
    queryFn: async () => {
      const { data, response, error } = await api.GET(
        "/api/v1/tenders/{id}/results-protocol",
        { params: { path: { id: tenderId } } }
      )
      if (response.status === 404) return null
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load results protocol")
      }
      return data
    },
  })

/** Формирование протокола итогов: возможно после завершения торгов по всем лотам. */
export const generateResultsProtocol = (tenderId: string) =>
  mutate(
    api.POST("/api/v1/tenders/{id}/results-protocol", {
      params: { path: { id: tenderId } },
    })
  )

/**
 * Ссылка на запись торгов (FR-306, п. 72): вносит секретарь после того, как
 * итоги подведены. Пустая строка снимает ошибочно внесенную ссылку.
 */
export const setRecordingUrl = (tenderId: string, recordingUrl: string) =>
  mutate(
    api.PUT("/api/v1/tenders/{id}/recording", {
      params: { path: { id: tenderId } },
      body: { recording_url: recordingUrl === "" ? null : recordingUrl },
    })
  )
