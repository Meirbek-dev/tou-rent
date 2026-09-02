import { useState } from "react"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"

import { m } from "#/paraglide/messages"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { api } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import { applicationVotesQuery } from "@/lib/commission"
import { formatTenge } from "@/lib/format"
import { reasonLabel } from "@/lib/participant"

import type { ApplicationDto, RejectionReason } from "@/lib/participant"

/**
 * Решение по заявке (FR-1103): секретарь фиксирует вердикт, который сервер
 * посчитал по голосам комиссии, и - при отклонении - основание п. 52.
 *
 * Вердикт здесь не выбирается: кнопка недоступна, пока подсчет не дал итога.
 * Форма переехала из файла маршрута вместе с разбиением экрана на вкладки.
 */
export function DecisionForm({
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
    <Card>
      <CardContent>
        {/* Имя заявителя стоит внутри самой формы: карточка решения
            опознается по нему - и глазами, и приемочным сценарием */}
        <form
          data-testid={`decide-form-${application.id}`}
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault()
            decide.mutate()
          }}
        >
          <div className="flex flex-wrap items-baseline justify-between gap-3">
            <h3 className="font-heading text-base font-medium">
              {applicantName}
            </h3>
            <span className="text-sm tabular-nums" suppressHydrationWarning>
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
                          vote.value === "for"
                            ? m.vote_for()
                            : m.vote_against(),
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
                    {reasonLabel(reasons, r.code)}
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
      </CardContent>
    </Card>
  )
}
