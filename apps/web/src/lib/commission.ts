import { queryOptions } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"

import { api } from "#/lib/api"

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

/**
 * Место текущего пользователя в комиссии: `null` - он в составе не значится.
 *
 * Кабинет комиссии открывается по роли, а голосует не роль, а член состава:
 * резервный член (п. 11) прав голоса не имеет до замещения выбывшего, и
 * до сих пор кнопки «За/Против» ему рисовались наравне со всеми - отказ
 * приходил только после подтверждения «необратимого» голоса.
 */
export function commissionSeat(
  commission: CommissionDto | null,
  userId: string | undefined
): CommissionMember | null {
  if (commission === null || userId === undefined) return null
  return commission.members.find((member) => member.user_id === userId) ?? null
}

/**
 * Свой уже поданный голос по заявке; `null` - не голосовал.
 *
 * Голос - процессуальный акт и отзыву не подлежит (п. 13–14): увидев его,
 * член комиссии не должен получать вторую пару живых кнопок.
 */
export function ownVote(
  votes: ApplicationVotes | null | undefined,
  memberId: string | undefined
): ApplicationVotes["votes"][number] | null {
  if (votes == null || memberId === undefined) return null
  return votes.votes.find((vote) => vote.member_id === memberId) ?? null
}

/**
 * Имя заявителя для строки реестра, карточки решения и голосования.
 *
 * `applicant_details` - свободная форма из заявки (FR-403), и имя в ней
 * может отсутствовать. Тогда остается короткий идентификатор - но он
 * запасной, а не единственный: восемь символов UUIDv7 у заявок одного
 * тендера совпадают, и таблица из таких строк не опознает ни одну.
 */
export function applicantName(application: {
  id: string
  applicant_details?: unknown
}): string {
  const details = application.applicant_details
  if (details !== null && typeof details === "object" && "name" in details) {
    const name = (details as Record<string, unknown>)["name"]
    if (typeof name === "string" && name.trim() !== "") return name
  }
  return application.id.slice(0, 8)
}

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
