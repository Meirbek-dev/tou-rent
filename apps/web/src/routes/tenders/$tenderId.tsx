import { FileQuestionIcon, FileTextIcon, SearchXIcon } from "lucide-react"
import { Link, createFileRoute, notFound } from "@tanstack/react-router"
import { useSuspenseQuery } from "@tanstack/react-query"

import { m } from "#/paraglide/messages"
import { getLocale } from "#/paraglide/runtime"
import { AmendmentsBanner } from "@/components/amendments-banner"
import { DeadlineBlock } from "@/components/deadline-block"
import { EmptyState } from "@/components/empty-state"
import { ProtocolsPanel } from "@/components/protocols-panel"
import { PublicShell } from "@/components/public-shell"
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
import { tenderAmendmentsQuery } from "@/lib/amendments"
import { tenderDocumentsQuery, tenderQuery } from "@/lib/api"
import { formatDateTime, formatTenge } from "@/lib/format"
import { tenderProtocolsQuery } from "@/lib/publications"
import { cn } from "@/lib/utils"

import type { LotDto } from "@/lib/api"

// FR-1401: карточка тендера - лоты, сроки, документация; SSR, работает без JS.
export const Route = createFileRoute("/tenders/$tenderId")({
  loader: async ({ context, params }) => {
    const tender = await context.queryClient.ensureQueryData(
      tenderQuery(params.tenderId)
    )
    if (tender === null) throw notFound()

    // NFR-04: изменения документации (продление срока, право отказаться -
    // FR-304, п. 27) и протоколы (FR-1402) - юридически значимая часть
    // карточки. Без предзагрузки их забирал только браузер, и без JS они
    // пропадали со страницы целиком. Оба маршрута открыты гостю.
    await Promise.all([
      context.queryClient.ensureQueryData(
        tenderAmendmentsQuery(params.tenderId)
      ),
      context.queryClient.ensureQueryData(
        tenderProtocolsQuery(params.tenderId)
      ),
      context.queryClient.ensureQueryData(
        tenderDocumentsQuery(params.tenderId)
      ),
    ])

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

function DateRow({
  label,
  value,
}: {
  label: string
  value?: string | null | undefined
}) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-sm text-muted-foreground">{label}</dt>
      {/* Intl-вывод может отличаться между SSR и браузерами с урезанным ICU */}
      <dd className="font-medium tabular-nums" suppressHydrationWarning>
        {formatDateTime(value) ?? m.tender_date_tbd()}
      </dd>
    </div>
  )
}

/** Ячейка карточки лота на узком экране: тот же лот, что и в строке таблицы. */
function LotFact({
  label,
  value,
  numeric = false,
  intl = false,
}: {
  label: string
  value: string
  numeric?: boolean
  intl?: boolean
}) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd
        className={cn(numeric && "tabular-nums")}
        suppressHydrationWarning={intl}
      >
        {value}
      </dd>
    </div>
  )
}

function LotCard({ lot }: { lot: LotDto }) {
  const kazakh = getLocale() === "kk"
  return (
    <li className="flex flex-col gap-3 rounded-xl border bg-card p-4 shadow-xs">
      <div className="flex items-baseline gap-2">
        <span className="text-sm text-muted-foreground tabular-nums">
          {m.lot_seq()}
          {lot.seq}
        </span>
        <h3 className="text-base font-semibold">
          {kazakh ? lot.purpose_kk : lot.purpose}
        </h3>
      </div>
      <dl className="grid grid-cols-2 gap-x-4 gap-y-3 text-sm">
        <LotFact
          label={m.lot_lease_months()}
          value={m.lot_months({ months: lot.lease_months })}
          numeric
        />
        <LotFact
          label={m.lot_base_rate()}
          value={formatTenge(lot.base_rate_monthly)}
          numeric
          intl
        />
        <LotFact
          label={m.lot_guarantee_fee()}
          value={formatTenge(lot.guarantee_fee)}
          numeric
          intl
        />
        <LotFact
          label={m.lot_viewing_terms()}
          value={lot.viewing_terms ?? "-"}
        />
      </dl>
    </li>
  )
}

function TenderPage() {
  const tender = Route.useLoaderData()
  const { data: documents } = useSuspenseQuery(tenderDocumentsQuery(tender.id))

  const kazakh = getLocale() === "kk"
  return (
    <PublicShell>
      <div className="flex flex-col gap-8">
        <nav aria-label={m.back_to_tenders()}>
          <Link
            to="/tenders"
            className="text-sm text-muted-foreground underline-offset-4 hover:underline"
          >
            ← {m.back_to_tenders()}
          </Link>
        </nav>

        <header className="grid grid-cols-[minmax(0,1fr)] overflow-hidden rounded-xl border bg-card shadow-xs md:grid-cols-[minmax(0,1fr)_auto]">
          <div className="flex flex-col gap-3 p-5 md:p-6">
            <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
              <TenderStatusBadge
                status={tender.status}
                deadline={tender.submission_deadline}
              />
              <span className="text-sm text-muted-foreground">
                {m.tender_id_label()}{" "}
                <span className="tabular-nums">{tender.id.slice(0, 8)}</span>
              </span>
            </div>
            <h1 className="text-3xl font-semibold tracking-tight text-balance">
              {tender.title}
            </h1>
          </div>
          <div className="border-t px-5 pt-4 pb-5 md:col-start-2 md:border-t-0 md:border-l md:px-6 md:py-6 md:text-right">
            <DeadlineBlock
              value={tender.submission_deadline}
              size="lg"
              className="md:min-w-[11rem] md:items-end"
            />
          </div>
        </header>

        <AmendmentsBanner tenderId={tender.id} />

        <section aria-labelledby="tender-dates">
          <h2 id="tender-dates" className="mb-3 text-xl font-semibold">
            {m.tender_dates_title()}
          </h2>
          <dl className="grid grid-cols-1 gap-4 rounded-xl border bg-card p-5 shadow-xs sm:grid-cols-2 lg:grid-cols-4">
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
          <h2 id="tender-lots" className="mb-3 text-xl font-semibold">
            {m.tender_lots_title()}
          </h2>

          {/* Одни и те же лоты в двух видах. Переключает CSS, а не JS:
              без скриптов узкий экран обязан получить читаемый список
              (NFR-04), а широкий - настоящую таблицу с заголовками */}
          <div className="hidden overflow-hidden rounded-xl border bg-card shadow-xs md:block">
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
                    <TableCell className="tabular-nums">{lot.seq}</TableCell>
                    <TableCell className="max-w-md whitespace-normal">
                      {kazakh ? lot.purpose_kk : lot.purpose}
                    </TableCell>
                    <TableCell className="tabular-nums">
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
          <ul className="flex flex-col gap-3 md:hidden">
            {tender.lots.map((lot) => (
              <LotCard key={lot.id} lot={lot} />
            ))}
          </ul>
        </section>

        <section aria-labelledby="tender-docs">
          <h2 id="tender-docs" className="mb-3 text-xl font-semibold">
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
          {documents.length === 0 ? (
            <EmptyState
              icon={FileQuestionIcon}
              title={m.tender_docs_empty_title()}
              description={m.tender_docs_empty()}
              className="mt-3 py-10"
            />
          ) : (
            <ul className="mt-3 grid gap-2">
              {documents.map((document) => (
                <li key={document.id}>
                  <a
                    href={`/api/v1/tenders/${tender.id}/documents/${document.id}`}
                    target="_blank"
                    rel="noreferrer"
                    className={cn(
                      buttonVariants({ variant: "outline" }),
                      "w-full justify-start"
                    )}
                  >
                    <FileTextIcon data-icon="inline-start" />
                    {document.title} · v{document.version}
                  </a>
                </li>
              ))}
            </ul>
          )}
        </section>
      </div>
    </PublicShell>
  )
}

function TenderNotFound() {
  return (
    <PublicShell>
      <EmptyState
        icon={SearchXIcon}
        title={m.tender_not_found_title()}
        titleAs="h1"
        description={m.tender_not_found_text()}
        className="my-10"
        action={
          <div className="flex flex-wrap items-center justify-center gap-3">
            <Link
              to="/tenders"
              className={cn(buttonVariants({ variant: "outline" }))}
            >
              {m.back_to_tenders()}
            </Link>
            <Link to="/" className={cn(buttonVariants({ variant: "ghost" }))}>
              {m.nav_home()}
            </Link>
          </div>
        }
      />
    </PublicShell>
  )
}
