import { queryOptions } from "@tanstack/react-query"
import { api } from "@/lib/api"
import type { components } from "@tou/api-client"

export type CommissionDocument = components["schemas"]["CommissionDocumentDto"]

async function result<T>(request: Promise<{ data?: T; error?: unknown }>) {
  const { data, error } = await request
  if (error !== undefined || data === undefined)
    throw error ?? new Error("Request failed")
  return data
}

export const commissionDocumentsQuery = (tenderId: string) =>
  queryOptions({
    queryKey: ["commission-documents", tenderId],
    queryFn: () =>
      result(
        api.GET("/api/v1/tenders/{id}/commission-documents", {
          params: { path: { id: tenderId } },
        })
      ),
  })

export function uploadCommissionDocument(
  tenderId: string,
  applicationId: string | undefined,
  title: string,
  number: string,
  documentDate: string,
  file: File
) {
  const body = new FormData()
  body.append("file", file)
  return result(
    api.POST("/api/v1/tenders/{id}/commission-documents", {
      params: {
        path: { id: tenderId },
        query: {
          title,
          number,
          document_date: documentDate,
          ...(applicationId === undefined
            ? {}
            : { application_id: applicationId }),
        },
      },
      body: body as unknown as string,
      bodySerializer: (value) => value as unknown as FormData,
    })
  )
}

export function shareCommissionDocument(id: string, shared: boolean) {
  return result(
    api.PUT("/api/v1/commission-documents/{id}/visibility", {
      params: { path: { id } },
      body: { shared },
    })
  )
}
