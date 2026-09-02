import { Link } from "@tanstack/react-router"
import { useQuery } from "@tanstack/react-query"
import { CalendarCheckIcon } from "lucide-react"

import { m } from "#/paraglide/messages"
import { QueryBoundary } from "@/components/query-boundary"
import { Skeleton } from "@/components/ui/skeleton"
import { deadlineLabel } from "@/lib/obligation-labels"
import { myObligationsQuery } from "@/lib/obligations"
import { formatDateTime } from "@/lib/format"

/**
 * «Мои сроки» (FR-1702): открытые обязательства ролей пользователя со
 * ссылкой на пункт Правил, из которого срок взят. Просроченные выделены -
 * их же дублирует уведомление-эскалация из фонового воркера.
 *
 * Раздел больше не исчезает целиком: «сроков нет» и «сроки не загрузились» -
 * разные новости, и вторую надо чинить, а не молча принимать за первую.
 */
export function MyDeadlines() {
  const obligations = useQuery(myObligationsQuery)

  return (
    <section aria-labelledby="deadlines" className="flex flex-col gap-3">
      <h2 id="deadlines" className="font-heading text-lg font-semibold">
        {m.deadlines_title()}
      </h2>
      <QueryBoundary
        query={obligations}
        skeleton={
          <div className="flex flex-col gap-2" aria-hidden="true">
            <Skeleton className="h-12 w-full rounded-lg" />
            <Skeleton className="h-12 w-full rounded-lg" />
            <Skeleton className="h-12 w-full rounded-lg" />
          </div>
        }
        empty={{
          when: (page) => page.items.length === 0,
          icon: CalendarCheckIcon,
          title: m.deadlines_empty_title(),
          description: m.deadlines_empty(),
        }}
      >
        {(page) => (
          <>
            {/* Обрезанная выборка сроков опаснее длинного списка: за потолком
                остается срок, о котором пользователь так и не узнает */}
            {page.truncated && (
              <p
                role="status"
                className="rounded-lg bg-amber-500/10 px-3 py-2 text-sm text-amber-700 ring-1 ring-amber-500/30 dark:text-amber-400"
                data-testid="deadlines-truncated"
              >
                {m.list_truncated({ count: page.items.length })}
              </p>
            )}
            <ul className="flex flex-col gap-2" data-testid="my-deadlines">
              {page.items.map((obligation) => (
                <li
                  key={obligation.id}
                  className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-lg border p-3 text-sm"
                >
                  <span className="font-medium">
                    {deadlineLabel(obligation.action)}
                  </span>
                  <span className="text-muted-foreground">
                    {obligation.rule_ref}
                  </span>
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
          </>
        )}
      </QueryBoundary>
    </section>
  )
}
