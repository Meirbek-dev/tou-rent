import { useState } from "react"
import { Link, createFileRoute, notFound } from "@tanstack/react-router"
import {
  useMutation,
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { ApplicationStatusBadge } from "@/components/application-status-badge"
import { ConfirmAction } from "@/components/confirm-action"
import { PageHeader } from "@/components/page-header"
import { PageShell } from "@/components/page-shell"
import { Panel } from "@/components/panel"
import { QueryBoundary } from "@/components/query-boundary"
import { TenderStatusBadge } from "@/components/tender-status-badge"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Skeleton } from "@/components/ui/skeleton"
import { api, localizedTenderTitle, tenderQuery } from "@/lib/api"
import { meQuery, problemMessage } from "@/lib/auth"
import {
  activeCommissionQuery,
  applicantName,
  applicationVotesQuery,
  commissionSeat,
  ownVote,
} from "@/lib/commission"
import { formatDateTime, formatTenge } from "@/lib/format"
import { meetingQuery, tenderApplicationsQuery } from "@/lib/participant"
import { notifyError, notifySuccess } from "@/lib/toast"
import { ArrowLeftIcon } from "lucide-react"

import type { CommissionMember, VoteValue } from "@/lib/commission"
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
      // Право голоса определяется составом (п. 9–11), а не ролью кабинета:
      // резервный член голосует только после замещения выбывшего
      context.queryClient.ensureQueryData(activeCommissionQuery),
      context.queryClient.ensureQueryData(meQuery),
    ])
  },
  head: () => ({ meta: [{ title: `${m.meeting_title()} - ToU Rent` }] }),
  component: CommissionTenderPage,
})

/** Подпись голоса: союз приходит из контракта, забыть ветку не даст компилятор. */
const VOTE_LABELS: Record<VoteValue, () => string> = {
  for: m.vote_for,
  against: m.vote_against,
}

function CommissionTenderPage() {
  const { tenderId } = Route.useParams()
  const queryClient = useQueryClient()
  const { data: tender } = useSuspenseQuery(tenderQuery(tenderId))
  const { data: meeting } = useSuspenseQuery(meetingQuery(tenderId))
  const { data: applications } = useSuspenseQuery(
    tenderApplicationsQuery(tenderId)
  )
  const { data: commission } = useSuspenseQuery(activeCommissionQuery)
  const { data: me } = useSuspenseQuery(meQuery)
  const [details, setDetails] = useState("")

  if (tender === null) throw notFound()

  // До вскрытия конвертов заявителей не существует: цены запечатаны, решений
  // нет, а голос до открытия заседания сервер все равно не примет (п. 12).
  // Кнопки «За/Против» стояли живыми, и отказ приходил уже после
  // подтверждения «необратимого» голоса
  const sealed = tender.opened_at == null || meeting?.opened_at == null
  const seat = commissionSeat(commission, me?.id)

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
    onSuccess: async () => {
      notifySuccess(m.coi_recorded())
      await refresh()
    },
  })

  const votable = applications.filter(
    (application) =>
      application.status === "submitted" ||
      application.status === "fee_confirmed"
  )

  return (
    <PageShell>
      <PageHeader
        breadcrumb={
          <Link
            to="/app/commission"
            className="inline-flex w-fit items-center gap-1.5 text-sm text-muted-foreground underline-offset-4 hover:underline"
          >
            <ArrowLeftIcon aria-hidden="true" className="size-4" />
            {m.back_to_cabinet()}
          </Link>
        }
        title={localizedTenderTitle(tender)}
        description={m.tender_card_title({ id: tender.id.slice(0, 8) })}
        badge={
          <TenderStatusBadge
            status={tender.status}
            deadline={tender.submission_deadline}
          />
        }
      />

      {meeting !== null && (
        <Panel title={m.meeting_title()}>
          <dl className="grid grid-cols-1 gap-3 sm:grid-cols-3">
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
              <dd className="font-medium tabular-nums">
                {meeting.quorum_present == null
                  ? "-"
                  : m.meeting_quorum_value({
                      present: meeting.quorum_present,
                      required: meeting.quorum_required ?? 0,
                    })}
              </dd>
            </div>
          </dl>
        </Panel>
      )}

      <Panel title={m.coi_title()} description={m.coi_hint()}>
        <div className="flex flex-col gap-3">
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
      </Panel>

      <section aria-labelledby="voting" className="flex flex-col gap-4">
        <h2 id="voting" className="font-heading text-lg font-semibold">
          {m.voting_title()}
        </h2>
        {sealed ? (
          <div
            data-testid="voting-sealed"
            className="flex flex-col gap-1 rounded-xl bg-muted/50 p-5 ring-1 ring-border"
          >
            <p className="font-medium">{m.commission_sealed_title()}</p>
            <p className="text-sm text-muted-foreground">
              {m.commission_sealed_hint()}
            </p>
          </div>
        ) : votable.length === 0 ? (
          <p className="text-muted-foreground">{m.voting_empty()}</p>
        ) : (
          votable.map((application) => (
            <VoteForm
              key={application.id}
              application={application}
              seat={seat}
            />
          ))
        )}
      </section>
    </PageShell>
  )
}

function VoteForm({
  application,
  seat,
}: {
  application: ApplicationDto
  /** Место голосующего в составе комиссии; `null` - он в составе не значится */
  seat: CommissionMember | null
}) {
  const queryClient = useQueryClient()
  const votes = useQuery(applicationVotesQuery(application.id))
  const [dissent, setDissent] = useState("")

  const cast = useMutation({
    mutationFn: async (value: VoteValue) => {
      const { data, error } = await api.POST("/api/v1/applications/{id}/vote", {
        params: { path: { id: application.id } },
        body: { value, dissent: dissent || null },
      })
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("vote failed")
      }
      return data
    },
    onSuccess: async () => {
      notifySuccess(m.commission_vote_cast_toast())
      await queryClient.invalidateQueries({
        queryKey: applicationVotesQuery(application.id).queryKey,
      })
    },
    onError: (error: unknown) => notifyError(problemMessage(error)),
  })

  const applicant = applicantName(application)
  const reserve = seat?.member_role === "reserve"
  // Голос - процессуальный акт и отзыву не подлежит (п. 13–14): поданный
  // виден, а вторая пара кнопок не рисуется. Сервер откажет правилом
  // `vote_already_cast`, но узнать об этом до щелчка - работа экрана
  const myVote = ownVote(votes.data, seat?.member_id)

  return (
    <div
      data-testid={`vote-form-${application.id}`}
      className="flex flex-col gap-4 rounded-lg border p-4"
    >
      <div className="flex flex-wrap items-center gap-3">
        <span className="font-medium">{applicant}</span>
        <ApplicationStatusBadge status={application.status} />
        <span className="text-sm text-muted-foreground tabular-nums">
          {application.price_amount != null
            ? formatTenge(application.price_amount)
            : m.application_price_sealed()}
        </span>
      </div>

      {/* Подсчет и уже поданные голоса грузятся отдельным запросом: до сих
          пор он подвешивал весь маршрут, а отказ уводил экран в границу
          ошибки целиком */}
      <QueryBoundary
        query={votes}
        skeleton={<Skeleton className="h-16 w-full rounded-lg" />}
      >
        {(data) =>
          data === null ? null : (
            <div className="flex flex-col gap-2">
              <p className="text-sm text-muted-foreground">
                {m.voting_tally({
                  for: data.tally.votes_for,
                  against: data.tally.votes_against,
                  eligible: data.tally.eligible,
                })}
              </p>
              {/* Причину ожидания сервер шлет готовой русской строкой -
                  наружу идет своя, переводимая (NFR-01) */}
              {data.tally.outcome == null && data.tally.pending != null && (
                <p className="text-sm text-muted-foreground">
                  {m.voting_pending()}
                </p>
              )}
              {/* Поданные голоса приходили в ответе и никуда не выводились:
                  член комиссии не видел ни своего голоса, ни чужих */}
              <div className="flex flex-col gap-1">
                <p className="text-sm font-medium">
                  {m.commission_votes_cast_title()}
                </p>
                {data.votes.length === 0 ? (
                  <p className="text-sm text-muted-foreground">
                    {m.commission_votes_none()}
                  </p>
                ) : (
                  <ul className="flex flex-col gap-1">
                    {data.votes.map((vote) => (
                      <li
                        key={vote.member_id}
                        className="flex flex-wrap items-center gap-2 text-sm"
                      >
                        <span>{vote.member_name}</span>
                        <Badge
                          variant={
                            vote.value === "for" ? "success" : "destructive"
                          }
                        >
                          {VOTE_LABELS[vote.value]()}
                        </Badge>
                        {vote.dissent != null && vote.dissent !== "" && (
                          <span className="text-muted-foreground">
                            {vote.dissent}
                          </span>
                        )}
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            </div>
          )
        }
      </QueryBoundary>

      {reserve ? (
        <p
          data-testid="vote-reserve-note"
          className="text-sm text-muted-foreground"
        >
          {m.commission_reserve_note()}
        </p>
      ) : myVote !== null ? (
        <p
          data-testid="vote-already-cast"
          className="text-sm text-muted-foreground"
        >
          {m.commission_vote_already_cast({
            value: VOTE_LABELS[myVote.value](),
          })}
        </p>
      ) : (
        <>
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

          {/* Голос члена комиссии - процессуальный акт: он заносится в
              протокол и отзыву не подлежит (п. 13–14). До сих пор он
              подавался первым щелчком, без единого вопроса */}
          <div className="flex flex-wrap gap-3">
            <ConfirmAction
              title={m.commission_vote_confirm_title()}
              description={m.commission_vote_confirm_description({
                vote: m.vote_for(),
                applicant,
              })}
              confirmLabel={m.vote_for()}
              variant="default"
              onConfirm={() => cast.mutate("for")}
              trigger={
                <Button
                  data-testid={`vote-for-${application.id}`}
                  disabled={cast.isPending}
                >
                  {m.vote_for()}
                </Button>
              }
            />
            <ConfirmAction
              title={m.commission_vote_confirm_title()}
              description={m.commission_vote_confirm_description({
                vote: m.vote_against(),
                applicant,
              })}
              confirmLabel={m.vote_against()}
              onConfirm={() => cast.mutate("against")}
              trigger={
                <Button
                  variant="outline"
                  data-testid={`vote-against-${application.id}`}
                  disabled={cast.isPending}
                >
                  {m.vote_against()}
                </Button>
              }
            />
          </div>
        </>
      )}

      {cast.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(cast.error)}
        </p>
      )}
    </div>
  )
}
