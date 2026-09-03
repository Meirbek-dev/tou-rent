import { useState } from "react"
import { Link, createFileRoute } from "@tanstack/react-router"
import { useSuspenseQuery } from "@tanstack/react-query"
import { GavelIcon } from "lucide-react"
import { m } from "#/paraglide/messages"
import { EmptyState } from "@/components/empty-state"
import { EvaderRegistry } from "@/components/evader-registry"
import { InvestmentContracts } from "@/components/investment-contracts"
import { PageHeader } from "@/components/page-header"
import { Panel } from "@/components/panel"
import { TenderStatusBadge } from "@/components/tender-status-badge"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { localizedTenderTitle } from "@/lib/api"
import { formatDateTime } from "@/lib/format"
import {
  investmentAttachmentsQuery,
  investmentContractsQuery,
} from "@/lib/investment"
import {
  matchesTenderFilter,
  organizerTendersQuery,
  splitSecretaryWorkList,
} from "@/lib/organizer"

import type { TenderDto, TenderStatus } from "@/lib/api"

// Секретарь: тендеры → журнал регистрации и заявки (FR-402).
export const Route = createFileRoute("/app/secretary/")({
  loader: async ({ context }) => {
    await Promise.all([
      context.queryClient.ensureQueryData(organizerTendersQuery),
      context.queryClient.ensureQueryData(investmentContractsQuery),
      context.queryClient.ensureQueryData(investmentAttachmentsQuery),
    ])
  },
  head: () => ({ meta: [{ title: `${m.cabinet_secretary()} - ToU Rent` }] }),
  component: SecretaryHome,
})

/** Статусы, между которыми секретарю есть смысл выбирать. */
const STATUS_OPTIONS: readonly TenderStatus[] = [
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

/**
 * Чем ближе дело, тем выше строка.
 *
 * Порядок и разделение на «в работе» и «завершенные» живут в
 * `splitSecretaryWorkList` (@/lib/organizer): список шел от самого старого
 * тендера вместе с отмененными и заключенными, и живое заседание оказывалось
 * тридцать восьмой строкой из тридцати девяти.
 */
function SecretaryHome() {
  const { data: page } = useSuspenseQuery(organizerTendersQuery)
  const [status, setStatus] = useState("")
  const [query, setQuery] = useState("")

  const filter = { status, query }
  const { active, completed } = splitSecretaryWorkList(page.items)
  const shownActive = active.filter((t) => matchesTenderFilter(t, filter))
  const shownCompleted = completed.filter((t) => matchesTenderFilter(t, filter))

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        title={m.cabinet_secretary()}
        description={m.secretary_dash_subtitle()}
      />

      <Panel title={m.secretary_tenders_title()} titleAs="h2">
        <div className="flex flex-col gap-4">
          <fieldset className="flex flex-wrap items-end gap-3">
            <legend className="sr-only">{m.tenders_filter_legend()}</legend>
            <div className="flex min-w-48 flex-col gap-1.5">
              <Label htmlFor="secretary-status">
                {m.tenders_filter_status_label()}
              </Label>
              <NativeSelect
                id="secretary-status"
                data-testid="secretary-status"
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
              <Label htmlFor="secretary-query">
                {m.tenders_filter_query_label()}
              </Label>
              <Input
                id="secretary-query"
                data-testid="secretary-query"
                type="search"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
            </div>
          </fieldset>

          {active.length === 0 && completed.length === 0 ? (
            <EmptyState
              icon={GavelIcon}
              title={m.org_tenders_empty()}
              description={m.secretary_tenders_empty_hint()}
            />
          ) : shownActive.length === 0 && shownCompleted.length === 0 ? (
            <p
              className="text-muted-foreground"
              data-testid="secretary-filtered"
            >
              {m.secretary_tenders_filtered_empty()}
            </p>
          ) : (
            <>
              {shownActive.length > 0 && (
                <ul
                  className="flex flex-col gap-2"
                  data-testid="secretary-active"
                >
                  {shownActive.map((tender) => (
                    <TenderRow key={tender.id} tender={tender} />
                  ))}
                </ul>
              )}
              {/* Завершенное - история, а не работа: свернуто, но на месте */}
              {shownCompleted.length > 0 && (
                <details data-testid="secretary-completed">
                  <summary className="cursor-pointer text-sm font-medium">
                    {m.secretary_completed_section({
                      count: shownCompleted.length,
                    })}
                  </summary>
                  <ul className="mt-2 flex flex-col gap-2">
                    {shownCompleted.map((tender) => (
                      <TenderRow key={tender.id} tender={tender} />
                    ))}
                  </ul>
                </details>
              )}
            </>
          )}
        </div>
      </Panel>

      <EvaderRegistry />

      {/* FR-1204 (п. 92): приемку инвестиций оформляет секретарь комиссии */}
      <InvestmentContracts roles={["secretary"]} />
    </div>
  )
}

/** Строка рабочего списка: состояние, название и обе даты тендера. */
function TenderRow({ tender }: { tender: TenderDto }) {
  return (
    <li className="grid overflow-hidden rounded-lg border transition-colors hover:bg-muted/50">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2 p-3">
        <TenderStatusBadge
          status={tender.status}
          deadline={tender.submission_deadline}
        />
        <h3 className="min-w-0 flex-1 font-medium break-words">
          <Link
            to="/app/secretary/tenders/$tenderId"
            params={{ tenderId: tender.id }}
            className="underline-offset-4 hover:underline"
          >
            {localizedTenderTitle(tender)}
          </Link>
        </h3>
        <DateFact label={m.meeting_scheduled_at()} value={tender.opening_at} />
        <DateFact
          label={m.tender_deadline()}
          value={tender.submission_deadline}
        />
      </div>
    </li>
  )
}

/** Дата тендера в строке реестра: подпись сверху, значение моноширинным. */
function DateFact({
  label,
  value,
}: {
  label: string
  value: string | null | undefined
}) {
  return (
    <span className="flex shrink-0 flex-col gap-0.5">
      <span className="text-xs text-muted-foreground">{label}</span>
      <span className="text-sm tabular-nums" suppressHydrationWarning>
        {formatDateTime(value) ?? m.tender_date_tbd()}
      </span>
    </span>
  )
}
