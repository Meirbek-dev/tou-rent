import { Link, createFileRoute } from "@tanstack/react-router"
import { useSuspenseQuery } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { MyDeadlines } from "@/components/my-deadlines"
import { ApplicationStatusBadge } from "@/components/application-status-badge"
import { LandInvestorPanel } from "@/components/land-panels"
import { MyProtocols } from "@/components/my-protocols"
import { TenderStatusBadge } from "@/components/tender-status-badge"
import { buttonVariants } from "@/components/ui/button"
import { tendersPageQuery } from "@/lib/api"
import { formatDateTime, formatTenge } from "@/lib/format"
import { myApplicationsQuery } from "@/lib/participant"
import { mySpecialRequestsQuery, specialStatusLabel } from "@/lib/special"
import { cn } from "@/lib/utils"

// Кабинет участника: мои заявки + тендеры с открытым приемом (FR-401).
export const Route = createFileRoute("/app/participant/")({
  loader: async ({ context }) => {
    await Promise.all([
      context.queryClient.ensureQueryData(myApplicationsQuery),
      context.queryClient.ensureQueryData(tendersPageQuery()),
      context.queryClient.ensureQueryData(mySpecialRequestsQuery),
    ])
  },
  component: ParticipantHome,
})

function ParticipantHome() {
  const { data: applications } = useSuspenseQuery(myApplicationsQuery)
  const { data: tendersPage } = useSuspenseQuery(tendersPageQuery())
  const { data: specialRequests } = useSuspenseQuery(mySpecialRequestsQuery)
  const accepting = tendersPage.items.filter((t) => t.status === "accepting")

  return (
    <div className="flex flex-col gap-8">
      <MyDeadlines />

      <MyProtocols />

      {/* FR-1801 (п. 104–105): заявка инвестора на земельный участок */}
      <LandInvestorPanel />

      {/* Свои договоры (FR-902, FR-1003): у нанимателя есть свои шаги
          конвейера и свой депозит - раньше их отмечал за него организатор */}
      <section aria-labelledby="my-contracts-link">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <h2
            id="my-contracts-link"
            className="font-heading text-lg font-semibold"
          >
            {m.my_contracts_title()}
          </h2>
          <Link
            to="/app/participant/contracts"
            className={cn(buttonVariants({ variant: "outline" }))}
          >
            {m.my_contracts_title()}
          </Link>
        </div>
      </section>

      <section aria-labelledby="my-applications">
        <h2
          id="my-applications"
          className="mb-3 font-heading text-lg font-semibold"
        >
          {m.my_applications_title()}
        </h2>
        {applications.length === 0 ? (
          <p className="text-muted-foreground">{m.my_applications_empty()}</p>
        ) : (
          <ul className="flex flex-col gap-3">
            {applications.map((application) => (
              <li
                key={application.id}
                className="rounded-lg border p-4 transition-colors hover:bg-muted/50"
              >
                <div className="flex flex-wrap items-center gap-3">
                  <ApplicationStatusBadge status={application.status} />
                  {application.price_amount != null && (
                    <span
                      className="text-sm text-muted-foreground"
                      suppressHydrationWarning
                    >
                      {m.application_price()}:{" "}
                      {formatTenge(application.price_amount)}
                    </span>
                  )}
                </div>
                <p className="mt-2 font-medium">
                  <Link
                    to="/app/participant/applications/$applicationId"
                    params={{ applicationId: application.id }}
                    className="underline-offset-4 hover:underline"
                  >
                    {m.application_card_title({
                      id: application.id.slice(0, 8),
                    })}
                  </Link>
                </p>
                <p
                  className="mt-1 text-sm text-muted-foreground"
                  suppressHydrationWarning
                >
                  {m.application_submitted_at()}:{" "}
                  {formatDateTime(application.submitted_at)}
                </p>
              </li>
            ))}
          </ul>
        )}
      </section>

      {/* FR-1201 (п. 87–88): особый порядок - заявка вне тендера */}
      <section aria-labelledby="special-requests">
        <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
          <h2
            id="special-requests"
            className="font-heading text-lg font-semibold"
          >
            {m.special_requests_title()}
          </h2>
          <Link
            to="/app/participant/special/new"
            className={cn(buttonVariants({ variant: "outline" }))}
          >
            {m.special_new_cta()}
          </Link>
        </div>
        {specialRequests.length === 0 ? (
          <p className="text-muted-foreground">{m.special_requests_empty()}</p>
        ) : (
          <ul className="flex flex-col gap-3">
            {specialRequests.map((request) => (
              <li
                key={request.id}
                className="rounded-lg border p-4 transition-colors hover:bg-muted/50"
              >
                <div className="flex flex-wrap items-center gap-3">
                  <span className="rounded-md border px-2 py-0.5 text-sm">
                    {specialStatusLabel(request.status)}
                  </span>
                  <span className="text-sm text-muted-foreground">
                    {request.category_label} ({request.category_rule_ref})
                  </span>
                </div>
                <p className="mt-2 font-medium">
                  <Link
                    to="/app/participant/special/$requestId"
                    params={{ requestId: request.id }}
                    className="underline-offset-4 hover:underline"
                  >
                    {m.special_card_title({ id: request.id.slice(0, 8) })}
                  </Link>
                </p>
                <p
                  className="mt-1 text-sm text-muted-foreground"
                  suppressHydrationWarning
                >
                  {m.application_submitted_at()}:{" "}
                  {formatDateTime(request.submitted_at)}
                </p>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section aria-labelledby="open-tenders">
        <h2
          id="open-tenders"
          className="mb-3 font-heading text-lg font-semibold"
        >
          {m.open_tenders_title()}
        </h2>
        {accepting.length === 0 ? (
          <p className="text-muted-foreground">{m.open_tenders_empty()}</p>
        ) : (
          <ul className="flex flex-col gap-3">
            {accepting.map((tender) => (
              <li key={tender.id} className="rounded-lg border p-4">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div>
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
                    <p className="mt-2 font-heading text-lg font-semibold">
                      {tender.title}
                    </p>
                  </div>
                  <Link
                    to="/app/participant/apply/$tenderId"
                    params={{ tenderId: tender.id }}
                    className={cn(buttonVariants())}
                  >
                    {m.apply_cta()}
                  </Link>
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  )
}
