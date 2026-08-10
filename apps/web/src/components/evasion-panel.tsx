import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { Button } from "@/components/ui/button"
import { problemMessage } from "@/lib/auth"
import { generateWinner2Protocol, tenderEvasionsQuery } from "@/lib/evasion"
import { formatDateTime } from "@/lib/format"

/**
 * Уклонение от подписания договора (FR-903, п. 116–118): кто уклонился и по
 * какому основанию, протокол о победителе № 2 за 5 рабочих дней и уведомление
 * участника № 2 - оно уходит вместе с протоколом.
 */
export function EvasionPanel({
  tenderId,
  canGenerateProtocol,
}: {
  tenderId: string
  canGenerateProtocol: boolean
}) {
  const queryClient = useQueryClient()
  const { data: evasions } = useQuery(tenderEvasionsQuery(tenderId))

  const protocol = useMutation({
    mutationFn: () => generateWinner2Protocol(tenderId),
    onSuccess: () =>
      queryClient.invalidateQueries({
        queryKey: tenderEvasionsQuery(tenderId).queryKey,
      }),
  })

  if (evasions === undefined || evasions.length === 0) return null

  return (
    <section
      aria-labelledby="evasion"
      className="flex flex-col gap-3 rounded-lg border p-4"
      data-testid="evasion-panel"
    >
      <h3 id="evasion" className="font-heading text-lg font-semibold">
        {m.evasion_title()}
      </h3>
      <ul className="flex flex-col gap-1 text-sm" data-testid="evasions">
        {evasions.map((evasion) => (
          <li key={evasion.id} className="flex flex-wrap items-center gap-x-3">
            <span className="font-medium">{evasion.user_name}</span>
            <span className="text-muted-foreground">
              {evasion.place_title_ru}
            </span>
            <span>
              {evasion.ground_label}{" "}
              <span className="text-muted-foreground">
                ({evasion.ground_rule_ref})
              </span>
            </span>
            <span className="text-muted-foreground" suppressHydrationWarning>
              {formatDateTime(evasion.declared_at)}
            </span>
          </li>
        ))}
      </ul>
      <p className="text-sm text-muted-foreground">{m.evasion_consequence()}</p>

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
    </section>
  )
}
