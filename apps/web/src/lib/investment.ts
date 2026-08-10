import { queryOptions } from "@tanstack/react-query"

import { m } from "#/paraglide/messages"
import { getLocale } from "#/paraglide/runtime"
import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type InvestmentContract = components["schemas"]["InvestmentContractDto"]
export type InvestmentAttachment =
  components["schemas"]["InvestmentAttachmentDto"]
export type InvestmentAcceptance = components["schemas"]["AcceptanceDto"]

/** Инвестиционные договоры (FR-1204, п. 91–94). */
export const investmentContractsQuery = queryOptions({
  queryKey: ["investment-contracts"],
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/investment-contracts")
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load contracts")
    }
    return data
  },
})

/** Закрытый перечень приложений проекта (п. 91). */
export const investmentAttachmentsQuery = queryOptions({
  queryKey: ["investment-attachments"],
  staleTime: 3_600_000,
  queryFn: async () => {
    const { data, error } = await api.GET(
      "/api/v1/refdata/investment-attachments"
    )
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load attachments")
    }
    return data
  },
})

/** Акты приемки инвестиций по договору (п. 92). */
export const investmentAcceptancesQuery = (contractId: string) =>
  queryOptions({
    queryKey: ["investment-acceptances", contractId],
    queryFn: async () => {
      const { data, error } = await api.GET(
        "/api/v1/investment-contracts/{id}/acceptances",
        { params: { path: { id: contractId } } }
      )
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load acceptances")
      }
      return data
    },
  })

/** Локализованная подпись позиции перечня п. 91. */
export function attachmentLabel(
  attachments: InvestmentAttachment[],
  code: string
): string {
  const attachment = attachments.find((item) => item.code === code)
  if (attachment === undefined) return code
  const locale = getLocale()
  if (locale === "kk") return attachment.label_kk ?? attachment.label_ru
  if (locale === "en") return attachment.label_en ?? attachment.label_ru
  return attachment.label_ru
}

/** Подпись способа продления (п. 93). */
export function extensionLabel(code: string): string {
  return code === "prolongation"
    ? m.investment_prolongation()
    : m.investment_extension()
}
