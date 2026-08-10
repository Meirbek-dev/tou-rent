import { Link, createFileRoute } from "@tanstack/react-router"
import { useSuspenseQuery } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { TenderStatusBadge } from "@/components/tender-status-badge"
import { buttonVariants } from "@/components/ui/button"
import { formatDateTime } from "@/lib/format"
import { organizerTendersQuery } from "@/lib/organizer"
import { cn } from "@/lib/utils"

// Тендеры глазами организатора: включая черновики (FR-301).
export const Route = createFileRoute("/app/organizer/tenders/")({
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(organizerTendersQuery),
  component: OrganizerTendersPage,
})

function OrganizerTendersPage() {
  const { data: page } = useSuspenseQuery(organizerTendersQuery)

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between gap-3">
        <h2 className="font-heading text-lg font-semibold">
          {m.org_tenders_title()}
        </h2>
        <Link to="/app/organizer/tenders/new" className={cn(buttonVariants())}>
          {m.tender_create_cta()}
        </Link>
      </div>

      {page.items.length === 0 ? (
        <p className="text-muted-foreground">{m.org_tenders_empty()}</p>
      ) : (
        <ul className="flex flex-col gap-3">
          {page.items.map((tender) => (
            <li
              key={tender.id}
              className="rounded-lg border p-4 transition-colors hover:bg-muted/50"
            >
              <div className="flex flex-wrap items-center gap-3">
                <TenderStatusBadge status={tender.status} />
                <span className="text-sm text-muted-foreground">
                  {m.tenders_lots_count({ count: tender.lots.length })}
                </span>
              </div>
              <h3 className="mt-2 font-heading text-lg font-semibold">
                <Link
                  to="/app/organizer/tenders/$tenderId"
                  params={{ tenderId: tender.id }}
                  className="underline-offset-4 hover:underline"
                >
                  {tender.title}
                </Link>
              </h3>
              {tender.submission_deadline != null && (
                <p
                  className="mt-1 text-sm text-muted-foreground"
                  suppressHydrationWarning
                >
                  {m.tender_deadline()}:{" "}
                  {formatDateTime(tender.submission_deadline)}
                </p>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
