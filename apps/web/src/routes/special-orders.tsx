import { createFileRoute } from "@tanstack/react-router"
import { m } from "#/paraglide/messages"
import { SiteHeader } from "@/components/site-header"
import { buttonVariants } from "@/components/ui/button"
import { formatDate, formatDateTime } from "@/lib/format"
import { publicRecordsQuery } from "@/lib/public-records"
import { cn } from "@/lib/utils"

import type { PublicRecord } from "@/lib/public-records"

// FR-1403 (п. 90, 92, 97): публикации особого порядка - результаты
// рассмотрения заявок, обоснования ставок и акты приемки инвестиций.
// Страница публичная и работает без JS (SSR, FR-1401); материал висит
// шесть месяцев, после чего снимается джобом и остается в досье (INV-076).
export const Route = createFileRoute("/special-orders")({
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(publicRecordsQuery),
  head: () => ({ meta: [{ title: `${m.public_records_title()} - ToU Rent` }] }),
  component: SpecialOrdersPage,
})

function SpecialOrdersPage() {
  const records = Route.useLoaderData()

  return (
    <>
      <SiteHeader />
      <main className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-6 py-10">
        <h1 className="font-heading text-3xl font-semibold tracking-tight">
          {m.public_records_title()}
        </h1>
        <p className="text-muted-foreground">{m.public_records_hint()}</p>

        {records.length === 0 ? (
          <p className="py-8 text-muted-foreground">
            {m.public_records_empty()}
          </p>
        ) : (
          <ul className="flex flex-col gap-3">
            {records.map((record) => (
              <li key={record.id}>
                <PublicRecordCard record={record} />
              </li>
            ))}
          </ul>
        )}
      </main>
    </>
  )
}

function PublicRecordCard({ record }: { record: PublicRecord }) {
  const rate = record.kind === "rate"
  const calculation = rate
    ? (record.payload as {
        monthly_rate?: string
        calculation?: { annual?: string; multiplier?: string }
      })
    : null

  return (
    <article className="flex flex-col gap-2 rounded-lg border p-4">
      <div className="flex flex-wrap items-center gap-3">
        <span className="rounded-md border px-2 py-0.5 text-sm">
          {record.kind_title_ru}
        </span>
        <span className="text-sm text-muted-foreground">{record.rule_ref}</span>
        <span
          className="text-sm text-muted-foreground"
          suppressHydrationWarning
        >
          {m.public_records_published_at()}:{" "}
          {formatDateTime(record.published_at)}
        </span>
      </div>
      <h2 className="font-medium">{record.title}</h2>

      {/* Обоснование ставки - сам расчет Прил. 4 (FR-201, п. 97) */}
      {calculation?.monthly_rate != null && (
        <p className="text-sm">
          {m.public_records_rate({ rate: calculation.monthly_rate })}
        </p>
      )}

      <div className="flex flex-wrap items-center gap-3">
        {record.has_file && (
          <a
            href={`/api/v1/public-records/${record.id}/pdf`}
            className={cn(buttonVariants({ variant: "outline", size: "sm" }))}
          >
            {m.public_records_document()}
          </a>
        )}
        {/* INV-076 (п. 76): публичный доступ длится шесть месяцев */}
        <span
          className="text-sm text-muted-foreground"
          suppressHydrationWarning
        >
          {m.public_records_available_until({
            date: formatDate(record.unpublish_at) ?? record.unpublish_at,
          })}
        </span>
      </div>
    </article>
  )
}
