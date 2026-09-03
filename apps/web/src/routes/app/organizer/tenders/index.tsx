import { useState } from "react"
import { Link, createFileRoute } from "@tanstack/react-router"
import { useSuspenseInfiniteQuery } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { PageHeader } from "@/components/page-header"
import { TenderStatusBadge } from "@/components/tender-status-badge"
import { Button, buttonVariants } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { formatDateTime } from "@/lib/format"
import { localizedTenderTitle } from "@/lib/api"
import {
  matchesTenderFilter,
  organizerTendersPagesQuery,
} from "@/lib/organizer"
import { cn } from "@/lib/utils"

import type { TenderStatus } from "@/lib/api"

// Тендеры глазами организатора: включая черновики (FR-301).
//
// Реестр читается страницами (`next_after` контракта): пятьдесят семь
// записей одним списком - лента, в которой нужную строку ищут прокруткой.
// Фильтр по статусу и поиск по названию работают по загруженным страницам,
// поэтому «Показать еще» остается на виду, пока сервер отдает продолжение.
export const Route = createFileRoute("/app/organizer/tenders/")({
  loader: ({ context }) =>
    context.queryClient.ensureInfiniteQueryData(organizerTendersPagesQuery),
  head: () => ({ meta: [{ title: `${m.org_tenders_title()} - ToU Rent` }] }),
  component: OrganizerTendersPage,
})

/** Статусы фильтра - те же, что показывает значок состояния тендера. */
const STATUS_OPTIONS: readonly TenderStatus[] = [
  "draft",
  "announced",
  "accepting",
  "qualification",
  "trading",
  "summed_up",
  "contracted",
  "failed",
  "repeat_announced",
  "cancelled",
]

const STATUS_LABELS: Record<TenderStatus, () => string> = {
  draft: m.tender_status_draft,
  announced: m.tender_status_announced,
  accepting: m.tender_status_accepting,
  qualification: m.tender_status_qualification,
  trading: m.tender_status_trading,
  summed_up: m.tender_status_summed_up,
  contracted: m.tender_status_contracted,
  failed: m.tender_status_failed,
  repeat_announced: m.tender_status_repeat_announced,
  cancelled: m.tender_status_cancelled,
}

function OrganizerTendersPage() {
  const tenders = useSuspenseInfiniteQuery(organizerTendersPagesQuery)
  const [status, setStatus] = useState("")
  const [query, setQuery] = useState("")

  const loaded = tenders.data.pages.flatMap((page) => page.items)
  const shown = loaded.filter((tender) =>
    matchesTenderFilter(tender, { status, query })
  )

  return (
    <div className="flex flex-col gap-4">
      {/* Заголовок страницы, а не раздела: реестр тендеров - самостоятельный
          экран кабинета, и `h1` принадлежит ему */}
      <PageHeader
        title={m.org_tenders_title()}
        actions={
          <Link
            to="/app/organizer/tenders/new"
            className={cn(buttonVariants())}
          >
            {m.tender_create_cta()}
          </Link>
        }
      />

      <fieldset className="flex flex-wrap items-end gap-3">
        <legend className="sr-only">{m.tenders_filter_legend()}</legend>
        <div className="flex min-w-48 flex-col gap-1.5">
          <Label htmlFor="org-tenders-status">
            {m.tenders_filter_status_label()}
          </Label>
          <NativeSelect
            id="org-tenders-status"
            data-testid="org-tenders-status"
            value={status}
            onChange={(event) => setStatus(event.target.value)}
          >
            <NativeSelectOption value="">
              {m.tenders_filter_all()}
            </NativeSelectOption>
            {STATUS_OPTIONS.map((value) => (
              <NativeSelectOption key={value} value={value}>
                {STATUS_LABELS[value]()}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </div>
        <div className="flex min-w-56 flex-1 flex-col gap-1.5">
          <Label htmlFor="org-tenders-query">
            {m.tenders_filter_query_label()}
          </Label>
          <Input
            id="org-tenders-query"
            data-testid="org-tenders-query"
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </div>
      </fieldset>

      {loaded.length === 0 ? (
        <p className="text-muted-foreground">{m.org_tenders_empty()}</p>
      ) : shown.length === 0 ? (
        <p className="text-muted-foreground" data-testid="org-tenders-filtered">
          {m.org_tenders_filtered_empty()}
        </p>
      ) : (
        <ul className="flex flex-col gap-3" data-testid="org-tenders-list">
          {shown.map((tender) => (
            <li
              key={tender.id}
              className="rounded-lg border p-4 transition-colors hover:bg-muted/50"
            >
              <div className="flex flex-wrap items-center gap-3">
                <TenderStatusBadge
                  status={tender.status}
                  deadline={tender.submission_deadline}
                />
                <span className="text-sm text-muted-foreground">
                  {m.tenders_lots_count({ count: tender.lots.length })}
                </span>
              </div>
              {/* Длинное название на 375 px разъезжалось вправо вместе со
                  всей страницей: ссылка обязана переноситься */}
              <h3 className="mt-2 min-w-0 font-heading text-lg font-semibold break-words">
                <Link
                  to="/app/organizer/tenders/$tenderId"
                  params={{ tenderId: tender.id }}
                  className="underline-offset-4 hover:underline"
                >
                  {localizedTenderTitle(tender)}
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

      {tenders.hasNextPage && (
        <div>
          <Button
            type="button"
            variant="outline"
            data-testid="org-tenders-more"
            disabled={tenders.isFetchingNextPage}
            onClick={() => void tenders.fetchNextPage()}
          >
            {m.list_load_more()}
          </Button>
        </div>
      )}
    </div>
  )
}
