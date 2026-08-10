import { useQuery } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { evaderRegistryQuery } from "@/lib/evasion"
import { formatDateTime } from "@/lib/format"

/**
 * Реестр уклонистов (FR-505, п. 52.4, 120): их заявки в будущих тендерах
 * отклоняются автоматически - реестр показывает, кого это касается.
 */
export function EvaderRegistry() {
  const { data: page } = useQuery(evaderRegistryQuery)
  if (page === undefined || page.items.length === 0) return null

  return (
    <section
      aria-labelledby="evaders"
      className="flex flex-col gap-2 rounded-lg border p-4"
      data-testid="evader-registry"
    >
      <h3 id="evaders" className="font-heading text-lg font-semibold">
        {m.evaders_title()}
      </h3>
      <p className="text-sm text-muted-foreground">{m.evaders_hint()}</p>
      {/* Усечение видно на самом реестре, а не сноской под ним: увидеть
          неполный список и не понять этого - хуже, чем не увидеть вовсе */}
      {page.truncated && (
        <p
          role="status"
          className="rounded-md border border-amber-500/60 bg-amber-500/10 px-3 py-2 text-sm"
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
              <span className="text-muted-foreground" suppressHydrationWarning>
                {formatDateTime(evader.last_declared_at)}
              </span>
            )}
          </li>
        ))}
      </ul>
    </section>
  )
}
