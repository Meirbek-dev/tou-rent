import { useState } from "react"
import { Link, createFileRoute, notFound } from "@tanstack/react-router"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { DossierPanel } from "@/components/dossier-panel"
import { EvasionPanel } from "@/components/evasion-panel"
import { ProtocolsPanel } from "@/components/protocols-panel"
import { FailurePanel } from "@/components/failure-panel"
import { ApplicationStatusBadge } from "@/components/application-status-badge"
import { AuctionLotsPanel } from "@/components/auction-lots-panel"
import { TenderStatusBadge } from "@/components/tender-status-badge"
import { Button, buttonVariants } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { api, tenderQuery } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import {
  activeCommissionQuery,
  applicationVotesQuery,
  memberRoleLabel,
} from "@/lib/commission"
import { formatDateTime, formatTenge } from "@/lib/format"
import {
  admissionProtocolQuery,
  meetingQuery,
  reasonLabel,
  rejectionReasonsQuery,
  tenderApplicationsQuery,
  tenderJournalQuery,
} from "@/lib/participant"
import { cn } from "@/lib/utils"

import type {
  ApplicationDto,
  MeetingDto,
  RejectionReason,
} from "@/lib/participant"

// Экран заседания секретаря (FR-501–503, FR-1102, FR-1104): явка и открытие
// заседания при кворуме, отводы по конфликту интересов, вскрытие, оглашение
// цен, фиксация решений по итогам голосования комиссии, протокол допуска.
export const Route = createFileRoute("/app/secretary/tenders/$tenderId")({
  loader: async ({ context, params }) => {
    const tender = await context.queryClient.ensureQueryData(
      tenderQuery(params.tenderId)
    )
    if (tender === null) throw notFound()
    await Promise.all([
      context.queryClient.ensureQueryData(tenderJournalQuery(params.tenderId)),
      context.queryClient.ensureQueryData(
        tenderApplicationsQuery(params.tenderId)
      ),
      context.queryClient.ensureQueryData(meetingQuery(params.tenderId)),
      context.queryClient.ensureQueryData(
        admissionProtocolQuery(params.tenderId)
      ),
      context.queryClient.ensureQueryData(rejectionReasonsQuery),
    ])
  },
  component: SecretaryTenderPage,
})

const ENTRY_KIND_LABELS: Record<string, () => string> = {
  application_submitted: m.journal_kind_submitted,
  application_withdrawn: m.journal_kind_withdrawn,
}

function SecretaryTenderPage() {
  const { tenderId } = Route.useParams()
  const queryClient = useQueryClient()
  const { data: tender } = useSuspenseQuery(tenderQuery(tenderId))
  const { data: journal } = useSuspenseQuery(tenderJournalQuery(tenderId))
  const { data: applications } = useSuspenseQuery(
    tenderApplicationsQuery(tenderId)
  )
  const { data: meeting } = useSuspenseQuery(meetingQuery(tenderId))
  const { data: protocol } = useSuspenseQuery(admissionProtocolQuery(tenderId))
  const { data: reasons } = useSuspenseQuery(rejectionReasonsQuery)

  if (tender === null) throw notFound()
  const sealed = tender.opened_at == null

  const refresh = async () => {
    await Promise.all([
      queryClient.invalidateQueries({
        queryKey: tenderQuery(tenderId).queryKey,
      }),
      queryClient.invalidateQueries({
        queryKey: tenderApplicationsQuery(tenderId).queryKey,
      }),
      queryClient.invalidateQueries({
        queryKey: meetingQuery(tenderId).queryKey,
      }),
      queryClient.invalidateQueries({
        queryKey: admissionProtocolQuery(tenderId).queryKey,
      }),
    ])
  }

  const open = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST("/api/v1/tenders/{id}/open", {
        params: { path: { id: tenderId } },
      })
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("opening failed")
      }
      return data
    },
    onSuccess: refresh,
  })

  const generateProtocol = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST(
        "/api/v1/tenders/{id}/admission-protocol",
        { params: { path: { id: tenderId } } }
      )
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("protocol generation failed")
      }
      return data
    },
    onSuccess: refresh,
  })

  // FR-504: после протокола - уведомление допущенных (стартовая ставка,
  // дата торгов); повторная рассылка отклоняется API (409)
  const notifyAdmitted = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST(
        "/api/v1/tenders/{id}/notify-admitted",
        { params: { path: { id: tenderId } } }
      )
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("notify failed")
      }
      return data
    },
    onSuccess: refresh,
  })

  const decidable = applications.filter(
    (a) => a.status === "submitted" || a.status === "fee_confirmed"
  )

  return (
    <div className="flex flex-col gap-8">
      <nav>
        <Link
          to="/app/secretary"
          className="text-sm text-muted-foreground underline-offset-4 hover:underline"
        >
          ← {m.back_to_cabinet()}
        </Link>
      </nav>

      <header className="flex flex-col gap-2">
        <div className="flex flex-wrap items-center gap-3">
          <TenderStatusBadge status={tender.status} />
          <span className="text-sm text-muted-foreground">
            {m.tender_card_title({ id: tender.id.slice(0, 8) })}
          </span>
        </div>
        <h2 className="font-heading text-2xl font-semibold">{tender.title}</h2>
        <div className="flex flex-wrap items-center gap-3 pt-1">
          {tender.status === "accepting" && (
            <Button
              data-testid="open-tender"
              onClick={() => open.mutate()}
              disabled={open.isPending || meeting?.opened_at == null}
              title={
                meeting?.opened_at == null ? m.meeting_open_first() : undefined
              }
            >
              {m.opening_button()}
            </Button>
          )}
          {!sealed && protocol === null && (
            <Button
              variant="outline"
              data-testid="generate-admission-protocol"
              onClick={() => generateProtocol.mutate()}
              disabled={generateProtocol.isPending}
            >
              {m.protocol_generate()}
            </Button>
          )}
          {protocol !== null && (
            <a
              href={`/api/v1/tenders/${tender.id}/admission-protocol.pdf`}
              data-testid="admission-protocol-pdf"
              target="_blank"
              rel="noreferrer"
              className={cn(buttonVariants({ variant: "outline" }))}
            >
              {m.protocol_pdf({ number: protocol.number ?? "" })}
            </a>
          )}
          {protocol !== null && (
            <Button
              variant="outline"
              data-testid="notify-admitted"
              onClick={() => notifyAdmitted.mutate()}
              disabled={notifyAdmitted.isPending}
            >
              {m.notify_admitted_button()}
            </Button>
          )}
        </div>
        {tender.trading_at != null && (
          <p className="text-sm text-muted-foreground" suppressHydrationWarning>
            {m.trading_at_label({
              date: formatDateTime(tender.trading_at) ?? "-",
            })}
          </p>
        )}
        {notifyAdmitted.isSuccess && (
          <p className="text-sm text-muted-foreground" suppressHydrationWarning>
            {m.notify_admitted_done({
              count: notifyAdmitted.data.notified,
              date: formatDateTime(notifyAdmitted.data.trading_at) ?? "-",
            })}
          </p>
        )}
        {notifyAdmitted.isError && (
          <p role="alert" className="text-sm text-destructive">
            {problemMessage(notifyAdmitted.error)}
          </p>
        )}
        {tender.status === "accepting" && (
          <p className="text-sm text-muted-foreground">{m.opening_hint()}</p>
        )}
        {open.isError && (
          <p role="alert" className="text-sm text-destructive">
            {problemMessage(open.error)}
          </p>
        )}
        {generateProtocol.isError && (
          <p role="alert" className="text-sm text-destructive">
            {problemMessage(generateProtocol.error)}
          </p>
        )}
      </header>

      {protocol !== null && (
        <AuctionLotsPanel tenderId={tenderId} lots={tender.lots} />
      )}

      <MeetingPanel tenderId={tenderId} meeting={meeting} onChanged={refresh} />

      <section aria-labelledby="journal">
        <h3 id="journal" className="mb-3 font-heading text-lg font-semibold">
          {m.journal_title()}
        </h3>
        {journal.length === 0 ? (
          <p className="text-muted-foreground">{m.journal_empty()}</p>
        ) : (
          <div className="rounded-lg border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead scope="col">{m.journal_seq()}</TableHead>
                  <TableHead scope="col">{m.journal_kind()}</TableHead>
                  <TableHead scope="col">{m.journal_time()}</TableHead>
                  <TableHead scope="col">{m.journal_application()}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {journal.map((entry) => (
                  <TableRow key={entry.seq}>
                    <TableCell className="font-medium">{entry.seq}</TableCell>
                    <TableCell>
                      {ENTRY_KIND_LABELS[entry.entry_kind]?.() ??
                        entry.entry_kind}
                    </TableCell>
                    <TableCell suppressHydrationWarning>
                      {formatDateTime(entry.occurred_at)}
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {entry.application_id?.slice(0, 8) ?? "-"}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
      </section>

      <section aria-labelledby="tender-apps">
        <div className="mb-3 flex flex-wrap items-baseline gap-3">
          <h3 id="tender-apps" className="font-heading text-lg font-semibold">
            {m.tender_applications_title()}
          </h3>
          {sealed && (
            <span className="text-sm text-muted-foreground">
              {m.prices_sealed_note()}
            </span>
          )}
        </div>
        {applications.length === 0 ? (
          <p className="text-muted-foreground">
            {m.tender_applications_empty()}
          </p>
        ) : (
          <div className="rounded-lg border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead scope="col">
                    {m.application_card_short()}
                  </TableHead>
                  <TableHead scope="col">{m.object_status_label()}</TableHead>
                  <TableHead scope="col">
                    {m.application_submitted_at()}
                  </TableHead>
                  <TableHead scope="col" className="text-right">
                    {m.application_price()}
                  </TableHead>
                  <TableHead scope="col">
                    {m.application_files_title()}
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {applications.map((application) => (
                  <TableRow key={application.id}>
                    <TableCell className="font-medium">
                      {application.id.slice(0, 8)}
                    </TableCell>
                    <TableCell>
                      <div className="flex flex-col gap-1">
                        <ApplicationStatusBadge status={application.status} />
                        {application.rejection_reason != null && (
                          <span className="text-xs text-muted-foreground">
                            {reasonLabel(reasons, application.rejection_reason)}
                          </span>
                        )}
                      </div>
                    </TableCell>
                    <TableCell suppressHydrationWarning>
                      {formatDateTime(application.submitted_at)}
                    </TableCell>
                    <TableCell
                      className="text-right tabular-nums"
                      suppressHydrationWarning
                    >
                      {application.price_amount != null
                        ? formatTenge(application.price_amount)
                        : m.application_price_sealed()}
                    </TableCell>
                    <TableCell>
                      {application.files.length === 0 ? (
                        <span className="text-muted-foreground">0</span>
                      ) : sealed ? (
                        <span className="text-muted-foreground">
                          {application.files.length}
                        </span>
                      ) : (
                        <div className="flex flex-col gap-0.5">
                          {application.files.map((file) => (
                            <a
                              key={file.id}
                              href={`/api/v1/applications/${application.id}/files/${file.id}`}
                              className="text-sm underline-offset-4 hover:underline"
                            >
                              {file.filename}
                            </a>
                          ))}
                        </div>
                      )}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
      </section>

      <ProtocolsPanel tenderId={tenderId} canPublish />

      <DossierPanel subject={{ kind: "tender", id: tenderId }} />

      <EvasionPanel tenderId={tenderId} canGenerateProtocol />

      <FailurePanel
        tenderId={tenderId}
        canDeclare
        canRepeat={false}
        onChanged={refresh}
      />

      {meeting !== null && decidable.length > 0 && (
        <section aria-labelledby="decisions" className="flex flex-col gap-4">
          <h3 id="decisions" className="font-heading text-lg font-semibold">
            {m.decisions_title()}
          </h3>
          {decidable.map((application) => (
            <DecisionForm
              key={application.id}
              application={application}
              reasons={reasons}
              onDecided={refresh}
            />
          ))}
        </section>
      )}
    </div>
  )
}

/**
 * Заседание комиссии (FR-1102, FR-1104): секретарь отмечает явку и
 * председательствующего, открывает заседание (кворум ⅔ проверяет сервер) и
 * фиксирует отводы по конфликту интересов.
 */
function MeetingPanel({
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
    <section aria-labelledby="meeting" className="flex flex-col gap-4">
      <h3 id="meeting" className="font-heading text-lg font-semibold">
        {m.meeting_title()}
      </h3>

      <dl className="grid grid-cols-1 gap-3 rounded-lg border p-4 sm:grid-cols-3">
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
          <dd className="font-medium" suppressHydrationWarning>
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

      {!opened && (
        <form
          data-testid="attendance-form"
          className="flex flex-col gap-4 rounded-lg border p-4"
          onSubmit={(event) => {
            event.preventDefault()
            saveAttendance.mutate()
          }}
        >
          <fieldset className="flex flex-col gap-2">
            <legend className="text-sm font-medium">
              {m.attendance_legend()}
            </legend>
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
      )}

      <form
        data-testid="recusal-form"
        className="flex flex-wrap items-end gap-3 rounded-lg border p-4"
        onSubmit={(event) => {
          event.preventDefault()
          recuse.mutate()
        }}
      >
        <div className="flex min-w-56 flex-col gap-1.5">
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
        <div className="flex min-w-56 flex-col gap-1.5">
          <Label htmlFor="recusal-reason">{m.recusal_reason_label()}</Label>
          <Input
            id="recusal-reason"
            value={recusalReason}
            onChange={(event) => setRecusalReason(event.target.value)}
          />
        </div>
        <div className="flex min-w-56 flex-col gap-1.5">
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
        <ul className="flex flex-col gap-1 text-sm">
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
    </section>
  )
}

function DecisionForm({
  application,
  reasons,
  onDecided,
}: {
  application: ApplicationDto
  reasons: RejectionReason[]
  onDecided: () => Promise<void>
}) {
  const queryClient = useQueryClient()
  const { data: votes } = useSuspenseQuery(
    applicationVotesQuery(application.id)
  )
  const [reason, setReason] = useState(reasons[0]?.code ?? "")

  // Вердикт считает сервер по голосам комиссии (FR-1103): секретарь его не
  // выбирает - он лишь фиксирует решение и, при отклонении, основание п. 52
  const tally = votes?.tally ?? null
  const rejecting = tally?.outcome === "rejected"

  const decide = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST(
        "/api/v1/applications/{id}/decide",
        {
          params: { path: { id: application.id } },
          body: { rejection_reason: rejecting ? reason : null },
        }
      )
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("decision failed")
      }
      return data
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: applicationVotesQuery(application.id).queryKey,
      })
      await onDecided()
    },
  })

  const applicantName =
    typeof application.applicant_details === "object" &&
    application.applicant_details !== null &&
    "name" in application.applicant_details
      ? String(
          (application.applicant_details as Record<string, unknown>)["name"]
        )
      : application.id.slice(0, 8)

  return (
    <form
      data-testid={`decide-form-${application.id}`}
      className="flex flex-col gap-4 rounded-lg border p-4"
      onSubmit={(event) => {
        event.preventDefault()
        decide.mutate()
      }}
    >
      <div className="flex flex-wrap items-center gap-3">
        <span className="font-medium">{applicantName}</span>
        <span
          className="text-sm text-muted-foreground"
          suppressHydrationWarning
        >
          {application.price_amount != null
            ? formatTenge(application.price_amount)
            : m.application_price_sealed()}
        </span>
      </div>

      {tally !== null && (
        <div className="flex flex-col gap-2">
          <p className="text-sm" data-testid={`tally-${application.id}`}>
            {m.voting_tally({
              for: tally.votes_for,
              against: tally.votes_against,
              eligible: tally.eligible,
            })}
          </p>
          {tally.outcome === null ? (
            <p className="text-sm text-muted-foreground">
              {tally.pending ?? m.voting_pending()}
            </p>
          ) : (
            <p className="text-sm font-medium">
              {tally.outcome === "admitted"
                ? m.decide_outcome_admitted()
                : m.decide_outcome_rejected()}
            </p>
          )}
          {votes !== null && votes.votes.length > 0 && (
            <ul className="flex flex-col gap-0.5 text-sm text-muted-foreground">
              {votes.votes.map((vote) => (
                <li key={vote.member_id}>
                  {m.voting_member_row({
                    member: vote.member_name,
                    value:
                      vote.value === "for" ? m.vote_for() : m.vote_against(),
                    dissent: vote.dissent ?? "-",
                  })}
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      {rejecting && (
        <div className="flex min-w-72 flex-col gap-1.5">
          <Label htmlFor={`reason-${application.id}`}>
            {m.decide_reason_label()}
          </Label>
          <NativeSelect
            id={`reason-${application.id}`}
            value={reason}
            onChange={(event) => setReason(event.target.value)}
          >
            {reasons.map((r) => (
              <NativeSelectOption key={r.code} value={r.code}>
                {reasonLabel(reasons, r.code)} ({r.rule_ref})
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </div>
      )}

      {decide.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(decide.error)}
        </p>
      )}
      <div>
        <Button
          type="submit"
          data-testid="decide-submit"
          disabled={decide.isPending || tally?.outcome == null}
        >
          {m.decide_submit()}
        </Button>
      </div>
    </form>
  )
}
