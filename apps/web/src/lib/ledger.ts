import { infiniteQueryOptions, queryOptions } from "@tanstack/react-query"

import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type LedgerAccountDto = components["schemas"]["LedgerAccountDto"]
export type LedgerEntryDto = components["schemas"]["LedgerEntryDto"]
export type RefundReasonDto = components["schemas"]["RefundReasonDto"]
export type FinancePendingFeeDto = components["schemas"]["FinancePendingFeeDto"]

/** Размер страницы очереди взносов: столько заявок читается за раз. */
export const PENDING_FEES_PAGE_SIZE = 20

/**
 * Заявки, ждущие подтверждения взноса (FR-405).
 *
 * Единственным входом в подтверждение была форма с полем «идентификатор
 * заявки»: оператор должен был откуда-то взять UUID, которого кабинет ему
 * не показывал. Очередь и есть этот список.
 *
 * Ключ лежит под `["ledger", …]` намеренно: подтверждение и возврат уже
 * сбрасывают всю эту ветку, и очередь обновляется тем же вызовом, что и
 * книга - строка уходит из нее сразу после проводки.
 */
export const pendingFeesQuery = infiniteQueryOptions({
  queryKey: ["ledger", "pending-fees"],
  queryFn: async ({ pageParam }) => {
    const { data, error } = await api.GET("/api/v1/finance/pending-fees", {
      params: {
        query: {
          limit: PENDING_FEES_PAGE_SIZE,
          ...(pageParam === null ? {} : { after: pageParam }),
        },
      },
    })
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load pending fees")
    }
    return data
  },
  initialPageParam: null as string | null,
  getNextPageParam: (last) => last.next_after ?? null,
})

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
 * Отдается страница целиком, как в `evaderRegistryQuery`: обрезанная
 * выписка - это недосчитанные деньги, и признак `truncated` экран финблока
 * показывает явно.
 *
 * TODO-ENGINEER: курсор `next_after` экран пока не листает - за потолком
 * выборки выписка обрывается, о чем и предупреждает признак.
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
      return data
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
