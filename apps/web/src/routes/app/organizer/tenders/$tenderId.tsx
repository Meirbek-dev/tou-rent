import { useState } from "react"
import { createFileRoute, notFound } from "@tanstack/react-router"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { AmendmentsBanner } from "@/components/amendments-banner"
import { ContractPanel } from "@/components/contract-panel"
import { DossierPanel } from "@/components/dossier-panel"
import { EvasionPanel } from "@/components/evasion-panel"
import { PageHeader } from "@/components/page-header"
import { Panel } from "@/components/panel"
import { TenderChangesPanel } from "@/components/tender-changes-panel"
import { FailurePanel } from "@/components/failure-panel"
import { TenderStatusBadge } from "@/components/tender-status-badge"
import { Button } from "@/components/ui/button"
import { buttonVariants } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { api, tenderQuery } from "@/lib/api"
import { cancelLot } from "@/lib/amendments"

import type { TenderDto } from "@/lib/api"

type TenderData = TenderDto | null
import { problemMessage } from "@/lib/auth"
import { formatDateTime, formatTenge } from "@/lib/format"
import {
  fromAlmatyInput,
  organizerTendersQuery,
  toAlmatyInput,
} from "@/lib/organizer"
import { tabSearch } from "@/lib/tabs"
import { cn } from "@/lib/utils"

/** Вкладки ведения тендера организатором - в порядке работы над ним. */
const TABS = ["overview", "lots", "changes", "contracts", "risk"] as const

// Управление тендером: даты черновика (PUT), переходы (publish и далее -
// законность решает триггер INV-021/FR-303), PDF объявления (Прил. 1).
//
// Разделы разложены по вкладкам, а открытая вкладка живет в адресе
// (@/lib/tabs): договоры и изменения нужны в разные дни, и держать их в
// одном столбце значило прокручивать мимо них каждый раз.
export const Route = createFileRoute("/app/organizer/tenders/$tenderId")({
  validateSearch: tabSearch(TABS),
  loader: async ({ context, params }) => {
    const tender = await context.queryClient.ensureQueryData(
      tenderQuery(params.tenderId)
    )
    if (tender === null) throw notFound()
  },
  head: () => ({
    meta: [{ title: `${m.page_title_org_tender()} - ToU Rent` }],
  }),
  component: ManageTenderPage,
})

function ManageTenderPage() {
  const { tenderId } = Route.useParams()
  const { data: tender } = useSuspenseQuery(tenderQuery(tenderId))

  if (tender === null) throw notFound()
  return <ManageTender tender={tender} />
}

function ManageTender({ tender }: { tender: NonNullable<TenderData> }) {
  const tenderId = tender.id
  const tab = Route.useSearch().tab ?? "overview"
  const navigate = Route.useNavigate()
  const queryClient = useQueryClient()

  const [deadline, setDeadline] = useState(() =>
    toAlmatyInput(tender.submission_deadline)
  )
  const [opening, setOpening] = useState(() => toAlmatyInput(tender.opening_at))
  const [trading, setTrading] = useState(() => toAlmatyInput(tender.trading_at))
  const [zoomUrl, setZoomUrl] = useState(tender.zoom_url ?? "")

  const refresh = async () => {
    await Promise.all([
      queryClient.invalidateQueries({
        queryKey: tenderQuery(tenderId).queryKey,
      }),
      queryClient.invalidateQueries({
        queryKey: organizerTendersQuery.queryKey,
      }),
    ])
  }

  const saveDates = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.PUT("/api/v1/tenders/{id}", {
        params: { path: { id: tenderId } },
        body: {
          title: tender.title,
          submission_deadline: fromAlmatyInput(deadline),
          opening_at: fromAlmatyInput(opening),
          trading_at: fromAlmatyInput(trading),
          zoom_url: zoomUrl === "" ? null : zoomUrl,
        },
      })
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("failed to update tender")
      }
      return data
    },
    onSuccess: refresh,
  })

  const transition = useMutation({
    mutationFn: async (action: "publish" | "open-acceptance") => {
      const opts = { params: { path: { id: tenderId } } }
      const { data, error } =
        action === "publish"
          ? await api.POST("/api/v1/tenders/{id}/publish", opts)
          : await api.POST("/api/v1/tenders/{id}/open-acceptance", opts)
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("transition failed")
      }
      return data
    },
    onSuccess: refresh,
  })

  const isDraft = tender.status === "draft"

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        title={tender.title}
        badge={<TenderStatusBadge status={tender.status} />}
        facts={
          <>
            <Fact label={m.tender_id_label()} value={tender.id.slice(0, 8)} />
            <Fact
              label={m.tender_deadline()}
              value={formatDateTime(tender.submission_deadline) ?? "-"}
            />
            <Fact
              label={m.tender_lots_title()}
              value={String(tender.lots.length)}
            />
          </>
        }
        actions={
          <>
            {isDraft && (
              <Button
                data-testid="publish-tender"
                onClick={() => transition.mutate("publish")}
                disabled={transition.isPending}
              >
                {m.tender_publish()}
              </Button>
            )}
            {tender.status === "announced" && (
              <Button
                onClick={() => transition.mutate("open-acceptance")}
                disabled={transition.isPending}
              >
                {m.tender_open_acceptance()}
              </Button>
            )}
            <a
              href={`/api/v1/tenders/${tender.id}/announcement.pdf`}
              target="_blank"
              rel="noreferrer"
              className={cn(buttonVariants({ variant: "outline" }))}
            >
              {m.tender_announcement_pdf()}
            </a>
          </>
        }
      />

      {transition.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(transition.error)}
        </p>
      )}

      {/* Новая редакция документации касается всего экрана, а не одной
          вкладки: баннер остается на виду */}
      <AmendmentsBanner tenderId={tenderId} />

      <Tabs
        value={tab}
        onValueChange={(value) => {
          void navigate({
            search: { tab: value as (typeof TABS)[number] },
            replace: true,
          })
        }}
        className="gap-6"
      >
        <TabsList className="max-w-full overflow-x-auto">
          <TabsTrigger value="overview">{m.tab_overview()}</TabsTrigger>
          <TabsTrigger value="lots">{m.tab_lots()}</TabsTrigger>
          <TabsTrigger value="changes">{m.tab_changes()}</TabsTrigger>
          <TabsTrigger value="contracts">{m.tab_contracts()}</TabsTrigger>
          <TabsTrigger value="risk">{m.tab_risk()}</TabsTrigger>
        </TabsList>

        {/* Одна панель на выбранную вкладку: разделы ниже сами ходят
            в сеть, и держать разметку всех пяти ради вкладки, которую
            сейчас не смотрят, незачем */}
        <TabsContent value={tab}>
          {tab === "overview" && (
            <div>
              <Panel title={m.tender_facts_title()} titleAs="h3">
                <dl className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
                  <DateView
                    label={m.tender_announced_at()}
                    value={tender.announced_at}
                  />
                  <DateView
                    label={m.tender_deadline()}
                    value={tender.submission_deadline}
                  />
                  <DateView
                    label={m.tender_opening_at()}
                    value={tender.opening_at}
                  />
                  <DateView
                    label={m.tender_trading_at()}
                    value={tender.trading_at}
                  />
                </dl>
              </Panel>
            </div>
          )}

          {tab === "lots" && (
            <div className="flex flex-col gap-4">
              {isDraft && (
                <Panel
                  title={m.tender_dates_title()}
                  titleAs="h3"
                  description={m.tender_dates_hint()}
                >
                  <form
                    className="flex flex-wrap items-end gap-3"
                    onSubmit={(event) => {
                      event.preventDefault()
                      saveDates.mutate()
                    }}
                  >
                    <div className="flex flex-col gap-1.5">
                      <Label htmlFor="dates-deadline">
                        {m.tender_deadline()}
                      </Label>
                      <Input
                        id="dates-deadline"
                        type="datetime-local"
                        value={deadline}
                        onChange={(event) => setDeadline(event.target.value)}
                      />
                    </div>
                    <div className="flex flex-col gap-1.5">
                      <Label htmlFor="dates-opening">
                        {m.tender_opening_at()}
                      </Label>
                      <Input
                        id="dates-opening"
                        type="datetime-local"
                        value={opening}
                        onChange={(event) => setOpening(event.target.value)}
                      />
                    </div>
                    <div className="flex flex-col gap-1.5">
                      <Label htmlFor="dates-trading">
                        {m.tender_trading_at()}
                      </Label>
                      <Input
                        id="dates-trading"
                        type="datetime-local"
                        value={trading}
                        onChange={(event) => setTrading(event.target.value)}
                      />
                    </div>
                    <div className="flex min-w-64 flex-1 flex-col gap-1.5">
                      <Label htmlFor="dates-zoom">{m.tender_zoom_url()}</Label>
                      <Input
                        id="dates-zoom"
                        type="url"
                        placeholder="https://zoom.us/j/..."
                        value={zoomUrl}
                        onChange={(event) => setZoomUrl(event.target.value)}
                      />
                    </div>
                    <Button
                      type="submit"
                      data-testid="save-dates"
                      disabled={saveDates.isPending}
                    >
                      {m.tender_dates_save()}
                    </Button>
                    {saveDates.isError && (
                      <p
                        role="alert"
                        className="w-full text-sm text-destructive"
                      >
                        {problemMessage(saveDates.error)}
                      </p>
                    )}
                  </form>
                </Panel>
              )}

              <Panel
                title={m.tender_lots_title()}
                titleAs="h3"
                contentClassName="px-0"
              >
                <div className="overflow-x-auto">
                  <Table>
                    <TableHeader>
                      <TableRow>
                        <TableHead scope="col">{m.lot_seq()}</TableHead>
                        <TableHead scope="col">{m.lot_purpose()}</TableHead>
                        <TableHead scope="col">
                          {m.lot_lease_months()}
                        </TableHead>
                        <TableHead scope="col" className="text-right">
                          {m.lot_base_rate()}
                        </TableHead>
                        <TableHead scope="col" className="text-right">
                          {m.lot_guarantee_fee()}
                        </TableHead>
                        <TableHead scope="col">
                          {m.lot_cancel_column()}
                        </TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {tender.lots.map((lot) => (
                        <TableRow key={lot.id}>
                          <TableCell className="tabular-nums">
                            {lot.seq}
                          </TableCell>
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
                          <TableCell className="max-w-64 whitespace-normal">
                            <LotCancellation lot={lot} onChanged={refresh} />
                          </TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                </div>
              </Panel>
            </div>
          )}

          {tab === "changes" && (
            <div>
              {["draft", "announced", "accepting"].includes(tender.status) ? (
                <TenderChangesPanel tenderId={tenderId} onChanged={refresh} />
              ) : (
                <p className="text-muted-foreground">
                  {m.tender_changes_closed()}
                </p>
              )}
            </div>
          )}

          {tab === "contracts" && (
            <div>
              <ContractPanel
                tenderId={tenderId}
                lots={tender.lots.map((lot) => ({ id: lot.id, seq: lot.seq }))}
                canDraft={["summed_up", "contracted"].includes(tender.status)}
              />
            </div>
          )}

          {tab === "risk" && (
            <div className="flex flex-col gap-4">
              <FailurePanel
                tenderId={tenderId}
                canDeclare={false}
                canRepeat
                onChanged={refresh}
              />
              <EvasionPanel tenderId={tenderId} canGenerateProtocol={false} />
              <DossierPanel subject={{ kind: "tender", id: tenderId }} />
            </div>
          )}
        </TabsContent>
      </Tabs>
    </div>
  )
}

/**
 * Отмена отдельного лота (FR-305, п. 78): тендер продолжается, объект лота
 * освобождается, взносы по лоту идут на возврат. Причина обязательна.
 */
function LotCancellation({
  lot,
  onChanged,
}: {
  lot: NonNullable<TenderData>["lots"][number]
  onChanged: () => Promise<void>
}) {
  const [reason, setReason] = useState("")
  const cancel = useMutation({
    mutationFn: () => cancelLot(lot.id, reason),
    onSuccess: async () => {
      setReason("")
      await onChanged()
    },
  })

  if (lot.cancelled_at != null) {
    return (
      <span className="text-sm text-muted-foreground">
        {m.lot_cancelled()}
        {lot.cancel_reason != null && `: ${lot.cancel_reason}`}
      </span>
    )
  }

  return (
    <form
      className="flex items-center gap-2"
      onSubmit={(event) => {
        event.preventDefault()
        cancel.mutate()
      }}
    >
      <Input
        aria-label={m.cancel_reason_label()}
        className="max-w-40"
        placeholder={m.cancel_reason_label()}
        value={reason}
        onChange={(event) => setReason(event.target.value)}
      />
      <Button
        type="submit"
        variant="outline"
        size="sm"
        data-testid="cancel-lot"
        disabled={cancel.isPending || reason === ""}
      >
        {m.lot_cancel()}
      </Button>
    </form>
  )
}

/** Пара «подпись - значение» в строке ключевых сведений под заголовком. */
function Fact({ label, value }: { label: string; value: string }) {
  return (
    <span className="flex items-baseline gap-1.5">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-medium tabular-nums" suppressHydrationWarning>
        {value}
      </span>
    </span>
  )
}

function DateView({
  label,
  value,
}: {
  label: string
  value?: string | null | undefined
}) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-sm text-muted-foreground">{label}</dt>
      <dd className="font-medium tabular-nums" suppressHydrationWarning>
        {formatDateTime(value) ?? m.tender_date_tbd()}
      </dd>
    </div>
  )
}
