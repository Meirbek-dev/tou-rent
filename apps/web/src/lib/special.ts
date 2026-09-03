import { queryOptions } from "@tanstack/react-query"

import { m } from "#/paraglide/messages"
import { api } from "#/lib/api"
import { serverLabel } from "#/lib/server-label"

import type { components } from "@tou/api-client"

export type SpecialCategory = components["schemas"]["SpecialCategoryDto"]
export type CategoryDocument = components["schemas"]["CategoryDocumentDto"]
export type SpecialRequest = components["schemas"]["SpecialRequestDto"]
export type SpecialRequestFile = components["schemas"]["SpecialRequestFileDto"]

/** Каталог категорий особого порядка (FR-1201, п. 87): закрытый перечень. */
export const specialCategoriesQuery = queryOptions({
  queryKey: ["special-categories"],
  staleTime: 3_600_000,
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/refdata/special-categories")
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load categories")
    }
    return data
  },
})

/** Мои заявки особого порядка (кабинет заявителя). */
export const mySpecialRequestsQuery = queryOptions({
  queryKey: ["special-requests", "my"],
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/special-requests/my")
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load special requests")
    }
    return data
  },
})

/** Заявки в рассмотрении (FR-1202): рабочий список подразделения и Правления. */
export const pendingSpecialRequestsQuery = specialRequestsQuery()

/** Заявки по состоянию: без фильтра - те, что в рассмотрении (п. 89–90). */
export function specialRequestsQuery(status?: string) {
  return queryOptions({
    queryKey: ["special-requests", status ?? "pending"],
    queryFn: async () => {
      const { data, error } = await api.GET("/api/v1/special-requests", {
        params: { query: status === undefined ? {} : { status } },
      })
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load special requests")
      }
      return data
    },
  })
}

/** Ход рассмотрения заявки: заключение подразделения и решение Правления. */
export const specialProgressQuery = (requestId: string) =>
  queryOptions({
    queryKey: ["special-progress", requestId],
    queryFn: async () => {
      const { data, error } = await api.GET(
        "/api/v1/special-requests/{id}/progress",
        { params: { path: { id: requestId } } }
      )
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load progress")
      }
      return data
    },
  })

export type SpecialCompetition = components["schemas"]["CompetitionDto"]

/** Конкуренция вокруг заявки (FR-1203, п. 86, 97). */
export const specialCompetitionQuery = (requestId: string) =>
  queryOptions({
    queryKey: ["special-competition", requestId],
    queryFn: async () => {
      const { data, error } = await api.GET(
        "/api/v1/special-requests/{id}/competition",
        { params: { path: { id: requestId } } }
      )
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load competition")
      }
      return data
    },
  })

/** Подпись решения из закрытого перечня п. 90. */
export function decisionLabel(code: string): string {
  switch (code) {
    case "grant":
      return m.special_decision_grant()
    case "refuse":
      return m.special_decision_refuse()
    case "redirect":
      return m.special_decision_redirect()
    default:
      return code
  }
}

/** Локализованная подпись категории или позиции перечня документов. */
export function localeLabel(item: {
  label_ru: string
  label_kk?: string | null
  label_en?: string | null
}): string {
  return serverLabel(item)
}

/**
 * Подпись категории заявки особого порядка (FR-1203, п. 87).
 *
 * Категория - позиция `refdata.special_categories`, и подписи у нее три.
 * В `SpecialRequestDto` русская лежит в самом `category_label`, казахская и
 * английская - рядом; кабинеты печатали русскую во всех локалях (G-066).
 */
export function requestCategoryLabel(request: {
  category_label: string
  category_label_kk?: string | null
  category_label_en?: string | null
}): string {
  return serverLabel(request, "category_label")
}

/** Минимум, которого хватает для подписи категории: сам справочник шире. */
export type CategoryLabelSource = {
  code: string
  label_ru: string
  label_kk?: string | null
  label_en?: string | null
}

/**
 * Подпись категории в уведомлении о решении Правления (FR-1301, п. 90).
 *
 * Код категории переводится справочником, а не сервером: до этого payload
 * нес готовую русскую подпись, и в казахском кабинете она оставалась
 * русской, хотя `refdata.special_categories` держит label_kk и label_en.
 *
 * Откат - к тексту из самого уведомления: у разосланных до правки строк
 * кода нет вовсе (поле `category`), а у новых он есть, но справочник
 * участнику мог не загрузиться.
 */
export function specialCategoryLabel(
  payload: {
    category_code?: string | undefined
    category_ru?: string | undefined
    category?: string | undefined
  },
  categories?: readonly CategoryLabelSource[]
): string {
  const code = payload.category_code
  const known =
    code === undefined
      ? undefined
      : categories?.find((category) => category.code === code)

  if (known !== undefined) return localeLabel(known)
  return payload.category_ru ?? payload.category ?? code ?? "-"
}

/** Подпись состояния заявки особого порядка (п. 88–90). */
export function specialStatusLabel(status: string): string {
  switch (status) {
    case "submitted":
      return m.special_status_submitted()
    case "under_review":
      return m.special_status_under_review()
    case "granted":
      return m.special_status_granted()
    case "refused":
      return m.special_status_refused()
    case "redirected":
      return m.special_status_redirected()
    case "withdrawn":
      return m.special_status_withdrawn()
    default:
      return status
  }
}

/** Срок проверки категории (FR-1202): вид дней задает справочник. */
export function reviewTermLabel(category: SpecialCategory): string {
  return category.review_term === "business"
    ? m.special_review_business({ days: category.review_days })
    : m.special_review_calendar({ days: category.review_days })
}

/** Льготная схема категории (FR-1205); `none` - льгота не применяется. */
export function benefitSchemeLabel(code: string): string {
  switch (code) {
    case "educational_equipment":
      return m.special_benefit_educational()
    case "spin_off":
      return m.special_benefit_spin_off()
    case "social":
      return m.special_benefit_social()
    default:
      return m.special_benefit_none()
  }
}
