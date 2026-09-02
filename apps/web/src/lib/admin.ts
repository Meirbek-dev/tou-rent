import { queryOptions } from "@tanstack/react-query"

import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type UserDto = components["schemas"]["UserDto"]
export type MrpDto = components["schemas"]["MrpDto"]
export type CoefficientVersionDto =
  components["schemas"]["CoefficientVersionDto"]
export type SiteAnnouncementDto = components["schemas"]["SiteAnnouncementDto"]
export type AdminDataOverviewDto = components["schemas"]["AdminDataOverviewDto"]
export type AdminPurgeScope = components["schemas"]["AdminPurgeScope"]
/** Вид данных вкладки «Данные»: любая область, кроме полной очистки. */
export type AdminDataKind = Exclude<AdminPurgeScope, "everything">
export type AdminRecordDto = components["schemas"]["AdminRecordDto"]

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

/** Слово подтверждения полной очистки - то же, что ждет сервер. */
export const PURGE_CONFIRMATION = "purge"

/**
 * Обзор данных стенда для вкладки «Данные»: что и сколько уйдет при очистке
 * и разрешена ли она конфигурацией стенда (`ALLOW_DATA_PURGE`).
 */
export const dataOverviewQuery = queryOptions({
  queryKey: ["admin", "data"],
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/admin/data")
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load data overview")
    }
    return data
  },
})

/**
 * Массовая очистка: весь стенд либо все записи одной области со всем, что
 * на них держится, одной транзакцией. Необратима; сервер сверяет слово
 * подтверждения сам - кнопки лишь не дают нажать раньше, чем оно набрано.
 */
export const purgeData = async (
  scope: AdminPurgeScope,
  confirmation: string
) => {
  const { data, error } = await api.POST("/api/v1/admin/data/purge", {
    body: { confirmation, scope },
  })
  if (error !== undefined || data === undefined) {
    throw (error as unknown) ?? new Error("purge failed")
  }
  return data
}

/**
 * Записи одного вида для точечного удаления, свежие сверху. Ключ включает
 * вид: смена вида в селекторе - другой запрос, а не перерисовка старого.
 */
export const recordsQuery = (kind: AdminDataKind) =>
  queryOptions({
    queryKey: ["admin", "data", "records", kind],
    queryFn: async () => {
      const { data, error } = await api.GET("/api/v1/admin/data/records", {
        params: { query: { kind } },
      })
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load records")
      }
      return data
    },
  })

/**
 * Удаление одной записи любого вида со всем, что на ней держится: заявка
 * уносит файлы, цену, журнал, торги и договор по ней, лот - заявки и
 * торги, объект - тендеры по нему.
 */
export const purgeRecord = async (kind: AdminDataKind, id: string) => {
  const { data, error } = await api.DELETE(
    "/api/v1/admin/data/records/{kind}/{id}",
    { params: { path: { kind, id } } }
  )
  if (error !== undefined || data === undefined) {
    throw (error as unknown) ?? new Error("record purge failed")
  }
  return data
}

/** Отключение демо-учеток `*@tou.demo` кроме своей (обратимо, W-07). */
export const deactivateDemoAccounts = async () => {
  const { data, error } = await api.POST(
    "/api/v1/admin/demo-accounts/deactivate"
  )
  if (error !== undefined || data === undefined) {
    throw (error as unknown) ?? new Error("demo accounts deactivation failed")
  }
  return data
}

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
  title_kk: string
  body: string
  body_kk: string
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
