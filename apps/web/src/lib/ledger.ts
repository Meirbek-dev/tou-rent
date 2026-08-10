import { queryOptions } from "@tanstack/react-query"

import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type LedgerAccountDto = components["schemas"]["LedgerAccountDto"]
export type LedgerEntryDto = components["schemas"]["LedgerEntryDto"]
export type RefundReasonDto = components["schemas"]["RefundReasonDto"]

/** Депозитная книга (FR-1001): счета взносов и депозитов с остатками. */
export const ledgerAccountsQuery = (kind?: string) =>
  queryOptions({
    queryKey: ["ledger", "accounts", kind ?? null],
    queryFn: async () => {
      const { data, error } = await api.GET("/api/v1/ledger/accounts", {
        params: { query: kind ? { kind } : {} },
      })
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load ledger accounts")
      }
      return data
    },
  })

/**
 * Выписка по счету: проводки двойной записи (INV-DB-05), первая страница.
 *
 * Маршрут отдает страницу `{ items, next_after, truncated }` (ТЗ § 7),
 * а сюда возвращаются только строки.
 * TODO-ENGINEER: экран финблока (`routes/app/finance/index.tsx`) признак
 * `truncated` пока не показывает и курсор `next_after` не листает -
 * запрос отдает `items`, чтобы экран не разошелся с контрактом. Как только
 * экран научится и тому и другому, отсюда должна возвращаться страница
 * целиком, как в `evaderRegistryQuery`.
 */
export const ledgerEntriesQuery = (accountId: string) =>
  queryOptions({
    queryKey: ["ledger", "entries", accountId],
    queryFn: async () => {
      const { data, error } = await api.GET(
        "/api/v1/ledger/accounts/{id}/entries",
        { params: { path: { id: accountId } } }
      )
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load ledger entries")
      }
      return data.items
    },
  })

/** Закрытый перечень оснований возврата (FR-1002, п. 26). */
export const refundReasonsQuery = queryOptions({
  queryKey: ["refdata", "refund-reasons"],
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/refdata/refund-reasons")
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load refund reasons")
    }
    return data
  },
  staleTime: Number.POSITIVE_INFINITY,
})

/** Подпись операции книги - предметный термин Правил. */
export const LEDGER_OPS = [
  "receipt_confirmed",
  "hold",
  "offset",
  "refund",
  "writeoff",
  "replenish",
] as const
