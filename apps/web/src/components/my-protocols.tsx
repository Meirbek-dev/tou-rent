import { useQuery } from "@tanstack/react-query"
import { FileTextIcon } from "lucide-react"

import { m } from "#/paraglide/messages"
import { protocolKindLabel } from "@/components/protocols-panel"
import { QueryBoundary } from "@/components/query-boundary"
import { Skeleton } from "@/components/ui/skeleton"
import { formatDateTime } from "@/lib/format"
import { myProtocolsQuery } from "@/lib/publications"

/**
 * Копии протоколов в кабинете участника (FR-703, п. 56): по всем тендерам,
 * где участник подавал заявку, - независимо от публичного срока (п. 76).
 */
export function MyProtocols() {
  const protocols = useQuery(myProtocolsQuery)

  return (
    <section
      aria-labelledby="my-protocols"
      className="flex flex-col gap-3"
      data-testid="my-protocols"
    >
      <h2 id="my-protocols" className="font-heading text-lg font-semibold">
        {m.my_protocols_title()}
      </h2>
      <QueryBoundary
        query={protocols}
        skeleton={
          <div className="flex flex-col gap-2" aria-hidden="true">
            <Skeleton className="h-12 w-full rounded-lg" />
            <Skeleton className="h-12 w-full rounded-lg" />
          </div>
        }
        empty={{
          when: (page) => page.items.length === 0,
          icon: FileTextIcon,
          title: m.protocols_my_empty_title(),
          description: m.protocols_my_empty(),
        }}
      >
        {(page) => (
          <>
            {page.truncated && (
              <p
                role="status"
                className="rounded-lg bg-amber-500/10 px-3 py-2 text-sm text-amber-700 ring-1 ring-amber-500/30 dark:text-amber-400"
                data-testid="protocols-truncated"
              >
                {m.list_truncated({ count: page.items.length })}
              </p>
            )}
            <ul className="flex flex-col gap-2 text-sm">
              {page.items.map((protocol) => (
                <li
                  key={protocol.id}
                  className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-lg border p-3"
                >
                  <span className="font-medium">{protocol.tender_title}</span>
                  <span>
                    {protocolKindLabel(protocol.kind)}
                    {protocol.number != null && ` №${protocol.number}`}
                  </span>
                  <span
                    className="text-muted-foreground"
                    suppressHydrationWarning
                  >
                    {formatDateTime(protocol.generated_at)}
                  </span>
                  {protocol.has_pdf && (
                    <a
                      href={`/api/v1/protocols/${protocol.id}/pdf`}
                      className="underline-offset-4 hover:underline"
                    >
                      {m.protocols_pdf()}
                    </a>
                  )}
                </li>
              ))}
            </ul>
          </>
        )}
      </QueryBoundary>
    </section>
  )
}
