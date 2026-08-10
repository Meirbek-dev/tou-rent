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
import { TenderChangesPanel } from "@/components/tender-changes-panel"
import { FailurePanel } from "@/components/failure-panel"
import { TenderStatusBadge } from "@/components/tender-status-badge"
import { Button } from "@/components/ui/button"
import { buttonVariants } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
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
import { cn } from "@/lib/utils"

// Управление тендером: даты черновика (PUT), переходы (publish и далее -
// законность решает триггер INV-021/FR-303), PDF объявления (Прил. 1).
export const Route = createFileRoute("/app/organizer/tenders/$tenderId")({
  loader: async ({ context, params }) => {
    const tender = await context.queryClient.ensureQueryData(
      tenderQuery(params.tenderId)
    )
    if (tender === null) throw notFound()
  },
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
    <div className="flex flex-col gap-8">
      <header className="flex flex-col gap-2">
        <div className="flex flex-wrap items-center gap-3">
          <TenderStatusBadge status={tender.status} />
          <span className="text-sm text-muted-foreground">
            {m.tender_card_title({ id: tender.id.slice(0, 8) })}
          </span>
        </div>
        <h2 className="font-heading text-2xl font-semibold">{tender.title}</h2>
        <div className="flex flex-wrap gap-3 pt-1">
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
        </div>
        {transition.isError && (
          <p role="alert" className="text-sm text-destructive">
            {problemMessage(transition.error)}
          </p>
        )}
      </header>

      <FailurePanel
        tenderId={tenderId}
        canDeclare={false}
        canRepeat
        onChanged={refresh}
      />

      <AmendmentsBanner tenderId={tenderId} />

      {["draft", "announced", "accepting"].includes(tender.status) && (
        <TenderChangesPanel tenderId={tenderId} onChanged={refresh} />
      )}

      <EvasionPanel tenderId={tenderId} canGenerateProtocol={false} />

      <ContractPanel
        tenderId={tenderId}
        lots={tender.lots.map((lot) => ({ id: lot.id, seq: lot.seq }))}
        canDraft={["summed_up", "contracted"].includes(tender.status)}
      />

      <DossierPanel subject={{ kind: "tender", id: tenderId }} />

      <section aria-labelledby="manage-dates">
        <h3
          id="manage-dates"
          className="mb-3 font-heading text-lg font-semibold"
        >
          {m.tender_dates_title()}
        </h3>
        {isDraft ? (
          <form
            className="flex flex-wrap items-end gap-3 rounded-lg border p-4"
            onSubmit={(event) => {
              event.preventDefault()
              saveDates.mutate()
            }}
          >
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="dates-deadline">{m.tender_deadline()}</Label>
              <Input
                id="dates-deadline"
                type="datetime-local"
                value={deadline}
                onChange={(event) => setDeadline(event.target.value)}
              />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="dates-opening">{m.tender_opening_at()}</Label>
              <Input
                id="dates-opening"
                type="datetime-local"
                value={opening}
                onChange={(event) => setOpening(event.target.value)}
              />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="dates-trading">{m.tender_trading_at()}</Label>
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
            <p className="w-full text-sm text-muted-foreground">
              {m.tender_dates_hint()}
            </p>
            {saveDates.isError && (
              <p role="alert" className="w-full text-sm text-destructive">
                {problemMessage(saveDates.error)}
              </p>
            )}
          </form>
        ) : (
          <dl className="grid grid-cols-1 gap-4 rounded-lg border p-4 sm:grid-cols-2 lg:grid-cols-4">
            <DateView
              label={m.tender_announced_at()}
              value={tender.announced_at}
            />
            <DateView
              label={m.tender_deadline()}
              value={tender.submission_deadline}
            />
            <DateView label={m.tender_opening_at()} value={tender.opening_at} />
            <DateView label={m.tender_trading_at()} value={tender.trading_at} />
          </dl>
        )}
      </section>

      <section aria-labelledby="manage-lots">
        <h3
          id="manage-lots"
          className="mb-3 font-heading text-lg font-semibold"
        >
          {m.tender_lots_title()}
        </h3>
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
                <TableHead scope="col">{m.lot_cancel_column()}</TableHead>
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
                  <TableCell className="max-w-64 whitespace-normal">
                    <LotCancellation lot={lot} onChanged={refresh} />
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      </section>
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

function DateView({ label, value }: { label: string; value?: string | null }) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-sm text-muted-foreground">{label}</dt>
      <dd className="font-medium" suppressHydrationWarning>
        {formatDateTime(value) ?? m.tender_date_tbd()}
      </dd>
    </div>
  )
}
