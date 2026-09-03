import { useState } from "react"
import { Link, createFileRoute } from "@tanstack/react-router"
import {
  useMutation,
  useQueries,
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { GavelIcon, ScrollTextIcon } from "lucide-react"
import { m } from "#/paraglide/messages"
import { ConfirmAction } from "@/components/confirm-action"
import { EmptyState } from "@/components/empty-state"
import { PageHeader } from "@/components/page-header"
import { Panel } from "@/components/panel"
import { StatCard } from "@/components/stat-card"
import { DossierPanel } from "@/components/dossier-panel"
import { InvestmentContracts } from "@/components/investment-contracts"
import { LandBoardPanel } from "@/components/land-panels"
import { SpecialProgress } from "@/components/special-progress"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { Textarea } from "@/components/ui/textarea"
import { useNowMs } from "@/hooks/use-now"
import { api } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import { DECIDED_STATUSES, mergeDecided } from "@/lib/board"
import { formatDateTime, formatTenge } from "@/lib/format"
import { formatRelative } from "@/lib/relative-time"
import { notifyError, notifySuccess } from "@/lib/toast"
import {
  investmentAttachmentsQuery,
  investmentContractsQuery,
} from "@/lib/investment"
import {
  decisionLabel,
  pendingSpecialRequestsQuery,
  requestCategoryLabel,
  specialCompetitionQuery,
  specialRequestsQuery,
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
  head: () => ({ meta: [{ title: `${m.cabinet_board()} - ToU Rent` }] }),
  component: BoardHome,
})

function BoardHome() {
  const { data: requests } = useSuspenseQuery(pendingSpecialRequestsQuery)

  // Ждущие решения - те, у которых есть заключение подразделения (INV-090):
  // остальные показываются, но хода за Правлением пока нет
  const awaiting = requests.filter(
    (request) => request.status === "under_review"
  )

  return (
    <div className="flex flex-col gap-6">
      {/* Имя кабинета - заголовок страницы: из макета он ушел вместе
          с прежней шапкой (каркас называет кабинет группой боковой
          навигации) */}
      <PageHeader
        title={m.cabinet_board()}
        description={m.board_dash_subtitle()}
      />

      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <StatCard
          label={m.board_stat_awaiting()}
          value={awaiting.length}
          urgency={awaiting.length > 0 ? "soon" : "normal"}
        />
        <StatCard label={m.board_stat_pending()} value={requests.length} />
      </div>

      <Panel title={m.board_requests_title()} titleAs="h2">
        {requests.length === 0 ? (
          <EmptyState
            icon={ScrollTextIcon}
            title={m.board_requests_empty()}
            description={m.board_requests_empty_hint()}
          />
        ) : (
          <ul className="flex flex-col gap-4">
            {requests.map((request) => (
              <li key={request.id}>
                <RequestCard request={request} />
              </li>
            ))}
          </ul>
        )}
      </Panel>

      {/* Принятое решение уходило из очереди и больше нигде не открывалось:
          ни перечитать формулировку, ни дойти до публикации (п. 90) */}
      <DecidedPanel />

      {/* FR-1204 (п. 93): продление инвестиционного договора - за Правлением */}
      <InvestmentContracts roles={["board"]} />

      {/* FR-1801 (п. 106): решения по заявкам на земельные участки */}
      <LandBoardPanel />
    </div>
  )
}

function RequestCard({ request }: { request: SpecialRequest }) {
  const queryClient = useQueryClient()
  const nowMs = useNowMs()
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
      notifySuccess(m.board_decision_toast())
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
        // Решенная заявка переезжает из очереди в перечень решений
        ...DECIDED_STATUSES.map((status) =>
          queryClient.invalidateQueries({
            queryKey: specialRequestsQuery(status).queryKey,
          })
        ),
      ])
    },
    onError: (error: unknown) => notifyError(problemMessage(error)),
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
            {requestCategoryLabel(request)}
          </span>
          <span
            className="text-sm text-muted-foreground tabular-nums"
            suppressHydrationWarning
          >
            {m.application_submitted_at()}:{" "}
            {formatDateTime(request.submitted_at)}
          </span>
          {/* Сколько заявка уже лежит: подсчет от «сейчас» появляется только
              после монтирования, иначе разметка сервера разошлась бы с
              браузерной (NFR-03). Никаких сроков сверх этого payload не
              несет - и выдумывать их здесь нельзя */}
          {nowMs !== null && (
            <span className="text-sm text-muted-foreground">
              {formatRelative(request.submitted_at, nowMs)}
            </span>
          )}
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
            : /* Сумма приходила строкой Decimal и печаталась как есть -
                 «1000000.00 ₸» мимо разрядов и мимо локали. Знак валюты
                 ставит `formatTenge`, поэтому у строки свой ключ: в
                 `special_competition_amounts` ₸ вписан прямо в текст, и
                 вместе с форматером выходило «2 500 000,00 ₸ ₸» */
              m.board_competition_amounts({
                rivals: competition.rivals,
                best:
                  competition.best_rival_amount == null
                    ? "-"
                    : formatTenge(competition.best_rival_amount),
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
            // Решение принимается через подтверждение: отправки формы
            // нажатием Enter здесь быть не должно
            event.preventDefault()
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
            {/* Решение Правления публикуется и назад не отыгрывается
                (п. 90, 97) - оно проходит через подтверждение */}
            <ConfirmAction
              title={m.board_decide_confirm_title()}
              description={m.board_decide_confirm_description({
                decision: decisionLabel(
                  permitted.includes(decision) ? decision : (permitted[0] ?? "")
                ),
              })}
              confirmLabel={m.special_decide_submit()}
              variant="default"
              disabled={decide.isPending || rationale.trim() === ""}
              onConfirm={() => decide.mutate()}
              trigger={
                <Button type="button" data-testid="special-decide-submit">
                  {m.special_decide_submit()}
                </Button>
              }
            />
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

/**
 * Принятые решения (п. 90).
 *
 * Контракт фильтрует список по одному состоянию за запрос, поэтому три
 * выборки складываются в один перечень. Связи «решение - публикация» в
 * контракте нет (`PublicRecordDto` не несет `source_id`), поэтому ссылка
 * ведет в реестр публикаций целиком, а сам текст решения открывается
 * печатной формой.
 */
function DecidedPanel() {
  const queries = useQueries({
    queries: DECIDED_STATUSES.map((status) => specialRequestsQuery(status)),
  })
  const decided = mergeDecided(queries.map((query) => query.data))

  return (
    <Panel title={m.board_decided_title()} titleAs="h2">
      {decided.length === 0 ? (
        <EmptyState
          icon={GavelIcon}
          title={m.board_decided_empty()}
          description={m.board_decided_empty_hint()}
        />
      ) : (
        <ul className="flex flex-col gap-3" data-testid="board-decided">
          {decided.map((request) => (
            <li
              key={request.id}
              className="flex flex-col gap-2 rounded-lg border p-4"
            >
              <div className="flex flex-wrap items-center gap-3">
                <span className="rounded-md border px-2 py-0.5 text-sm">
                  {specialStatusLabel(request.status)}
                </span>
                <span className="text-sm text-muted-foreground">
                  {requestCategoryLabel(request)}
                </span>
                <span
                  className="text-sm text-muted-foreground tabular-nums"
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
              <div className="flex flex-wrap gap-4 text-sm">
                <a
                  className="underline underline-offset-4"
                  href={`/api/v1/special-requests/${request.id}/decision.pdf`}
                  target="_blank"
                  rel="noreferrer"
                  data-testid={`decision-pdf-${request.id}`}
                >
                  {m.public_records_document()}
                </a>
                <Link
                  className="underline underline-offset-4"
                  to="/special-orders"
                >
                  {m.public_records_title()}
                </Link>
              </div>
            </li>
          ))}
        </ul>
      )}
    </Panel>
  )
}
