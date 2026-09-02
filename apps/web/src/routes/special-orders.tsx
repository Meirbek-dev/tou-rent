import { FileTextIcon } from "lucide-react"
import { createFileRoute } from "@tanstack/react-router"

import { m } from "#/paraglide/messages"
import { EmptyState } from "@/components/empty-state"
import { PublicShell } from "@/components/public-shell"
import { RegistryPending } from "@/components/registry-skeleton"
import { Badge } from "@/components/ui/badge"
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
  pendingComponent: () => <RegistryPending title={m.public_records_title()} />,
})

function SpecialOrdersPage() {
  const page = Route.useLoaderData()
  const records = page.items

  return (
    <PublicShell>
      <div className="flex flex-col gap-6">
        <div className="flex flex-col gap-2">
          <h1 className="text-3xl font-semibold tracking-tight">
            {m.public_records_title()}
          </h1>
          <p className="max-w-[68ch] text-muted-foreground">
            {m.public_records_hint()}
          </p>
        </div>

        {records.length === 0 ? (
          <EmptyState
            icon={FileTextIcon}
            title={m.public_records_empty_title()}
            description={m.public_records_empty()}
          />
        ) : (
          <>
            {/* Реестр публикаций обрезан потолком выборки: об этом говорят
                прямо, иначе неполный реестр читается как полный */}
            {page.truncated && (
              <p
                role="status"
                className="rounded-lg bg-amber-500/10 px-3 py-2 text-sm text-amber-700 ring-1 ring-amber-500/30 dark:text-amber-400"
                data-testid="public-records-truncated"
              >
                {m.list_truncated({ count: records.length })}
              </p>
            )}
            <p className="text-sm text-muted-foreground">
              {m.registry_found()}:{" "}
              <span className="font-medium text-foreground tabular-nums">
                {records.length}
              </span>
            </p>
            <ul className="flex flex-col gap-3">
              {records.map((record) => (
                <li key={record.id}>
                  <PublicRecordCard record={record} />
                </li>
              ))}
            </ul>
          </>
        )}
      </div>
    </PublicShell>
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
    <article className="overflow-hidden rounded-xl border bg-card shadow-xs">
      <div className="flex flex-col gap-3 p-5">
        <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
          <Badge variant="outline">{record.kind_title_ru}</Badge>
          <span className="text-sm text-muted-foreground tabular-nums">
            {record.rule_ref}
          </span>
          <span
            className="text-sm text-muted-foreground"
            suppressHydrationWarning
          >
            {m.public_records_published_at()}:{" "}
            <span className="tabular-nums">
              {formatDateTime(record.published_at)}
            </span>
          </span>
        </div>
        <h2 className="text-base font-semibold">{record.title}</h2>

        {/* Обоснование ставки - сам расчет Прил. 4 (FR-201, п. 97) */}
        {calculation?.monthly_rate != null && (
          <p className="text-sm">
            {m.public_records_rate({ rate: calculation.monthly_rate })}
          </p>
        )}

        {record.has_file && (
          <div>
            <a
              href={`/api/v1/public-records/${record.id}/pdf`}
              className={cn(buttonVariants({ variant: "outline", size: "sm" }))}
            >
              {m.public_records_document()}
            </a>
          </div>
        )}
      </div>

      {/* INV-076 (п. 76): публичный доступ длится шесть месяцев */}
      <p
        className="border-t bg-muted px-5 py-3 text-sm text-muted-foreground"
        suppressHydrationWarning
      >
        {m.public_records_available_until({
          date: formatDate(record.unpublish_at) ?? record.unpublish_at,
        })}
      </p>
    </article>
  )
}
