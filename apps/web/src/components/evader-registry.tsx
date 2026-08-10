import { useQuery } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { evaderRegistryQuery } from "@/lib/evasion"
import { formatDateTime } from "@/lib/format"

/**
 * Реестр уклонистов (FR-505, п. 52.4, 120): их заявки в будущих тендерах
 * отклоняются автоматически - реестр показывает, кого это касается.
 */
export function EvaderRegistry() {
  const { data: evaders } = useQuery(evaderRegistryQuery)
  if (evaders === undefined || evaders.length === 0) return null

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
      <ul className="flex flex-col gap-1 text-sm">
        {evaders.map((evader) => (
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
