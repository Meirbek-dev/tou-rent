import { queryOptions } from "@tanstack/react-query"

import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type ContractAmendment = components["schemas"]["ContractAmendmentDto"]
export type AmendableField = components["schemas"]["AmendableFieldDto"]

async function mutate<T>(promise: Promise<{ data?: T; error?: unknown }>) {
  const { data, error } = await promise
  if (error !== undefined || data === undefined) {
    throw error ?? new Error("request failed")
  }
  return data
}

/** Допсоглашения договора (FR-906, п. 125). */
export const contractAmendmentsQuery = (contractId: string) =>
  queryOptions({
    queryKey: ["contract-amendments", contractId],
    queryFn: () =>
      mutate(
        api.GET("/api/v1/contracts/{id}/amendments", {
          params: { path: { id: contractId } },
        })
      ),
  })

/** Изменяемые поля договора (п. 125): существенных условий в перечне нет. */
export const amendableFieldsQuery = queryOptions({
  queryKey: ["refdata", "amendable-fields"],
  queryFn: () => mutate(api.GET("/api/v1/refdata/amendable-fields")),
})

/** Заключение допсоглашения с diff-контролем (FR-906, FR-901). */
export const createAmendment = (
  contractId: string,
  body: {
    ground: string
    effective_on: string
    changes: { field_code: string; old_value: string; new_value: string }[]
  }
) =>
  mutate(
    api.POST("/api/v1/contracts/{id}/amendments", {
      params: { path: { id: contractId } },
      body,
    })
  )
