import { queryOptions } from "@tanstack/react-query"

import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type NotificationDto = components["schemas"]["NotificationDto"]

/** Данные приглашения на торги (FR-504): рендер локализует клиент. */
export type AuctionInvitationPayload = {
  tender_id: string
  tender_title: string
  lot_id: string
  lot: string
  application_id: string
  starting_bid: string
  trading_at: string
  place: string | null
}

/** Решение комиссии по заявке (FR-502, п. 52, 56): отказ с основанием. */
export type ApplicationRejectedPayload = {
  tender_id: string
  application_id: string
  reason_code: string | null
  reason: string | null
}

/** Просроченный срок Правил (FR-1702): эскалация исполнителю. */
export type ObligationOverduePayload = {
  obligation_id: string
  action: string
  rule_ref: string
  tender_id: string | null
  tender_title: string | null
  due_at: number
}

/** Предложение договора участнику № 2 (FR-903, п. 117–118). */
export type RunnerUpOfferPayload = {
  tender_title: string
  lot: string
  amount: string
  protocol_number: string
}

/** Новая редакция документации (FR-304, п. 27). */
export type TenderAmendedPayload = {
  tender_title: string
  version: number
  summary: string | null
  new_deadline: string | null
}

/** Отмена тендера или лота (FR-305, п. 78–79). */
export type TenderCancelledPayload = {
  tender_title: string
  scope: string
  reason: string
}

/** Опубликованный протокол (FR-702, FR-703, п. 56, 75). */
export type ProtocolPublishedPayload = {
  tender_title: string
  protocol_kind: string
  protocol_number: string
}

/** Данные решения по заявке особого порядка (FR-1202, п. 90). */
export type SpecialDecidedPayload = {
  special_request_id: string
  category: string
  decision: string
  rationale: string
}

/** Счетчик непрочитанных для колокольчика (FR-1301). */
export const unreadCountQuery = queryOptions({
  queryKey: ["notifications", "unread-count"],
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/notifications/unread-count")
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load unread count")
    }
    return data.count
  },
})

/** История уведомлений, новые сверху (страница 50). */
export const notificationsQuery = queryOptions({
  queryKey: ["notifications", "list"],
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/notifications")
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load notifications")
    }
    return data
  },
})

/** Отметить прочитанными; без ids - все. */
export async function markRead(ids?: string[]) {
  const { data, error } = await api.POST("/api/v1/notifications/read", {
    body: { ids: ids ?? null },
  })
  if (error !== undefined || data === undefined) {
    throw (error as unknown) ?? new Error("failed to mark notifications read")
  }
  return data.updated
}
