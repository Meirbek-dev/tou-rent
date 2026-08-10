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

/** Льгота договора с расписанием платы; null - льгота не применялась. */
export const contractBenefitQuery = (contractId: string) =>
  queryOptions({
    queryKey: ["contract-benefit", contractId],
    queryFn: async () => {
      const { data, response, error } = await api.GET(
        "/api/v1/contracts/{id}/benefit",
        { params: { path: { id: contractId } } }
      )
      if (response.status === 404) return null
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load benefit")
      }
      return data
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
