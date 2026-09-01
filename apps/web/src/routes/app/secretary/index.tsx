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
import { localizedTenderTitle } from "@/lib/api"
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
  head: () => ({ meta: [{ title: `${m.cabinet_secretary()} - ToU Rent` }] }),
  component: SecretaryHome,
})

/**
 * Чем ближе дело, тем выше строка.
 *
 * Ключ сортировки - ближайшая из двух дат тендера: заседание по вскрытию и
 * окончание приема заявок. Именно они и есть работа секретаря, а порядок
 * выдачи реестра (по созданию) к ней отношения не имеет. Сравниваются строки
 * ISO, а не `Date.now()`: порядок обязан совпасть на сервере и в браузере.
 * Тендер без обеих дат уходит вниз, а не выдает себя за самый срочный.
 */
const FAR_FUTURE = "9999"

function nearestDate(tender: {
  opening_at?: string | null | undefined
  submission_deadline?: string | null | undefined
}): string {
  const dates = [tender.opening_at, tender.submission_deadline].filter(
    (value): value is string => value != null
  )
  return dates.length === 0 ? FAR_FUTURE : (dates.toSorted()[0] ?? FAR_FUTURE)
}

function SecretaryHome() {
  const { data: page } = useSuspenseQuery(organizerTendersQuery)
  const relevant = page.items
    .filter((t) => t.status !== "draft")
    .toSorted((left, right) =>
      nearestDate(left).localeCompare(nearestDate(right))
    )

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        title={m.cabinet_secretary()}
        description={m.secretary_dash_subtitle()}
      />

      <Panel title={m.secretary_tenders_title()} titleAs="h2">
        {relevant.length === 0 ? (
          <EmptyState
            icon={GavelIcon}
            title={m.org_tenders_empty()}
            description={m.secretary_tenders_empty_hint()}
          />
        ) : (
          <ul className="flex flex-col gap-2">
            {relevant.map((tender) => (
              <li
                key={tender.id}
                className="grid overflow-hidden rounded-lg border transition-colors hover:bg-muted/50"
              >
                <div className="flex flex-wrap items-center gap-x-4 gap-y-2 p-3">
                  <TenderStatusBadge status={tender.status} />
                  <h3 className="min-w-0 flex-1 font-medium">
                    <Link
                      to="/app/secretary/tenders/$tenderId"
                      params={{ tenderId: tender.id }}
                      className="underline-offset-4 hover:underline"
                    >
                      {localizedTenderTitle(tender)}
                    </Link>
                  </h3>
                  <DateFact
                    label={m.meeting_scheduled_at()}
                    value={tender.opening_at}
                  />
                  <DateFact
                    label={m.tender_deadline()}
                    value={tender.submission_deadline}
                  />
                </div>
              </li>
            ))}
          </ul>
        )}
      </Panel>

      <EvaderRegistry />

      {/* FR-1204 (п. 92): приемку инвестиций оформляет секретарь комиссии */}
      <InvestmentContracts roles={["secretary"]} />
    </div>
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
