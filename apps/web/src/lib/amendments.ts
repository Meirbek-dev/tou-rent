import { queryOptions } from "@tanstack/react-query"

import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type AmendmentDto = components["schemas"]["AmendmentDto"]
export type AmendLotInput = components["schemas"]["CreateLotRequest"]

async function mutate<T>(promise: Promise<{ data?: T; error?: unknown }>) {
  const { data, error } = await promise
  if (error !== undefined || data === undefined) {
    throw error ?? new Error("request failed")
  }
  return data
}

/** Редакции тендерной документации (FR-304, п. 27) - баннер изменений. */
export const tenderAmendmentsQuery = (tenderId: string) =>
  queryOptions({
    queryKey: ["amendments", tenderId],
    queryFn: () =>
      mutate(
        api.GET("/api/v1/tenders/{id}/amendments", {
          params: { path: { id: tenderId } },
        })
      ),
  })

/** Публикация новой редакции: срок приема продлевается (п. 27). */
export const amendTender = (
  tenderId: string,
  summary: string,
  schedule: {
    newDeadline: string
    newOpeningAt: string | null
    newTradingAt: string | null
  },
  lots: AmendLotInput[]
) =>
  mutate(
    api.POST("/api/v1/tenders/{id}/amendments", {
      params: { path: { id: tenderId } },
      body: {
        summary,
        new_deadline: schedule.newDeadline,
        new_opening_at: schedule.newOpeningAt,
        new_trading_at: schedule.newTradingAt,
        lots,
      },
    })
  )

/** Отказ от участия из-за изменения условий (FR-1004, п. 26.5). */
export const declineAmendment = (applicationId: string) =>
  mutate(
    api.POST("/api/v1/applications/{id}/decline-amendment", {
      params: { path: { id: applicationId } },
    })
  )

/** Отмена тендера с основанием (FR-305, п. 78). */
export const cancelTender = (tenderId: string, reason: string) =>
  mutate(
    api.POST("/api/v1/tenders/{id}/cancel", {
      params: { path: { id: tenderId } },
      body: { reason },
    })
  )

/** Отмена отдельного лота (FR-305): тендер продолжается. */
export const cancelLot = (lotId: string, reason: string) =>
  mutate(
    api.POST("/api/v1/lots/{id}/cancel", {
      params: { path: { id: lotId } },
      body: { reason },
    })
  )
