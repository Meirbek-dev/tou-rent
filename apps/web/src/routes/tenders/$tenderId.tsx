import { Link, createFileRoute, notFound } from "@tanstack/react-router"
import { m } from "#/paraglide/messages"
import { AmendmentsBanner } from "@/components/amendments-banner"
import { ProtocolsPanel } from "@/components/protocols-panel"
import { SiteHeader } from "@/components/site-header"
import { TenderStatusBadge } from "@/components/tender-status-badge"
import { buttonVariants } from "@/components/ui/button"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { tenderQuery } from "@/lib/api"
import { formatDateTime, formatTenge } from "@/lib/format"
import { cn } from "@/lib/utils"

// FR-1401: карточка тендера - лоты, сроки, документация; SSR, работает без JS.
export const Route = createFileRoute("/tenders/$tenderId")({
  loader: async ({ context, params }) => {
    const tender = await context.queryClient.ensureQueryData(
      tenderQuery(params.tenderId)
    )
    if (tender === null) throw notFound()
    return tender
  },
  head: ({ loaderData }) => ({
    meta: [
      { title: loaderData ? `${loaderData.title} - ToU Rent` : "ToU Rent" },
    ],
  }),
  component: TenderPage,
  notFoundComponent: TenderNotFound,
})

function DateRow({ label, value }: { label: string; value?: string | null }) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-sm text-muted-foreground">{label}</dt>
      {/* Intl-вывод может отличаться между SSR и браузерами с урезанным ICU */}
      <dd className="font-medium" suppressHydrationWarning>
        {formatDateTime(value) ?? m.tender_date_tbd()}
      </dd>
    </div>
  )
}

function TenderPage() {
  const tender = Route.useLoaderData()

  return (
    <>
      <SiteHeader />
      <main className="mx-auto flex w-full max-w-6xl flex-col gap-8 px-6 py-10">
        <nav aria-label={m.back_to_tenders()}>
          <Link
            to="/tenders"
            className="text-sm text-muted-foreground underline-offset-4 hover:underline"
          >
            ← {m.back_to_tenders()}
          </Link>
        </nav>

        <header className="flex flex-col gap-3">
          <div className="flex flex-wrap items-center gap-3">
            <TenderStatusBadge status={tender.status} />
            <span className="text-sm text-muted-foreground">
              {m.tender_card_title({ id: tender.id.slice(0, 8) })}
            </span>
          </div>
          <h1 className="font-heading text-3xl font-semibold tracking-tight text-balance">
            {tender.title}
          </h1>
        </header>

        <AmendmentsBanner tenderId={tender.id} />

        <section aria-labelledby="tender-dates">
          <h2
            id="tender-dates"
            className="mb-3 font-heading text-xl font-semibold"
          >
            {m.tender_dates_title()}
          </h2>
          <dl className="grid grid-cols-1 gap-4 rounded-lg border p-4 sm:grid-cols-2 lg:grid-cols-4">
            <DateRow
              label={m.tender_announced_at()}
              value={tender.announced_at}
            />
            <DateRow
              label={m.tender_deadline()}
              value={tender.submission_deadline}
            />
            <DateRow label={m.tender_opening_at()} value={tender.opening_at} />
            <DateRow label={m.tender_trading_at()} value={tender.trading_at} />
          </dl>
        </section>

        <ProtocolsPanel tenderId={tender.id} />

        <section aria-labelledby="tender-lots">
          <h2
            id="tender-lots"
            className="mb-3 font-heading text-xl font-semibold"
          >
            {m.tender_lots_title()}
          </h2>
          <div className="rounded-lg border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead scope="col">{m.lot_seq()}</TableHead>
                  <TableHead scope="col">{m.lot_purpose()}</TableHead>
                  <TableHead scope="col">{m.lot_lease_months()}</TableHead>
                  <TableHead scope="col" className="text-right">
                    {m.lot_base_rate()}
                  </TableHead>
                  <TableHead scope="col" className="text-right">
                    {m.lot_guarantee_fee()}
                  </TableHead>
                  <TableHead scope="col">{m.lot_viewing_terms()}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {tender.lots.map((lot) => (
                  <TableRow key={lot.id}>
                    <TableCell>{lot.seq}</TableCell>
                    <TableCell className="max-w-md whitespace-normal">
                      {lot.purpose}
                    </TableCell>
                    <TableCell>
                      {m.lot_months({ months: lot.lease_months })}
                    </TableCell>
                    <TableCell
                      className="text-right tabular-nums"
                      suppressHydrationWarning
                    >
                      {formatTenge(lot.base_rate_monthly)}
                    </TableCell>
                    <TableCell
                      className="text-right tabular-nums"
                      suppressHydrationWarning
                    >
                      {formatTenge(lot.guarantee_fee)}
                    </TableCell>
                    <TableCell className="max-w-xs whitespace-normal text-muted-foreground">
                      {lot.viewing_terms ?? "-"}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        </section>

        <section aria-labelledby="tender-docs">
          <h2
            id="tender-docs"
            className="mb-3 font-heading text-xl font-semibold"
          >
            {m.tender_docs_title()}
          </h2>
          <p>
            <a
              href={`/api/v1/tenders/${tender.id}/announcement.pdf`}
              target="_blank"
              rel="noreferrer"
              className={cn(buttonVariants({ variant: "outline" }))}
            >
              {m.tender_announcement_pdf()}
            </a>
          </p>
          {/* Тендерную документацию (файлы) подключает Т8 (RustFS) */}
          <p className="mt-3 text-muted-foreground">{m.tender_docs_empty()}</p>
        </section>
      </main>
    </>
  )
}

function TenderNotFound() {
  return (
    <>
      <SiteHeader />
      <main className="mx-auto flex w-full max-w-6xl flex-col items-start gap-4 px-6 py-16">
        <h1 className="font-heading text-2xl font-semibold">
          {m.tender_not_found_title()}
        </h1>
        <p className="text-muted-foreground">{m.tender_not_found_text()}</p>
        <Link
          to="/tenders"
          className={cn(buttonVariants({ variant: "outline" }))}
        >
          {m.back_to_tenders()}
        </Link>
      </main>
    </>
  )
}
