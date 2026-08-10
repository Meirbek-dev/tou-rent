import { queryOptions } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"

import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type User = components["schemas"]["UserDto"]
export type Problem = components["schemas"]["Problem"]

/** Текущая сессия; null - не аутентифицирован (401 - штатный ответ). */
export const meQuery = queryOptions({
  queryKey: ["auth", "me"],
  queryFn: async (): Promise<User | null> => {
    const { data, response } = await api.GET("/api/v1/auth/me")
    if (response.status === 401) return null
    if (data === undefined) throw new Error("failed to load session")
    return data
  },
  staleTime: 60_000,
})

/** Человекочитаемое сообщение из problem+json (RFC 9457, NFR-08). */
export function problemMessage(error: unknown): string {
  if (error && typeof error === "object") {
    const problem = error as Partial<Problem>
    if (typeof problem.detail === "string" && problem.detail !== "") {
      return problem.detail
    }
    if (typeof problem.title === "string" && problem.title !== "") {
      return problem.title
    }
  }
  return "unknown_error"
}

/** Кабинеты по ролям (ТЗ § 8, INV-POL-01). Ключи - snake_case ролей домена. */
export const CABINET_PATHS: Record<string, string> = {
  participant: "/app/participant",
  organizer: "/app/organizer",
  secretary: "/app/secretary",
  commission: "/app/commission",
  finance: "/app/finance",
  board: "/app/board",
  admin: "/app/admin",
}

/** Подпись кабинета роли: одна на весь интерфейс (ТЗ § 8). */
const CABINET_LABELS: Record<string, () => string> = {
  participant: m.cabinet_participant,
  organizer: m.cabinet_organizer,
  secretary: m.cabinet_secretary,
  commission: m.cabinet_commission,
  finance: m.cabinet_finance,
  board: m.cabinet_board,
  admin: m.cabinet_admin,
}

export function cabinetLabel(role: string): string {
  return CABINET_LABELS[role]?.() ?? role
}
