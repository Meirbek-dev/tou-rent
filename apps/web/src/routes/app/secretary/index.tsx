import { Link, createFileRoute } from "@tanstack/react-router"
import { useSuspenseQuery } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { EvaderRegistry } from "@/components/evader-registry"
import { InvestmentContracts } from "@/components/investment-contracts"
import { MyDeadlines } from "@/components/my-deadlines"
import { TenderStatusBadge } from "@/components/tender-status-badge"
import { formatDateTime } from "@/lib/format"
import {
  investmentAttachmentsQuery,
  investmentContractsQuery,
} from "@/lib/investment"
import { organizerTendersQuery } from "@/lib/organizer"

// Секретарь: тендеры → журнал регистрации и заявки (FR-402).
export const Route = createFileRoute("/app/secretary/")({
  loader: async ({ context }) => {
    await Promise.all([
      context.queryClient.ensureQueryData(organizerTendersQuery),
      context.queryClient.ensureQueryData(investmentContractsQuery),
      context.queryClient.ensureQueryData(investmentAttachmentsQuery),
    ])
  },
  component: SecretaryHome,
})

function SecretaryHome() {
  const { data: page } = useSuspenseQuery(organizerTendersQuery)
  const relevant = page.items.filter((t) => t.status !== "draft")

  return (
    <div className="flex flex-col gap-6">
      <MyDeadlines />
      <EvaderRegistry />
      {/* FR-1204 (п. 92): приемку инвестиций оформляет секретарь комиссии */}
      <InvestmentContracts roles={["secretary"]} />
      <h2 className="font-heading text-lg font-semibold">
        {m.secretary_tenders_title()}
      </h2>
      {relevant.length === 0 ? (
        <p className="text-muted-foreground">{m.org_tenders_empty()}</p>
      ) : (
        <ul className="flex flex-col gap-3">
          {relevant.map((tender) => (
            <li
              key={tender.id}
              className="rounded-lg border p-4 transition-colors hover:bg-muted/50"
            >
              <div className="flex flex-wrap items-center gap-3">
                <TenderStatusBadge status={tender.status} />
                {tender.submission_deadline != null && (
                  <span
                    className="text-sm text-muted-foreground"
                    suppressHydrationWarning
                  >
                    {m.tender_deadline()}:{" "}
                    {formatDateTime(tender.submission_deadline)}
                  </span>
                )}
              </div>
              <h3 className="mt-2 font-heading text-lg font-semibold">
                <Link
                  to="/app/secretary/tenders/$tenderId"
                  params={{ tenderId: tender.id }}
                  className="underline-offset-4 hover:underline"
                >
                  {tender.title}
                </Link>
              </h3>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
