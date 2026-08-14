import { useState } from "react"
import { useMutation, useSuspenseQuery } from "@tanstack/react-query"

import { m } from "#/paraglide/messages"
import { Panel } from "@/components/panel"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { api } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import { activeCommissionQuery, memberRoleLabel } from "@/lib/commission"
import { formatDateTime } from "@/lib/format"

import type { MeetingDto } from "@/lib/participant"

/**
 * Заседание комиссии (FR-1102, FR-1104): секретарь отмечает явку и
 * председательствующего, открывает заседание (кворум ⅔ проверяет сервер) и
 * фиксирует отводы по конфликту интересов.
 *
 * Панель переехала из файла маршрута в компоненты вместе с разбиением экрана
 * секретаря на вкладки: маршрут отвечает за то, что где лежит, а не за то,
 * как устроено заседание.
 */
export function MeetingPanel({
  tenderId,
  meeting,
  onChanged,
}: {
  tenderId: string
  meeting: MeetingDto | null
  onChanged: () => Promise<void>
}) {
  const { data: commission } = useSuspenseQuery(activeCommissionQuery)
  const opened = meeting?.opened_at != null

  const [present, setPresent] = useState<Record<string, boolean>>({})
  const [chairing, setChairing] = useState<string>("")
  const [recusalMember, setRecusalMember] = useState<string>("")
  const [recusalReason, setRecusalReason] = useState("")
  const [replacement, setReplacement] = useState<string>("")

  const members = commission?.members ?? []
  const recorded = new Map(
    (meeting?.attendance ?? []).map((row) => [row.member_id, row])
  )
  const isPresent = (memberId: string) =>
    present[memberId] ?? recorded.get(memberId)?.present ?? false
  const chairingId =
    chairing ||
    (meeting?.attendance ?? []).find((row) => row.chairing)?.member_id ||
    ""

  // Заседание создается вместе с первой отметкой явки (до вскрытия, п. 12)
  const saveAttendance = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST(
        "/api/v1/tenders/{id}/meeting/attendance",
        {
          params: { path: { id: tenderId } },
          body: {
            rows: members.map((member) => ({
              member_id: member.member_id,
              present: isPresent(member.member_id),
              chairing: chairingId === member.member_id,
            })),
          },
        }
      )
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("attendance failed")
      }
      return data
    },
    onSuccess: onChanged,
  })

  const openMeeting = useMutation({
    mutationFn: async () => {
      const { error } = await api.POST("/api/v1/tenders/{id}/meeting/open", {
        params: { path: { id: tenderId } },
      })
      if (error !== undefined) throw error
    },
    onSuccess: onChanged,
  })

  const recuse = useMutation({
    mutationFn: async () => {
      const { error } = await api.POST("/api/v1/tenders/{id}/recusals", {
        params: { path: { id: tenderId } },
        body: {
          member_id: recusalMember,
          reason: recusalReason,
          replacement_member_id: replacement || null,
          lot_id: null,
        },
      })
      if (error !== undefined) throw error
    },
    onSuccess: async () => {
      setRecusalReason("")
      await onChanged()
    },
  })

  if (commission === null) {
    return <p className="text-muted-foreground">{m.commission_none()}</p>
  }

  return (
    <div className="flex flex-col gap-4">
      <Panel title={m.meeting_title()} titleAs="h3">
        <dl className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <div className="flex flex-col gap-0.5">
            <dt className="text-sm text-muted-foreground">
              {m.meeting_commission()}
            </dt>
            <dd className="font-medium">{commission.name}</dd>
          </div>
          <div className="flex flex-col gap-0.5">
            <dt className="text-sm text-muted-foreground">
              {m.meeting_opened_at()}
            </dt>
            <dd className="font-medium tabular-nums" suppressHydrationWarning>
              {formatDateTime(meeting?.opened_at) ?? m.meeting_not_opened()}
            </dd>
          </div>
          <div className="flex flex-col gap-0.5">
            <dt className="text-sm text-muted-foreground">
              {m.meeting_quorum()}
            </dt>
            <dd className="font-medium" data-testid="meeting-quorum">
              {meeting?.quorum_present == null
                ? m.meeting_quorum_needed({ count: commission.quorum_required })
                : m.meeting_quorum_value({
                    present: meeting.quorum_present,
                    required: meeting.quorum_required ?? 0,
                  })}
            </dd>
          </div>
        </dl>
      </Panel>

      {!opened && (
        <Panel title={m.attendance_legend()} titleAs="h3">
          <form
            data-testid="attendance-form"
            className="flex flex-col gap-4"
            onSubmit={(event) => {
              event.preventDefault()
              saveAttendance.mutate()
            }}
          >
            <fieldset className="flex flex-col gap-2">
              <legend className="sr-only">{m.attendance_legend()}</legend>
              <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
                {members.map((member) => (
                  <label
                    key={member.member_id}
                    className="flex items-center gap-2 text-sm"
                  >
                    <input
                      type="checkbox"
                      name="present"
                      value={member.member_id}
                      checked={isPresent(member.member_id)}
                      onChange={(event) =>
                        setPresent((current) => ({
                          ...current,
                          [member.member_id]: event.target.checked,
                        }))
                      }
                    />
                    <span>{member.full_name}</span>
                    <span className="text-muted-foreground">
                      {memberRoleLabel(member.member_role)}
                    </span>
                  </label>
                ))}
              </div>
            </fieldset>

            <div className="flex flex-col gap-1.5">
              <Label htmlFor="chairing">{m.attendance_chairing_label()}</Label>
              <NativeSelect
                id="chairing"
                value={chairingId}
                onChange={(event) => {
                  const memberId = event.target.value
                  setChairing(memberId)
                  // Председательствующий обязан присутствовать (CHECK БД)
                  if (memberId !== "") {
                    setPresent((current) => ({ ...current, [memberId]: true }))
                  }
                }}
              >
                <NativeSelectOption value="">-</NativeSelectOption>
                {members
                  .filter(
                    (member) =>
                      member.member_role === "chairman" ||
                      member.member_role === "deputy"
                  )
                  .map((member) => (
                    <NativeSelectOption
                      key={member.member_id}
                      value={member.member_id}
                    >
                      {member.full_name}
                    </NativeSelectOption>
                  ))}
              </NativeSelect>
            </div>

            <div className="flex flex-wrap gap-3">
              <Button
                type="submit"
                data-testid="save-attendance"
                disabled={saveAttendance.isPending}
              >
                {m.attendance_save()}
              </Button>
              <Button
                type="button"
                variant="outline"
                data-testid="open-meeting"
                onClick={() => openMeeting.mutate()}
                disabled={openMeeting.isPending || saveAttendance.isPending}
              >
                {m.meeting_open_button()}
              </Button>
            </div>
            {saveAttendance.isError && (
              <p role="alert" className="text-sm text-destructive">
                {problemMessage(saveAttendance.error)}
              </p>
            )}
            {openMeeting.isError && (
              <p role="alert" className="text-sm text-destructive">
                {problemMessage(openMeeting.error)}
              </p>
            )}
          </form>
        </Panel>
      )}

      <Panel title={m.recusal_title()} titleAs="h3">
        <form
          data-testid="recusal-form"
          className="flex flex-wrap items-end gap-3"
          onSubmit={(event) => {
            event.preventDefault()
            recuse.mutate()
          }}
        >
          <div className="flex min-w-56 flex-1 flex-col gap-1.5">
            <Label htmlFor="recusal-member">{m.recusal_member_label()}</Label>
            <NativeSelect
              id="recusal-member"
              value={recusalMember}
              onChange={(event) => setRecusalMember(event.target.value)}
            >
              <NativeSelectOption value="">-</NativeSelectOption>
              {members.map((member) => (
                <NativeSelectOption
                  key={member.member_id}
                  value={member.member_id}
                >
                  {member.full_name}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </div>
          <div className="flex min-w-56 flex-1 flex-col gap-1.5">
            <Label htmlFor="recusal-reason">{m.recusal_reason_label()}</Label>
            <Input
              id="recusal-reason"
              value={recusalReason}
              onChange={(event) => setRecusalReason(event.target.value)}
            />
          </div>
          <div className="flex min-w-56 flex-1 flex-col gap-1.5">
            <Label htmlFor="recusal-replacement">
              {m.recusal_replacement_label()}
            </Label>
            <NativeSelect
              id="recusal-replacement"
              value={replacement}
              onChange={(event) => setReplacement(event.target.value)}
            >
              <NativeSelectOption value="">-</NativeSelectOption>
              {members
                .filter((member) => member.member_role === "reserve")
                .map((member) => (
                  <NativeSelectOption
                    key={member.member_id}
                    value={member.member_id}
                  >
                    {member.full_name}
                  </NativeSelectOption>
                ))}
            </NativeSelect>
          </div>
          <Button
            type="submit"
            data-testid="recuse-member"
            disabled={recuse.isPending}
          >
            {m.recusal_submit()}
          </Button>
          {recuse.isError && (
            <p role="alert" className="w-full text-sm text-destructive">
              {problemMessage(recuse.error)}
            </p>
          )}
        </form>

        {(meeting?.recusals.length ?? 0) > 0 && (
          <ul className="mt-4 flex flex-col gap-1 text-sm">
            {meeting?.recusals.map((recusal) => (
              <li key={recusal.member_id} className="text-muted-foreground">
                {m.recusal_row({
                  member: recusal.full_name,
                  reason: recusal.reason,
                  replacement: recusal.replacement_name ?? "-",
                })}
              </li>
            ))}
          </ul>
        )}
      </Panel>
    </div>
  )
}
