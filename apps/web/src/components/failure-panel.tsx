import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { m } from "#/paraglide/messages"
import { Panel } from "@/components/panel"
import { QueryBoundary } from "@/components/query-boundary"
import { Button } from "@/components/ui/button"
import { problemMessage } from "@/lib/auth"
import {
  declareFailed,
  failureStateQuery,
  generateFailedProtocol,
  repeatTender,
} from "@/lib/failure"
import { notifySuccess } from "@/lib/toast"

import type { FailureStateDto } from "@/lib/failure"

/**
 * Несостоявшийся тендер (FR-801–802): основание п. 81 система выводит сама
 * и показывает до признания, следствие п. 82–83 - тоже. Секретарь признает
 * и оформляет протокол, организатор объявляет повторный тендер.
 *
 * Раздел условный - он появляется только там, где основание наступило или
 * тендер уже признан. Поэтому заглушки у него нет: на обычном тендере она
 * мигала бы впустую. А вот отказ запроса виден - иначе исчезнувшая панель
 * читалась бы как «основание не наступило».
 */
export function FailurePanel({
  tenderId,
  canDeclare,
  canRepeat,
  onChanged,
}: {
  tenderId: string
  canDeclare: boolean
  canRepeat: boolean
  onChanged: () => Promise<void>
}) {
  const queryClient = useQueryClient()
  const failure = useQuery(failureStateQuery(tenderId))

  const refresh = async () => {
    await queryClient.invalidateQueries({
      queryKey: failureStateQuery(tenderId).queryKey,
    })
    await onChanged()
  }

  const declare = useMutation({
    mutationFn: () => declareFailed(tenderId),
    onSuccess: async () => {
      notifySuccess(m.failure_declared())
      await refresh()
    },
  })
  const protocol = useMutation({
    mutationFn: () => generateFailedProtocol(tenderId),
    onSuccess: async () => {
      notifySuccess(m.failure_protocol_created())
      await refresh()
    },
  })
  const repeat = useMutation({
    mutationFn: () => repeatTender(tenderId),
    onSuccess: async () => {
      notifySuccess(m.failure_repeat_created())
      await refresh()
    },
  })

  return (
    <QueryBoundary query={failure} skeleton={<></>}>
      {(state) =>
        // Панель нужна там, где основание наступило или уже признано
        state.ground == null && !state.failed ? null : (
          <div data-testid="failure-panel">
            <Panel
              title={m.failure_title()}
              titleAs="h3"
              contentClassName="flex flex-col gap-3"
            >
              <FailureFacts state={state} />

              <div className="flex flex-wrap gap-3">
                {canDeclare && !state.failed && (
                  <Button
                    variant="outline"
                    data-testid="declare-failed"
                    disabled={declare.isPending}
                    onClick={() => declare.mutate()}
                  >
                    {m.failure_declare()}
                  </Button>
                )}
                {canDeclare && state.failed && (
                  <>
                    <Button
                      variant="outline"
                      data-testid="failed-protocol"
                      disabled={protocol.isPending}
                      onClick={() => protocol.mutate()}
                    >
                      {m.failure_protocol()}
                    </Button>
                    <a
                      href={`/api/v1/tenders/${tenderId}/failed-protocol.pdf`}
                      className="text-sm underline-offset-4 hover:underline"
                    >
                      {m.failure_protocol_pdf()}
                    </a>
                  </>
                )}
                {canRepeat &&
                  state.failed &&
                  state.consequence !== "board_referral" && (
                    <Button
                      variant="outline"
                      data-testid="repeat-tender"
                      disabled={repeat.isPending}
                      onClick={() => repeat.mutate()}
                    >
                      {m.failure_repeat()}
                    </Button>
                  )}
              </div>

              {(declare.isError || protocol.isError || repeat.isError) && (
                <p role="alert" className="text-sm text-destructive">
                  {problemMessage(
                    declare.error ?? protocol.error ?? repeat.error
                  )}
                </p>
              )}
              {repeat.isSuccess && (
                <p className="text-sm text-muted-foreground">
                  {m.failure_repeat_created()}
                </p>
              )}
            </Panel>
          </div>
        )
      }
    </QueryBoundary>
  )
}

/** Основание, следствие и счетчики п. 81–83 - то, из чего выводится решение. */
function FailureFacts({ state }: { state: FailureStateDto }) {
  return (
    <dl className="grid grid-cols-1 gap-x-6 gap-y-1 text-sm sm:grid-cols-2">
      <div className="flex gap-2">
        <dt className="text-muted-foreground">{m.failure_ground_label()}:</dt>
        <dd data-testid="failure-ground">
          {groundLabel(state.ground)}
          {state.ground_rule_ref != null && ` (${state.ground_rule_ref})`}
        </dd>
      </div>
      <div className="flex gap-2">
        <dt className="text-muted-foreground">
          {m.failure_consequence_label()}:
        </dt>
        <dd data-testid="failure-consequence">
          {consequenceLabel(state.consequence)}
        </dd>
      </div>
      <div className="flex gap-2">
        <dt className="text-muted-foreground">
          {m.failure_applications_label()}:
        </dt>
        <dd>{state.applications}</dd>
      </div>
      <div className="flex gap-2">
        <dt className="text-muted-foreground">{m.failure_admitted_label()}:</dt>
        <dd>{state.admitted}</dd>
      </div>
    </dl>
  )
}

function groundLabel(ground: string | null | undefined): string {
  switch (ground) {
    case "no_applications":
      return m.failure_ground_no_applications()
    case "single_application":
      return m.failure_ground_single_application()
    case "fewer_than_two_admitted":
      return m.failure_ground_fewer_admitted()
    case "winners_evaded":
      return m.failure_ground_winners_evaded()
    default:
      return m.failure_ground_none()
  }
}

function consequenceLabel(consequence: string | null | undefined): string {
  switch (consequence) {
    case "single_source":
      return m.failure_consequence_single_source()
    case "board_referral":
      return m.failure_consequence_board()
    case "repeat":
      return m.failure_consequence_repeat()
    default:
      return "-"
  }
}
