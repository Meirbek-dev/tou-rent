import { useQuery } from "@tanstack/react-query"
import { ShieldCheckIcon } from "lucide-react"

import { m } from "#/paraglide/messages"
import { Panel } from "@/components/panel"
import { QueryBoundary } from "@/components/query-boundary"
import { Skeleton } from "@/components/ui/skeleton"
import { evaderRegistryQuery } from "@/lib/evasion"
import { formatDateTime } from "@/lib/format"

/**
 * Реестр уклонистов (FR-505, п. 52.4, 120): их заявки в будущих тендерах
 * отклоняются автоматически - реестр показывает, кого это касается.
 *
 * Пустой реестр - утверждение («уклонившихся нет»), а не отсутствие ответа,
 * поэтому он произносится вслух и отличим от неудавшегося запроса.
 */
export function EvaderRegistry() {
  const registry = useQuery(evaderRegistryQuery)

  return (
    <div data-testid="evader-registry">
      <Panel
        title={m.evaders_title()}
        description={m.evaders_hint()}
        contentClassName="flex flex-col gap-2"
      >
        <QueryBoundary
          query={registry}
          skeleton={
            <div className="flex flex-col gap-1.5" aria-hidden="true">
              <Skeleton className="h-5 w-2/3 rounded-md" />
              <Skeleton className="h-5 w-1/2 rounded-md" />
            </div>
          }
          empty={{
            when: (page) => page.items.length === 0,
            icon: ShieldCheckIcon,
            title: m.evaders_empty_title(),
            description: m.evaders_empty(),
          }}
        >
          {(page) => (
            <>
              {/* Усечение видно на самом реестре, а не сноской под ним: увидеть
                  неполный список и не понять этого - хуже, чем не увидеть вовсе */}
              {page.truncated && (
                <p
                  role="status"
                  className="rounded-lg bg-amber-500/10 px-3 py-2 text-sm text-amber-700 ring-1 ring-amber-500/30 dark:text-amber-400"
                  data-testid="evaders-truncated"
                >
                  {m.list_truncated({ count: page.items.length })}
                </p>
              )}
              <ul className="flex flex-col gap-1 text-sm">
                {page.items.map((evader) => (
                  <li
                    key={evader.user_id}
                    className="flex flex-wrap items-center gap-x-3"
                  >
                    <span className="font-medium">{evader.full_name}</span>
                    <span className="text-muted-foreground">
                      {m.evaders_count({ count: evader.evasions })}
                    </span>
                    {evader.last_declared_at != null && (
                      <span
                        className="text-muted-foreground"
                        suppressHydrationWarning
                      >
                        {formatDateTime(evader.last_declared_at)}
                      </span>
                    )}
                  </li>
                ))}
              </ul>
            </>
          )}
        </QueryBoundary>
      </Panel>
    </div>
  )
}
