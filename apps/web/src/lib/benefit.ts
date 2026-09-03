import { queryOptions } from "@tanstack/react-query"

import { m } from "#/paraglide/messages"
import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type BenefitSchemeDto = components["schemas"]["BenefitSchemeDto"]
export type BenefitGrant = components["schemas"]["BenefitGrantDto"]

/** Каталог льготных схем (FR-1205, п. 95–96, Прил. 4). */
export const benefitSchemesQuery = queryOptions({
  queryKey: ["benefit-schemes"],
  staleTime: 3_600_000,
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/refdata/benefit-schemes")
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load benefit schemes")
    }
    return data
  },
})

/**
 * Льгота договора с расписанием платы; `null` - льгота не применялась.
 *
 * «Льготы нет» сервер отдает пустым телом с кодом 200 (G-067): 404 остался
 * за ненайденным договором, и молчаливо превращать его в «льготы нет»
 * кабинету больше нельзя - это разные вещи.
 *
 * Читать льготу вправе не всякая роль (`Compound::BENEFIT_READ`), и панель
 * льготы не монтируется там, где права заведомо нет: 403 в консоли на
 * каждой загрузке карточки договора - не диагностика, а шум.
 */
export const contractBenefitQuery = (contractId: string) =>
  queryOptions({
    queryKey: ["contract-benefit", contractId],
    queryFn: async () => {
      const { data, error } = await api.GET("/api/v1/contracts/{id}/benefit", {
        params: { path: { id: contractId } },
      })
      if (error !== undefined) {
        throw error as unknown
      }
      return data ?? null
    },
  })

/** Подпись льготной схемы (FR-1205). */
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

/** Чем определяется плата за год найма (п. 95–96). */
export function yearRuleLabel(rule: string): string {
  switch (rule) {
    case "communal_only":
      return m.benefit_rule_communal()
    case "share":
      return m.benefit_rule_share()
    default:
      return m.benefit_rule_full()
  }
}
