import { Link } from "@tanstack/react-router"
import { useQuery } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { deadlineLabel } from "@/lib/obligation-labels"
import { myObligationsQuery } from "@/lib/obligations"
import { formatDateTime } from "@/lib/format"

/**
 * «Мои сроки» (FR-1702): открытые обязательства ролей пользователя со
 * ссылкой на пункт Правил, из которого срок взят. Просроченные выделены -
 * их же дублирует уведомление-эскалация из фонового воркера.
 */
export function MyDeadlines() {
  const { data: obligations } = useQuery(myObligationsQuery)

  if (obligations === undefined || obligations.length === 0) return null

  return (
    <section aria-labelledby="deadlines" className="flex flex-col gap-3">
      <h2 id="deadlines" className="font-heading text-lg font-semibold">
        {m.deadlines_title()}
      </h2>
      <ul className="flex flex-col gap-2" data-testid="my-deadlines">
        {obligations.map((obligation) => (
          <li
            key={obligation.id}
            className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-lg border p-3 text-sm"
          >
            <span className="font-medium">
              {deadlineLabel(obligation.action)}
            </span>
            <span className="text-muted-foreground">{obligation.rule_ref}</span>
            {obligation.tender_id != null &&
              obligation.tender_title != null && (
                <Link
                  to="/tenders/$tenderId"
                  params={{ tenderId: obligation.tender_id }}
                  className="underline-offset-4 hover:underline"
                >
                  {obligation.tender_title}
                </Link>
              )}
            <span
              className={
                obligation.status === "overdue"
                  ? "ml-auto font-medium text-destructive"
                  : "ml-auto text-muted-foreground"
              }
              suppressHydrationWarning
            >
              {obligation.status === "overdue"
                ? m.deadline_overdue({
                    date: formatDateTime(obligation.due_at) ?? "-",
                  })
                : m.deadline_due({
                    date: formatDateTime(obligation.due_at) ?? "-",
                  })}
            </span>
          </li>
        ))}
      </ul>
    </section>
  )
}
