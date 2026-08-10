import { useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import {
  useMutation,
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { MyDeadlines } from "@/components/my-deadlines"
import { DossierPanel } from "@/components/dossier-panel"
import { InvestmentContracts } from "@/components/investment-contracts"
import { LandBoardPanel } from "@/components/land-panels"
import { SpecialProgress } from "@/components/special-progress"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { Textarea } from "@/components/ui/textarea"
import { api } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import { formatDateTime } from "@/lib/format"
import {
  investmentAttachmentsQuery,
  investmentContractsQuery,
} from "@/lib/investment"
import {
  decisionLabel,
  pendingSpecialRequestsQuery,
  specialCompetitionQuery,
  specialStatusLabel,
} from "@/lib/special"

import type { SpecialRequest } from "@/lib/special"

// FR-1202 (п. 90): Правление решает по заявкам, вынесенным на рассмотрение
// заключением подразделения. Без заключения решения нет (INV-090) - такие
// заявки показываются, но кнопки решения у них не появляется.
export const Route = createFileRoute("/app/board/")({
  loader: async ({ context }) => {
    await Promise.all([
      context.queryClient.ensureQueryData(pendingSpecialRequestsQuery),
      context.queryClient.ensureQueryData(investmentContractsQuery),
      context.queryClient.ensureQueryData(investmentAttachmentsQuery),
    ])
  },
  component: BoardHome,
})

function BoardHome() {
  const { data: requests } = useSuspenseQuery(pendingSpecialRequestsQuery)

  return (
    <div className="flex flex-col gap-6">
      <MyDeadlines />

      <section aria-labelledby="board-requests">
        <h2
          id="board-requests"
          className="mb-3 font-heading text-lg font-semibold"
        >
          {m.board_requests_title()}
        </h2>
        {requests.length === 0 ? (
          <p className="text-muted-foreground">{m.board_requests_empty()}</p>
        ) : (
          <ul className="flex flex-col gap-4">
            {requests.map((request) => (
              <li key={request.id}>
                <RequestCard request={request} />
              </li>
            ))}
          </ul>
        )}
      </section>

      {/* FR-1204 (п. 93): продление инвестиционного договора - за Правлением */}
      <InvestmentContracts roles={["board"]} />

      {/* FR-1801 (п. 106): решения по заявкам на земельные участки */}
      <LandBoardPanel />
    </div>
  )
}

function RequestCard({ request }: { request: SpecialRequest }) {
  const queryClient = useQueryClient()
  const [decision, setDecision] = useState("grant")
  const [rationale, setRationale] = useState("")
  // INV-086 (п. 86, 97): конкуренция закрывает часть решений - перечень
  // доступных считает сервер, форма показывает только их
  const { data: competition } = useQuery(specialCompetitionQuery(request.id))
  const permitted = competition?.permitted_decisions ?? [
    "grant",
    "refuse",
    "redirect",
  ]

  const decide = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST(
        "/api/v1/special-requests/{id}/decision",
        {
          params: { path: { id: request.id } },
          body: { decision, rationale },
        }
      )
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("decision failed")
      }
      return data
    },
    onSuccess: async () => {
      setRationale("")
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: pendingSpecialRequestsQuery.queryKey,
        }),
        queryClient.invalidateQueries({
          queryKey: ["special-progress", request.id],
        }),
        queryClient.invalidateQueries({
          queryKey: ["special-competition", request.id],
        }),
        // Решение попадает в досье триггером БД - состав перечитывается
        queryClient.invalidateQueries({
          queryKey: ["dossier", "special-request", request.id],
        }),
      ])
    },
  })

  // Решение возможно только по заявке с заключением (INV-090): в это
  // состояние заявку переводит само заключение подразделения
  const awaitsDecision = request.status === "under_review"

  return (
    <article className="flex flex-col gap-4 rounded-lg border p-4">
      <header className="flex flex-col gap-2">
        <div className="flex flex-wrap items-center gap-3">
          <span className="rounded-md border px-2 py-0.5 text-sm">
            {specialStatusLabel(request.status)}
          </span>
          <span className="text-sm text-muted-foreground">
            {request.category_label} ({request.category_rule_ref})
          </span>
          <span
            className="text-sm text-muted-foreground"
            suppressHydrationWarning
          >
            {m.application_submitted_at()}:{" "}
            {formatDateTime(request.submitted_at)}
          </span>
        </div>
        <p className="font-medium">
          {m.special_card_title({ id: request.id.slice(0, 8) })}
        </p>
        <p className="text-sm">{request.purpose}</p>
        {request.object_name != null && (
          <p className="text-sm text-muted-foreground">
            {m.special_object_label()}: {request.object_name}
          </p>
        )}
      </header>

      {competition !== undefined && competition.rivals > 0 && (
        <p className="rounded-lg border border-dashed p-3 text-sm">
          {competition.rule === "redirect"
            ? m.special_competition_redirect({
                total: competition.rivals + 1,
              })
            : m.special_competition_amounts({
                rivals: competition.rivals,
                best: competition.best_rival_amount ?? "-",
              })}
          {competition.rule === "highest_amount" &&
            competition.amounts_comparable &&
            ` ${m.special_competition_comparable()}`}
        </p>
      )}

      <SpecialProgress requestId={request.id} />

      {/* FR-1206 (п. 97): досье решения - доказательная база Правления */}
      <DossierPanel subject={{ kind: "special-request", id: request.id }} />

      {awaitsDecision ? (
        <form
          className="flex flex-col gap-3 border-t pt-4"
          onSubmit={(event) => {
            event.preventDefault()
            decide.mutate()
          }}
        >
          <div className="flex flex-wrap gap-3">
            <div className="flex min-w-64 flex-col gap-1.5">
              <Label htmlFor={`decision-${request.id}`}>
                {m.special_decision_label()}
              </Label>
              <NativeSelect
                id={`decision-${request.id}`}
                value={permitted.includes(decision) ? decision : permitted[0]}
                onChange={(event) => setDecision(event.target.value)}
              >
                {permitted.map((code) => (
                  <NativeSelectOption key={code} value={code}>
                    {decisionLabel(code)}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
            </div>
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor={`rationale-${request.id}`}>
              {m.special_rationale_label()}
            </Label>
            <Textarea
              id={`rationale-${request.id}`}
              required
              rows={3}
              value={rationale}
              onChange={(event) => setRationale(event.target.value)}
            />
            <p className="text-sm text-muted-foreground">
              {m.special_rationale_hint()}
            </p>
          </div>
          {decide.isError && (
            <p role="alert" className="text-sm text-destructive">
              {problemMessage(decide.error)}
            </p>
          )}
          <div>
            <Button
              type="submit"
              data-testid="special-decide-submit"
              disabled={decide.isPending}
            >
              {m.special_decide_submit()}
            </Button>
          </div>
        </form>
      ) : (
        <p className="border-t pt-4 text-sm text-muted-foreground">
          {m.special_awaits_review()}
        </p>
      )}
    </article>
  )
}
