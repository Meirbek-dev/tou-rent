import { queryOptions } from "@tanstack/react-query"

// `#/` вместо `@/`: оба псевдонима ведут в `src/`, но `#/` объявлен полем
// `imports` в package.json и потому разрешается не только сборщиком, а и
// раннером тестов - без этого `amendments.test.ts` не поднимается.
import { api } from "#/lib/api"

import type { components } from "@tou/api-client"

export type AmendmentDto = components["schemas"]["AmendmentDto"]
export type AmendLotInput = components["schemas"]["CreateLotRequest"]

/**
 * Успех запроса - это отсутствие `error`, а не наличие тела.
 *
 * Условие стояло `data === undefined` тоже, и отмена тендера и лота
 * (`POST /tenders/{id}/cancel`, `/lots/{id}/cancel` - 200 без тела, как и
 * отказ от измененных условий) читалась как отказ: пользователь получал
 * «Не удалось выполнить операцию», карточка не обновлялась, а в базе
 * отмена уже прошла. Тело есть у GET и у создания редакции - им пустой
 * ответ и так не приходит, поэтому проверять его отдельно незачем.
 */
export async function unwrap<T>(
  promise: Promise<{ data?: T; error?: unknown }>
): Promise<T> {
  const { data, error } = await promise
  if (error !== undefined) throw error
  return data as T
}

/** Редакции тендерной документации (FR-304, п. 27) - баннер изменений. */
export const tenderAmendmentsQuery = (tenderId: string) =>
  queryOptions({
    queryKey: ["amendments", tenderId],
    queryFn: () =>
      unwrap(
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
  unwrap(
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
  unwrap(
    api.POST("/api/v1/applications/{id}/decline-amendment", {
      params: { path: { id: applicationId } },
    })
  )

/** Отмена тендера с основанием (FR-305, п. 78). */
export const cancelTender = (tenderId: string, reason: string) =>
  unwrap(
    api.POST("/api/v1/tenders/{id}/cancel", {
      params: { path: { id: tenderId } },
      body: { reason },
    })
  )

/** Отмена отдельного лота (FR-305): тендер продолжается. */
export const cancelLot = (lotId: string, reason: string) =>
  unwrap(
    api.POST("/api/v1/lots/{id}/cancel", {
      params: { path: { id: lotId } },
      body: { reason },
    })
  )
