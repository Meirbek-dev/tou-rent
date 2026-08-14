import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { FileTextIcon } from "lucide-react"

import { m } from "#/paraglide/messages"
import { QueryBoundary } from "@/components/query-boundary"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { problemMessage } from "@/lib/auth"
import { formatDateTime } from "@/lib/format"
import { publishProtocol, tenderProtocolsQuery } from "@/lib/publications"
import { notifySuccess } from "@/lib/toast"

/**
 * Протоколы тендера и их публикация (FR-702, FR-1402, INV-076): публичный
 * доступ длится шесть месяцев, дальше протокол снимается джобом и остается
 * в досье. Гость видит только опубликованные - их фильтрует сервер.
 *
 * На публичной карточке тендера список приходит загрузчиком маршрута
 * (`routes/tenders/$tenderId.tsx`), поэтому к первой отрисовке он уже в кеше:
 * заглушка там не появляется и верстку SSR не двигает. В кабинетах запрос
 * идет из браузера - там заглушка нужна, иначе «протоколов нет» и «протоколы
 * не загрузились» выглядят одинаково.
 */
export function ProtocolsPanel({
  tenderId,
  canPublish = false,
}: {
  tenderId: string
  canPublish?: boolean
}) {
  const queryClient = useQueryClient()
  const protocols = useQuery(tenderProtocolsQuery(tenderId))

  const publish = useMutation({
    mutationFn: (protocolId: string) => publishProtocol(protocolId),
    onSuccess: async () => {
      notifySuccess(m.protocols_published())
      await queryClient.invalidateQueries({
        queryKey: tenderProtocolsQuery(tenderId).queryKey,
      })
    },
  })

  return (
    <section
      aria-labelledby="protocols"
      className="flex flex-col gap-3"
      data-testid="protocols-panel"
    >
      <h3 id="protocols" className="font-heading text-lg font-semibold">
        {m.protocols_title()}
      </h3>
      <QueryBoundary
        query={protocols}
        skeleton={
          <div className="flex flex-col gap-2" aria-hidden="true">
            <Skeleton className="h-16 w-full rounded-xl" />
            <Skeleton className="h-16 w-full rounded-xl" />
          </div>
        }
        empty={{
          when: (items) => items.length === 0,
          icon: FileTextIcon,
          title: m.protocols_empty_title(),
          description: m.protocols_empty(),
        }}
      >
        {(items) => (
          <ul className="flex flex-col gap-2 text-sm">
            {items.map((protocol) => (
              <li
                key={protocol.id}
                className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-xl border bg-card p-4 shadow-xs"
              >
                <span className="font-medium">
                  {protocolKindLabel(protocol.kind)}
                  {protocol.number != null && (
                    <span className="tabular-nums"> №{protocol.number}</span>
                  )}
                </span>
                <span
                  className="text-muted-foreground tabular-nums"
                  suppressHydrationWarning
                >
                  {formatDateTime(protocol.generated_at)}
                </span>
                <span
                  className={
                    protocol.is_public
                      ? "text-foreground"
                      : "text-muted-foreground"
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
        )}
      </QueryBoundary>
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
