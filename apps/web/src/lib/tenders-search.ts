import type { TenderStatus } from "@/lib/api"

// Статусы, осмысленные в публичном реестре (draft гость не видит - API его не отдает)
export const PUBLIC_STATUSES: TenderStatus[] = [
  "announced",
  "accepting",
  "qualification",
  "trading",
  "summed_up",
  "contracted",
  "failed",
  "repeat_announced",
  "cancelled",
]

export type TendersSearch = {
  status?: TenderStatus
  q?: string
  after?: string
}

/**
 * FR-1401: фильтры реестра живут в URL. Нормализация «сырых» query-параметров:
 * неизвестный статус и пустые значения отбрасываются, а не ломают страницу.
 */
export function validateTendersSearch(
  search: Record<string, unknown>
): TendersSearch {
  return {
    status: PUBLIC_STATUSES.includes(search["status"] as TenderStatus)
      ? (search["status"] as TenderStatus)
      : undefined,
    q:
      typeof search["q"] === "string" && search["q"].trim() !== ""
        ? search["q"]
        : undefined,
    after:
      typeof search["after"] === "string" && search["after"] !== ""
        ? search["after"]
        : undefined,
  }
}
