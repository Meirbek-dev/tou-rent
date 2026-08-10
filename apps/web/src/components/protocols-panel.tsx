import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { Button } from "@/components/ui/button"
import { problemMessage } from "@/lib/auth"
import { formatDateTime } from "@/lib/format"
import { publishProtocol, tenderProtocolsQuery } from "@/lib/publications"

/**
 * Протоколы тендера и их публикация (FR-702, FR-1402, INV-076): публичный
 * доступ длится шесть месяцев, дальше протокол снимается джобом и остается
 * в досье. Гость видит только опубликованные - их фильтрует сервер.
 */
export function ProtocolsPanel({
  tenderId,
  canPublish = false,
}: {
  tenderId: string
  canPublish?: boolean
}) {
  const queryClient = useQueryClient()
  const { data: protocols } = useQuery(tenderProtocolsQuery(tenderId))

  const publish = useMutation({
    mutationFn: (protocolId: string) => publishProtocol(protocolId),
    onSuccess: () =>
      queryClient.invalidateQueries({
        queryKey: tenderProtocolsQuery(tenderId).queryKey,
      }),
  })

  if (protocols === undefined || protocols.length === 0) return null

  return (
    <section
      aria-labelledby="protocols"
      className="flex flex-col gap-3"
      data-testid="protocols-panel"
    >
      <h3 id="protocols" className="font-heading text-lg font-semibold">
        {m.protocols_title()}
      </h3>
      <ul className="flex flex-col gap-2 text-sm">
        {protocols.map((protocol) => (
          <li
            key={protocol.id}
            className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-lg border p-3"
          >
            <span className="font-medium">
              {protocolKindLabel(protocol.kind)}
              {protocol.number != null && ` №${protocol.number}`}
            </span>
            <span className="text-muted-foreground" suppressHydrationWarning>
              {formatDateTime(protocol.generated_at)}
            </span>
            <span
              className={
                protocol.is_public ? "text-foreground" : "text-muted-foreground"
              }
              suppressHydrationWarning
            >
              {protocol.unpublished_at != null
                ? m.protocols_unpublished()
                : protocol.published_at != null
                  ? m.protocols_public_until({
                      date: formatDateTime(protocol.unpublish_at) ?? "-",
                    })
                  : m.protocols_not_published()}
            </span>
            {protocol.has_pdf && (
              <a
                href={`/api/v1/protocols/${protocol.id}/pdf`}
                className="underline-offset-4 hover:underline"
              >
                {m.protocols_pdf()}
              </a>
            )}
            {canPublish && protocol.published_at == null && (
              <Button
                variant="outline"
                size="sm"
                data-testid="publish-protocol"
                disabled={publish.isPending}
                onClick={() => publish.mutate(protocol.id)}
              >
                {m.protocols_publish()}
              </Button>
            )}
          </li>
        ))}
      </ul>
      {publish.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(publish.error)}
        </p>
      )}
    </section>
  )
}

/** Вид протокола (п. 55, 73–74, 82, 117). */
export function protocolKindLabel(kind: string): string {
  switch (kind) {
    case "admission":
      return m.protocol_kind_admission()
    case "results":
      return m.protocol_kind_results()
    case "failed":
      return m.protocol_kind_failed()
    case "winner2":
      return m.protocol_kind_winner2()
    default:
      return kind
  }
}
