import { queryOptions } from "@tanstack/react-query"

import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type UserDto = components["schemas"]["UserDto"]
export type MrpDto = components["schemas"]["MrpDto"]
export type CoefficientVersionDto =
  components["schemas"]["CoefficientVersionDto"]
export type SiteAnnouncementDto = components["schemas"]["SiteAnnouncementDto"]

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

/**
 * Сброс пароля админом (W-07).
 *
 * Канала доставки в контуре 1 нет (почта - T41), поэтому пароль не уходит
 * письмом: сервер генерирует его сам и возвращает ровно один раз - в ответе
 * на это нажатие. Нигде больше он не существует: ни в логе, ни в БД, ни в
 * аудите (там только отпечаток). Перезапрос страницы его не вернет - будет
 * новый сброс.
 */
export const resetPassword = async (userId: string): Promise<string> => {
  const { data, error } = await api.POST(
    "/api/v1/admin/users/{user_id}/password-reset",
    { params: { path: { user_id: userId } } }
  )
  if (error !== undefined || data === undefined) {
    throw (error as unknown) ?? new Error("failed to reset password")
  }
  return data.password
}

/** Отключение и возврат учетной записи (W-07): удаления нет и не будет. */
export const setUserActive = (userId: string, isActive: boolean) =>
  mutate(
    api.PUT("/api/v1/admin/users/{user_id}/active", {
      params: { path: { user_id: userId } },
      body: { is_active: isActive },
    })
  )

/**
 * Состояние hash-цепочки аудита (INV-A01, FR-1601).
 *
 * Отвечает не «цела ли цепочка прямо сейчас», а «что показала последняя
 * сверка»: пересчет всего журнала по открытию страницы стоил бы полного
 * прохода по аудиту. Сверку ведет фоновый воркер по расписанию - здесь
 * читается его след.
 */
export const auditChainQuery = queryOptions({
  queryKey: ["audit", "chain"],
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/admin/audit/chain")
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load audit chain")
    }
    return data
  },
})

/** Объявление для админской формы, включая скрытый черновик. */
export const adminSiteAnnouncementQuery = queryOptions({
  queryKey: ["admin", "site-announcement"],
  queryFn: async () => {
    const { data, error, response } = await api.GET(
      "/api/v1/admin/site-announcement"
    )
    if (response.status === 404) return null
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load site announcement")
    }
    return data
  },
})

export const saveSiteAnnouncement = (body: {
  title: string
  body: string
  is_published: boolean
}) => mutate(api.PUT("/api/v1/admin/site-announcement", { body }))

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
