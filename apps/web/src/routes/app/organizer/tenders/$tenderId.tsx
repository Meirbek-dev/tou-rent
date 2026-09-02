import { useRef, useState } from "react"
import { createFileRoute, notFound } from "@tanstack/react-router"
import {
  useMutation,
  useQueryClient,
  useSuspenseQueries,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { getLocale } from "#/paraglide/runtime"
import { AmendmentsBanner } from "@/components/amendments-banner"
import { ContractPanel } from "@/components/contract-panel"
import { DossierPanel } from "@/components/dossier-panel"
import { EvasionPanel } from "@/components/evasion-panel"
import { ConfirmAction } from "@/components/confirm-action"
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
import {
  api,
  localizedTenderTitle,
  objectQuery,
  tenderDocumentsQuery,
  tenderQuery,
} from "@/lib/api"
import { cancelLot } from "@/lib/amendments"

import type { ObjectDto, TenderDto } from "@/lib/api"

type TenderData = TenderDto | null
import { problemMessage } from "@/lib/auth"
import { formatDateTime, formatTenge, trimZeros } from "@/lib/format"
import {
  fromAlmatyInput,
  objectsQuery,
  organizerTendersQuery,
  toAlmatyInput,
} from "@/lib/organizer"
import { tabSearch } from "@/lib/tabs"
import { notifySuccess } from "@/lib/toast"
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
    await Promise.all([
      context.queryClient.ensureQueryData(
        tenderDocumentsQuery(params.tenderId)
      ),
      // Справочник объектов нужен вкладке изменений: лоты новой редакции
      // выбираются из него (FR-304). Запрос общий на весь кабинет, поэтому
      // на других вкладках он берется из кеша
      context.queryClient.ensureQueryData(objectsQuery),
      ...tender.lots.map((lot) =>
        context.queryClient.ensureQueryData(objectQuery(lot.object_id))
      ),
    ])
  },
  head: () => ({
    meta: [{ title: `${m.page_title_org_tender()} - ToU Rent` }],
  }),
  component: ManageTenderPage,
})

function ManageTenderPage() {
  const { tenderId } = Route.useParams()
  const { data: tender } = useSuspenseQuery(tenderQuery(tenderId))
  const { data: documents } = useSuspenseQuery(tenderDocumentsQuery(tenderId))

  if (tender === null) throw notFound()
  return <ManageTenderData tender={tender} documents={documents} />
}

function ManageTenderData({
  tender,
  documents,
}: {
  tender: NonNullable<TenderData>
  documents: Array<{ id: string; title: string; version: number }>
}) {
  const objectResults = useSuspenseQueries({
    queries: tender.lots.map((lot) => objectQuery(lot.object_id)),
  })
  const objectsById = new Map<string, ObjectDto>()
  for (const result of objectResults) {
    if (result.data !== null) objectsById.set(result.data.id, result.data)
  }

  return (
    <ManageTender
      tender={tender}
      documents={documents}
      objectsById={objectsById}
    />
  )
}

function ManageTender({
  tender,
  documents,
  objectsById,
}: {
  tender: NonNullable<TenderData>
  documents: Array<{ id: string; title: string; version: number }>
  objectsById: ReadonlyMap<string, ObjectDto>
}) {
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
  const [documentTitle, setDocumentTitle] = useState("")
  const documentInput = useRef<HTMLInputElement>(null)

  const refresh = async () => {
    await Promise.all([
      queryClient.invalidateQueries({
        queryKey: tenderQuery(tenderId).queryKey,
      }),
      queryClient.invalidateQueries({
        queryKey: organizerTendersQuery.queryKey,
      }),
      queryClient.invalidateQueries({
        queryKey: tenderDocumentsQuery(tenderId).queryKey,
      }),
    ])
  }

  const saveDates = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.PUT("/api/v1/tenders/{id}", {
        params: { path: { id: tenderId } },
        body: {
          title: tender.title,
          title_kk: tender.title_kk,
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
    onSuccess: async () => {
      // Новые сроки делают прежний отказ публикации неактуальным.
      // Сбрасываем состояние отдельной мутации сразу, без перезагрузки страницы.
      transition.reset()
      await refresh()
    },
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

  // Удалить можно только черновик (FR-301): объявленный тендер отменяют
  // (FR-305, п. 78), сервер такой запрос отклонит. После удаления страницы
  // тендера больше нет - уходим в список
  const removeDraft = useMutation({
    mutationFn: async () => {
      const { error } = await api.DELETE("/api/v1/tenders/{id}", {
        params: { path: { id: tenderId } },
      })
      if (error !== undefined) throw error
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: organizerTendersQuery.queryKey,
      })
      notifySuccess(m.tender_draft_deleted_toast())
      await navigate({ to: "/app/organizer/tenders" })
    },
  })

  const uploadDocument = useMutation({
    mutationFn: async () => {
      const file = documentInput.current?.files?.[0]
      if (file === undefined) throw new Error(m.file_not_selected())
      const form = new FormData()
      form.append("file", file)
      const { data, error } = await api.POST("/api/v1/tenders/{id}/documents", {
        params: {
          path: { id: tenderId },
          query: { title: documentTitle.trim() },
        },
        body: form as unknown as number[],
        bodySerializer: (body: unknown) => body as FormData,
      })
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("document upload failed")
      }
      return data
    },
    onSuccess: async () => {
      setDocumentTitle("")
      if (documentInput.current) documentInput.current.value = ""
      await refresh()
    },
  })

  const isDraft = tender.status === "draft"

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        title={localizedTenderTitle(tender)}
        description={isDraft ? m.tender_draft_visibility_hint() : undefined}
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
            {/* Удаление стоит рядом с публикацией и только у черновика:
                объявленный тендер отсюда исчезает, и остается отмена */}
            {isDraft && (
              <ConfirmAction
                title={m.tender_draft_delete_confirm_title()}
                description={m.tender_draft_delete_confirm_description()}
                confirmLabel={m.tender_draft_delete()}
                variant="destructive-solid"
                disabled={removeDraft.isPending}
                onConfirm={() => removeDraft.mutate()}
                trigger={
                  <Button
                    type="button"
                    variant="destructive"
                    data-testid="delete-tender"
                  >
                    {m.tender_draft_delete()}
                  </Button>
                }
              />
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
      {removeDraft.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(removeDraft.error)}
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
        <TabsList className="max-w-full overflow-x-auto overflow-y-hidden">
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
            <div className="flex flex-col gap-4">
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
              <Panel title={m.tender_docs_title()} titleAs="h3">
                {isDraft && (
                  <form
                    className="mb-4 flex flex-wrap items-end gap-3"
                    onSubmit={(event) => {
                      event.preventDefault()
                      uploadDocument.mutate()
                    }}
                  >
                    <div className="flex min-w-56 flex-1 flex-col gap-1.5">
                      <Label htmlFor="tender-document-title">
                        {m.tender_document_title_label()}
                      </Label>
                      <Input
                        id="tender-document-title"
                        required
                        maxLength={300}
                        value={documentTitle}
                        onChange={(event) =>
                          setDocumentTitle(event.target.value)
                        }
                      />
                    </div>
                    <div className="flex min-w-56 flex-1 flex-col gap-1.5">
                      <Label htmlFor="tender-document-file">
                        {m.tender_document_pdf_label()}
                      </Label>
                      <Input
                        ref={documentInput}
                        id="tender-document-file"
                        type="file"
                        accept="application/pdf,.pdf"
                        required
                      />
                    </div>
                    <Button type="submit" disabled={uploadDocument.isPending}>
                      {m.tender_document_upload()}
                    </Button>
                    {uploadDocument.isError && (
                      <p
                        role="alert"
                        className="w-full text-sm text-destructive"
                      >
                        {problemMessage(uploadDocument.error)}
                      </p>
                    )}
                  </form>
                )}
                {documents.length === 0 ? (
                  <p className="text-sm text-muted-foreground">
                    {m.tender_docs_empty()}
                  </p>
                ) : (
                  <ul className="grid gap-2">
                    {documents.map((document) => (
                      <li key={document.id}>
                        <a
                          href={`/api/v1/tenders/${tenderId}/documents/${document.id}`}
                          target="_blank"
                          rel="noreferrer"
                          className="text-sm text-primary underline underline-offset-4"
                        >
                          {document.title} · v{document.version}
                        </a>
                      </li>
                    ))}
                  </ul>
                )}
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
                        <TableHead scope="col">
                          {m.object_name_label()}
                        </TableHead>
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
                          <TableCell className="min-w-64 whitespace-normal">
                            <LotObjectDetails
                              object={objectsById.get(lot.object_id)}
                            />
                          </TableCell>
                          <TableCell className="max-w-md whitespace-normal">
                            {getLocale() === "kk"
                              ? lot.purpose_kk
                              : lot.purpose}
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
                <TenderChangesPanel
                  tenderId={tenderId}
                  lotCount={tender.lots.length}
                  openingAt={tender.opening_at ?? null}
                  tradingAt={tender.trading_at ?? null}
                  onChanged={refresh}
                />
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

function LotObjectDetails({ object }: { object: ObjectDto | undefined }) {
  if (object === undefined) return <span aria-hidden="true">—</span>

  const kazakh = getLocale() === "kk"
  return (
    <div className="flex flex-col gap-0.5">
      <span className="font-medium">
        {kazakh ? object.name_kk : object.name}
      </span>
      <span className="text-sm text-muted-foreground">
        {kazakh ? object.address_kk : object.address}
      </span>
      <span className="text-sm text-muted-foreground tabular-nums">
        {m.object_area_value({ area: trimZeros(object.area_m2) })}
      </span>
    </div>
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
