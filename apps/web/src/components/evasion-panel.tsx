import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { m } from "#/paraglide/messages"
import { Panel } from "@/components/panel"
import { QueryBoundary } from "@/components/query-boundary"
import { Button } from "@/components/ui/button"
import { problemMessage } from "@/lib/auth"
import { generateWinner2Protocol, tenderEvasionsQuery } from "@/lib/evasion"
import { formatDateTime } from "@/lib/format"
import { serverLabel } from "@/lib/server-label"
import { notifySuccess } from "@/lib/toast"

/**
 * Уклонение от подписания договора (FR-903, п. 116–118): кто уклонился и по
 * какому основанию, протокол о победителе № 2 за 5 рабочих дней и уведомление
 * участника № 2 - оно уходит вместе с протоколом.
 *
 * Раздел условный: у большинства тендеров уклонений нет, и говорить об этом
 * отдельной панелью не о чем. Поэтому пустой ответ и незавершенный запрос
 * рисуют пустоту (заглушка сдвинула бы карточку тендера на каждой отрисовке),
 * а вот отказ запроса виден: молчащая панель означала бы «уклонений нет»
 * там, где ответа просто не получили.
 */
export function EvasionPanel({
  tenderId,
  canGenerateProtocol,
}: {
  tenderId: string
  canGenerateProtocol: boolean
}) {
  const queryClient = useQueryClient()
  const evasions = useQuery(tenderEvasionsQuery(tenderId))

  const protocol = useMutation({
    mutationFn: () => generateWinner2Protocol(tenderId),
    onSuccess: async () => {
      notifySuccess(m.evasion_protocol_created())
      await queryClient.invalidateQueries({
        queryKey: tenderEvasionsQuery(tenderId).queryKey,
      })
    },
  })

  return (
    <QueryBoundary query={evasions} skeleton={<></>}>
      {(rows) =>
        rows.length === 0 ? null : (
          <div data-testid="evasion-panel">
            <Panel
              title={m.evasion_title()}
              titleAs="h3"
              contentClassName="flex flex-col gap-3"
            >
              <ul
                className="flex flex-col gap-1 text-sm"
                data-testid="evasions"
              >
                {rows.map((evasion) => (
                  <li
                    key={evasion.id}
                    className="flex flex-wrap items-center gap-x-3"
                  >
                    <span className="font-medium">{evasion.user_name}</span>
                    <span className="text-muted-foreground">
                      {serverLabel(evasion, "place_title")}
                    </span>
                    <span>{evasion.ground_label}</span>
                    <span
                      className="text-muted-foreground"
                      suppressHydrationWarning
                    >
                      {formatDateTime(evasion.declared_at)}
                    </span>
                  </li>
                ))}
              </ul>
              <p className="text-sm text-muted-foreground">
                {m.evasion_consequence()}
              </p>

              {canGenerateProtocol && (
                <div className="flex flex-wrap items-center gap-3">
                  <Button
                    variant="outline"
                    size="sm"
                    data-testid="winner2-protocol"
                    disabled={protocol.isPending}
                    onClick={() => protocol.mutate()}
                  >
                    {m.evasion_protocol()}
                  </Button>
                  <a
                    href={`/api/v1/tenders/${tenderId}/winner2-protocol.pdf`}
                    className="text-sm underline-offset-4 hover:underline"
                  >
                    {m.evasion_protocol_pdf()}
                  </a>
                </div>
              )}

              {protocol.isError && (
                <p role="alert" className="text-sm text-destructive">
                  {problemMessage(protocol.error)}
                </p>
              )}
            </Panel>
          </div>
        )
      }
    </QueryBoundary>
  )
}
