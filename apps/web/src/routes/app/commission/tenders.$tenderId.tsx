import { useState } from "react"
import { Link, createFileRoute, notFound } from "@tanstack/react-router"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { ApplicationStatusBadge } from "@/components/application-status-badge"
import { TenderStatusBadge } from "@/components/tender-status-badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { api, tenderQuery } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import { applicationVotesQuery } from "@/lib/commission"
import { formatDateTime, formatTenge } from "@/lib/format"
import { meetingQuery, tenderApplicationsQuery } from "@/lib/participant"

import type { ApplicationDto } from "@/lib/participant"

// Заседание глазами члена комиссии (FR-1103–1104): декларация конфликта
// интересов до заседания, личный голос «за»/«против» с особым мнением.
// Материалы отведенного лота ему не приходят - их закрывает RLS (п. 15).
export const Route = createFileRoute("/app/commission/tenders/$tenderId")({
  loader: async ({ context, params }) => {
    const tender = await context.queryClient.ensureQueryData(
      tenderQuery(params.tenderId)
    )
    if (tender === null) throw notFound()
    await Promise.all([
      context.queryClient.ensureQueryData(meetingQuery(params.tenderId)),
      context.queryClient.ensureQueryData(
        tenderApplicationsQuery(params.tenderId)
      ),
    ])
  },
  component: CommissionTenderPage,
})

function CommissionTenderPage() {
  const { tenderId } = Route.useParams()
  const queryClient = useQueryClient()
  const { data: tender } = useSuspenseQuery(tenderQuery(tenderId))
  const { data: meeting } = useSuspenseQuery(meetingQuery(tenderId))
  const { data: applications } = useSuspenseQuery(
    tenderApplicationsQuery(tenderId)
  )
  const [details, setDetails] = useState("")

  if (tender === null) throw notFound()

  const refresh = async () => {
    await queryClient.invalidateQueries({
      queryKey: meetingQuery(tenderId).queryKey,
    })
  }

  // FR-1104: декларация подается лично и до заседания
  const declare = useMutation({
    mutationFn: async (hasConflict: boolean) => {
      const { error } = await api.POST(
        "/api/v1/tenders/{id}/conflict-of-interest",
        {
          params: { path: { id: tenderId } },
          body: { has_conflict: hasConflict, details: details || null },
        }
      )
      if (error !== undefined) throw error
    },
    onSuccess: refresh,
  })

  const votable = applications.filter(
    (application) =>
      application.status === "submitted" ||
      application.status === "fee_confirmed"
  )

  return (
    <div className="flex flex-col gap-8">
      <nav>
        <Link
          to="/app/commission"
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
      </header>

      {meeting !== null && (
        <section aria-labelledby="meeting" className="flex flex-col gap-3">
          <h3 id="meeting" className="font-heading text-lg font-semibold">
            {m.meeting_title()}
          </h3>
          <dl className="grid grid-cols-1 gap-3 rounded-lg border p-4 sm:grid-cols-3">
            <div className="flex flex-col gap-0.5">
              <dt className="text-sm text-muted-foreground">
                {m.meeting_commission()}
              </dt>
              <dd className="font-medium">{meeting.commission_name}</dd>
            </div>
            <div className="flex flex-col gap-0.5">
              <dt className="text-sm text-muted-foreground">
                {m.meeting_opened_at()}
              </dt>
              <dd className="font-medium" suppressHydrationWarning>
                {formatDateTime(meeting.opened_at) ?? m.meeting_not_opened()}
              </dd>
            </div>
            <div className="flex flex-col gap-0.5">
              <dt className="text-sm text-muted-foreground">
                {m.meeting_quorum()}
              </dt>
              <dd className="font-medium">
                {meeting.quorum_present == null
                  ? "-"
                  : m.meeting_quorum_value({
                      present: meeting.quorum_present,
                      required: meeting.quorum_required ?? 0,
                    })}
              </dd>
            </div>
          </dl>
        </section>
      )}

      <section aria-labelledby="coi" className="flex flex-col gap-3">
        <h3 id="coi" className="font-heading text-lg font-semibold">
          {m.coi_title()}
        </h3>
        <p className="text-sm text-muted-foreground">{m.coi_hint()}</p>
        <div className="flex flex-col gap-3 rounded-lg border p-4">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="coi-details">{m.coi_details_label()}</Label>
            <Input
              id="coi-details"
              value={details}
              onChange={(event) => setDetails(event.target.value)}
            />
          </div>
          <div className="flex flex-wrap gap-3">
            <Button
              data-testid="coi-none"
              onClick={() => declare.mutate(false)}
              disabled={declare.isPending}
            >
              {m.coi_declare_none()}
            </Button>
            <Button
              variant="outline"
              data-testid="coi-conflict"
              onClick={() => declare.mutate(true)}
              disabled={declare.isPending}
            >
              {m.coi_declare_conflict()}
            </Button>
          </div>
          {declare.isSuccess && (
            <p className="text-sm text-muted-foreground">{m.coi_recorded()}</p>
          )}
          {declare.isError && (
            <p role="alert" className="text-sm text-destructive">
              {problemMessage(declare.error)}
            </p>
          )}
        </div>
      </section>

      <section aria-labelledby="voting" className="flex flex-col gap-4">
        <h3 id="voting" className="font-heading text-lg font-semibold">
          {m.voting_title()}
        </h3>
        {votable.length === 0 ? (
          <p className="text-muted-foreground">{m.voting_empty()}</p>
        ) : (
          votable.map((application) => (
            <VoteForm key={application.id} application={application} />
          ))
        )}
      </section>
    </div>
  )
}

function VoteForm({ application }: { application: ApplicationDto }) {
  const queryClient = useQueryClient()
  const { data: votes } = useSuspenseQuery(
    applicationVotesQuery(application.id)
  )
  const [dissent, setDissent] = useState("")

  const cast = useMutation({
    mutationFn: async (value: "for" | "against") => {
      const { data, error } = await api.POST("/api/v1/applications/{id}/vote", {
        params: { path: { id: application.id } },
        body: { value, dissent: dissent || null },
      })
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("vote failed")
      }
      return data
    },
    onSuccess: () =>
      queryClient.invalidateQueries({
        queryKey: applicationVotesQuery(application.id).queryKey,
      }),
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
    <div
      data-testid={`vote-form-${application.id}`}
      className="flex flex-col gap-4 rounded-lg border p-4"
    >
      <div className="flex flex-wrap items-center gap-3">
        <span className="font-medium">{applicantName}</span>
        <ApplicationStatusBadge status={application.status} />
        <span className="text-sm text-muted-foreground">
          {application.price_amount != null
            ? formatTenge(application.price_amount)
            : m.application_price_sealed()}
        </span>
      </div>

      {votes !== null && (
        <p className="text-sm text-muted-foreground">
          {m.voting_tally({
            for: votes.tally.votes_for,
            against: votes.tally.votes_against,
            eligible: votes.tally.eligible,
          })}
        </p>
      )}

      <div className="flex flex-col gap-1.5">
        <Label htmlFor={`dissent-${application.id}`}>
          {m.voting_dissent_label()}
        </Label>
        <Input
          id={`dissent-${application.id}`}
          value={dissent}
          onChange={(event) => setDissent(event.target.value)}
        />
      </div>

      <div className="flex flex-wrap gap-3">
        <Button
          data-testid={`vote-for-${application.id}`}
          onClick={() => cast.mutate("for")}
          disabled={cast.isPending}
        >
          {m.vote_for()}
        </Button>
        <Button
          variant="outline"
          data-testid={`vote-against-${application.id}`}
          onClick={() => cast.mutate("against")}
          disabled={cast.isPending}
        >
          {m.vote_against()}
        </Button>
      </div>

      {cast.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(cast.error)}
        </p>
      )}
    </div>
  )
}
