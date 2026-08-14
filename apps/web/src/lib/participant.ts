import { queryOptions } from "@tanstack/react-query"

import { api } from "@/lib/api"
import { serverLabel } from "@/lib/server-label"

import type { components } from "@tou/api-client"

export type ApplicationDto = components["schemas"]["ApplicationDto"]
export type ApplicationStatus = components["schemas"]["ApplicationStatusDto"]
export type ApplicantKind = components["schemas"]["ApplicantKindDto"]
export type ApplicationFile = components["schemas"]["ApplicationFileDto"]
export type JournalEntry = components["schemas"]["JournalEntryDto"]

/** Мои заявки (FR-401/404): своя цена видна всегда (RLS INV-040). */
export const myApplicationsQuery = queryOptions({
  queryKey: ["applications", "my"],
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/applications/my")
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load applications")
    }
    return data
  },
})

/** Заявки тендера глазами секретаря/комиссии: цены до вскрытия - null. */
export const tenderApplicationsQuery = (tenderId: string) =>
  queryOptions({
    queryKey: ["applications", "tender", tenderId],
    queryFn: async () => {
      const { data, error } = await api.GET(
        "/api/v1/tenders/{id}/applications",
        {
          params: { path: { id: tenderId } },
        }
      )
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load applications")
      }
      return data
    },
  })

export type MeetingDto = components["schemas"]["MeetingDto"]
export type RejectionReason = components["schemas"]["RejectionReasonDto"]
/** Факт формирования протокола допуска (FR-503): номер и время. */
export type GeneratedProtocolDto = components["schemas"]["GeneratedProtocolDto"]
export type VoteValue = components["schemas"]["VoteValueDto"]

/** Заседание допуска (после вскрытия): состав комиссии для голосов (FR-503). */
export const meetingQuery = (tenderId: string) =>
  queryOptions({
    queryKey: ["meeting", tenderId],
    queryFn: async () => {
      const { data, response, error } = await api.GET(
        "/api/v1/tenders/{id}/meeting",
        { params: { path: { id: tenderId } } }
      )
      if (response.status === 404) return null
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load meeting")
      }
      return data
    },
  })

/** Закрытый перечень оснований отклонения (FR-502, п. 52). */
export const rejectionReasonsQuery = queryOptions({
  queryKey: ["rejection-reasons"],
  staleTime: 3_600_000,
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/refdata/rejection-reasons")
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load reasons")
    }
    return data
  },
})

/** Реквизиты протокола допуска; null - еще не сформирован. */
export const admissionProtocolQuery = (tenderId: string) =>
  queryOptions({
    queryKey: ["admission-protocol", tenderId],
    queryFn: async () => {
      const { data, response, error } = await api.GET(
        "/api/v1/tenders/{id}/admission-protocol",
        { params: { path: { id: tenderId } } }
      )
      if (response.status === 404) return null
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load protocol")
      }
      return data
    },
  })

/** Локализованная подпись основания отклонения (FR-502, п. 52). */
export function reasonLabel(
  reasons: RejectionReason[],
  code: string | null | undefined
): string | null {
  if (code == null) return null
  const reason = reasons.find((r) => r.code === code)
  if (reason === undefined) return code
  return serverLabel(reason)
}

/** Журнал регистрации (Прил. 12, FR-402). */
export const tenderJournalQuery = (tenderId: string) =>
  queryOptions({
    queryKey: ["journal", tenderId],
    queryFn: async () => {
      const { data, error } = await api.GET("/api/v1/tenders/{id}/journal", {
        params: { path: { id: tenderId } },
      })
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load journal")
      }
      return data
    },
  })
