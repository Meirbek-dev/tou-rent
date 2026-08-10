import { queryOptions } from "@tanstack/react-query"

import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type UserDto = components["schemas"]["UserDto"]
export type MrpDto = components["schemas"]["MrpDto"]
export type CoefficientVersionDto =
  components["schemas"]["CoefficientVersionDto"]

/** Роли, назначаемые админом (FR-1503): `guest` - аноним, он не хранится. */
export const GRANTABLE_ROLES = [
  "participant",
  "organizer",
  "secretary",
  "commission",
  "board",
  "finance",
  "admin",
] as const

export type GrantableRole = (typeof GRANTABLE_ROLES)[number]

async function mutate<T>(
  call: Promise<{ data?: T; error?: unknown; response: Response }>
): Promise<T | null> {
  const { data, error, response } = await call
  if (error !== undefined) throw error as unknown
  // 204 без тела - обычный ответ на назначение и отзыв роли
  if (response.status === 204) return null
  if (data === undefined) throw new Error("admin request failed")
  return data
}

/** Пользователи и их роли (FR-1902). */
export const usersQuery = queryOptions({
  queryKey: ["admin", "users"],
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/admin/users")
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load users")
    }
    return data
  },
})

export const grantRole = (userId: string, role: GrantableRole) =>
  mutate(
    api.POST("/api/v1/admin/users/{user_id}/roles", {
      params: { path: { user_id: userId } },
      body: { role },
    })
  )

export const revokeRole = (userId: string, role: string) =>
  mutate(
    api.DELETE("/api/v1/admin/users/{user_id}/roles/{role}", {
      params: { path: { user_id: userId, role } },
    })
  )

/** МРП по годам (FR-1901): база расчета ставки Прил. 4. */
export const mrpQuery = queryOptions({
  queryKey: ["refdata", "mrp"],
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/refdata/mrp")
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load mrp")
    }
    return data
  },
})

export const setMrp = (year: number, amount: string) =>
  mutate(
    api.PUT("/api/v1/refdata/mrp/{year}", {
      params: { path: { year } },
      body: { amount },
    })
  )

/**
 * Версии коэффициентов Прил. 4 (FR-202): справочник версионируется, поэтому
 * список - история, а не текущее состояние.
 */
export const coefficientsQuery = queryOptions({
  queryKey: ["refdata", "coefficients"],
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/refdata/coefficients")
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load coefficients")
    }
    return data
  },
})

export const addCoefficientVersion = (body: {
  coefficient: string
  option_code: string
  label_ru: string
  label_kk: string | null
  label_en: string | null
  value: string
  effective_from: string
}) => mutate(api.POST("/api/v1/refdata/coefficients", { body }))

/** Праздник производственного календаря (FR-1701). */
export const addHoliday = (day: string, label: string) =>
  mutate(
    api.POST("/api/v1/refdata/holidays", { body: { day, label_ru: label } })
  )

export const removeHoliday = (day: string) =>
  mutate(
    api.DELETE("/api/v1/refdata/holidays/{day}", {
      params: { path: { day } },
    })
  )
