import { queryOptions } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"

import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type CommissionDto = components["schemas"]["CommissionDto"]
export type CommissionMember = components["schemas"]["MemberDto"]
export type MemberRole = components["schemas"]["MemberRoleDto"]
export type AttendanceRow = components["schemas"]["AttendanceRowDto"]
export type ApplicationVotes = components["schemas"]["ApplicationVotesDto"]
export type Tally = components["schemas"]["TallyDto"]
export type VoteValue = components["schemas"]["VoteValueDto"]

/** Действующая комиссия и ее состав (FR-1101). null - комиссии нет. */
export const activeCommissionQuery = queryOptions({
  queryKey: ["commission", "active"],
  queryFn: async () => {
    const { data, error, response } = await api.GET(
      "/api/v1/commissions/active"
    )
    if (response.status === 404) return null
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load commission")
    }
    return data
  },
})

/** Голоса и подсчет по заявке (FR-1103): база большинства - присутствующие. */
export const applicationVotesQuery = (applicationId: string) =>
  queryOptions({
    queryKey: ["votes", applicationId],
    queryFn: async () => {
      const { data, error, response } = await api.GET(
        "/api/v1/applications/{id}/votes",
        { params: { path: { id: applicationId } } }
      )
      if (response.status === 404) return null
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load votes")
      }
      return data
    },
  })

/** Подпись роли члена комиссии (п. 9–11): одна на весь интерфейс. */
export function memberRoleLabel(role: string): string {
  switch (role) {
    case "chairman":
      return m.member_role_chairman()
    case "deputy":
      return m.member_role_deputy()
    case "reserve":
      return m.member_role_reserve()
    default:
      return m.member_role_member()
  }
}
