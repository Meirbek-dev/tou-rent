import { Link, createFileRoute, notFound } from "@tanstack/react-router"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { DossierPanel } from "@/components/dossier-panel"
import { EvasionPanel } from "@/components/evasion-panel"
import { ProtocolsPanel } from "@/components/protocols-panel"
import { FailurePanel } from "@/components/failure-panel"
import { ApplicationStatusBadge } from "@/components/application-status-badge"
import { AuctionLotsPanel } from "@/components/auction-lots-panel"
import { DecisionForm } from "@/components/decision-form"
import { MeetingPanel } from "@/components/meeting-panel"
import { PageHeader } from "@/components/page-header"
import { Panel } from "@/components/panel"
import { TenderStatusBadge } from "@/components/tender-status-badge"
import { Button, buttonVariants } from "@/components/ui/button"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { api, localizedTenderTitle, tenderQuery } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import { formatDateTime, formatTenge } from "@/lib/format"
import {
  admissionProtocolQuery,
  meetingQuery,
  reasonLabel,
  rejectionReasonsQuery,
  tenderApplicationsQuery,
  tenderJournalQuery,
} from "@/lib/participant"
import { tabSearch } from "@/lib/tabs"
import { cn } from "@/lib/utils"

import type { MeetingDto } from "@/lib/participant"
import type { ReactNode } from "react"

/**
 * Вкладки экрана ведения тендера. Порядок - порядок работы заседания:
 * сперва «что это за тендер», потом заседание, заявки, протоколы, торги,
 * и последним - то, что понадобится, если что-то пошло не так.
 */
const TABS = [
  "overview",
  "meeting",
  "applications",
  "protocols",
  "auction",
  "risk",
] as const

// Экран заседания секретаря (FR-501–503, FR-1102, FR-1104): явка и открытие
// заседания при кворуме, отводы по конфликту интересов, вскрытие, оглашение
// цен, фиксация решений по итогам голосования комиссии, протокол допуска.
//
// Тринадцать разделов стояли в одном столбце: чтобы отметить явку, секретарь
// прокручивал журнал, таблицу заявок, протоколы, досье и реестр уклонений.
// Теперь это вкладки, а вкладка живет в адресе (@/lib/tabs) - ссылка на
// нужный раздел существует, обновление страницы его не теряет.
export const Route = createFileRoute("/app/secretary/tenders/$tenderId")({
  validateSearch: tabSearch(TABS),
  loader: async ({ context, params }) => {
    const tender = await context.queryClient.ensureQueryData(
      tenderQuery(params.tenderId)
    )
    if (tender === null) throw notFound()
    await Promise.all([
      context.queryClient.ensureQueryData(tenderJournalQuery(params.tenderId)),
      context.queryClient.ensureQueryData(
        tenderApplicationsQuery(params.tenderId)
      ),
      context.queryClient.ensureQueryData(meetingQuery(params.tenderId)),
      context.queryClient.ensureQueryData(
        admissionProtocolQuery(params.tenderId)
      ),
      context.queryClient.ensureQueryData(rejectionReasonsQuery),
    ])
  },
  head: () => ({
    meta: [{ title: `${m.page_title_secretary_tender()} - ToU Rent` }],
  }),
  component: SecretaryTenderPage,
})

const ENTRY_KIND_LABELS: Record<string, () => string> = {
  application_submitted: m.journal_kind_submitted,
  application_withdrawn: m.journal_kind_withdrawn,
}

function SecretaryTenderPage() {
  const { tenderId } = Route.useParams()
  const tab = Route.useSearch().tab ?? "overview"
  const navigate = Route.useNavigate()
  const queryClient = useQueryClient()
  const { data: tender } = useSuspenseQuery(tenderQuery(tenderId))
  const { data: journal } = useSuspenseQuery(tenderJournalQuery(tenderId))
  const { data: applications } = useSuspenseQuery(
    tenderApplicationsQuery(tenderId)
  )
  const { data: meeting } = useSuspenseQuery(meetingQuery(tenderId))
  const { data: protocol } = useSuspenseQuery(admissionProtocolQuery(tenderId))
  const { data: reasons } = useSuspenseQuery(rejectionReasonsQuery)

  if (tender === null) throw notFound()
  const sealed = tender.opened_at == null

  const refresh = async () => {
    await Promise.all([
      queryClient.invalidateQueries({
        queryKey: tenderQuery(tenderId).queryKey,
      }),
      queryClient.invalidateQueries({
        queryKey: tenderApplicationsQuery(tenderId).queryKey,
      }),
      queryClient.invalidateQueries({
        queryKey: meetingQuery(tenderId).queryKey,
      }),
      queryClient.invalidateQueries({
        queryKey: admissionProtocolQuery(tenderId).queryKey,
      }),
    ])
  }

  const open = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST("/api/v1/tenders/{id}/open", {
        params: { path: { id: tenderId } },
      })
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("opening failed")
      }
      return data
    },
    onSuccess: refresh,
  })

  const generateProtocol = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST(
        "/api/v1/tenders/{id}/admission-protocol",
        { params: { path: { id: tenderId } } }
      )
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("protocol generation failed")
      }
      return data
    },
    onSuccess: refresh,
  })

  // FR-504: после протокола - уведомление допущенных (стартовая ставка,
  // дата торгов); повторная рассылка отклоняется API (409)
  const notifyAdmitted = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST(
        "/api/v1/tenders/{id}/notify-admitted",
        { params: { path: { id: tenderId } } }
      )
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("notify failed")
      }
      return data
    },
    onSuccess: refresh,
  })

  const decidable = applications.filter(
    (a) => a.status === "submitted" || a.status === "fee_confirmed"
  )

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        title={localizedTenderTitle(tender)}
        badge={<TenderStatusBadge status={tender.status} />}
        breadcrumb={
          <nav>
            <Link
              to="/app/secretary"
              className="text-sm text-muted-foreground underline-offset-4 hover:underline"
            >
              ← {m.back_to_cabinet()}
            </Link>
          </nav>
        }
        facts={
          <>
            <Fact label={m.tender_id_label()} value={tender.id.slice(0, 8)} />
            <Fact
              label={m.tender_deadline()}
              value={formatDateTime(tender.submission_deadline) ?? "-"}
            />
            <Fact
              label={m.meeting_scheduled_at()}
              value={formatDateTime(tender.opening_at) ?? "-"}
            />
            <Fact
              label={m.tender_trading_label()}
              value={formatDateTime(tender.trading_at) ?? "-"}
            />
          </>
        }
        actions={
          <>
            {tender.status === "accepting" && (
              <Button
                data-testid="open-tender"
                onClick={() => open.mutate()}
                disabled={open.isPending || meeting?.opened_at == null}
                title={
                  meeting?.opened_at == null
                    ? m.meeting_open_first()
                    : undefined
                }
              >
                {m.opening_button()}
              </Button>
            )}
            {!sealed && protocol === null && (
              <Button
                variant="outline"
                data-testid="generate-admission-protocol"
                onClick={() => generateProtocol.mutate()}
                disabled={generateProtocol.isPending}
              >
                {m.protocol_generate()}
              </Button>
            )}
            {protocol !== null && (
              <a
                href={`/api/v1/tenders/${tender.id}/admission-protocol.pdf`}
                data-testid="admission-protocol-pdf"
                target="_blank"
                rel="noreferrer"
                className={cn(buttonVariants({ variant: "outline" }))}
              >
                {m.protocol_pdf({ number: protocol.number ?? "" })}
              </a>
            )}
            {protocol !== null && (
              <Button
                variant="outline"
                data-testid="notify-admitted"
                onClick={() => notifyAdmitted.mutate()}
                disabled={notifyAdmitted.isPending}
              >
                {m.notify_admitted_button()}
              </Button>
            )}
          </>
        }
      />

      {/* Исходы действий шапки: они относятся ко всему экрану, а не
          к открытой сейчас вкладке */}
      {notifyAdmitted.isSuccess && (
        <p className="text-sm text-muted-foreground" suppressHydrationWarning>
          {m.notify_admitted_done({
            count: notifyAdmitted.data.notified,
            date: formatDateTime(notifyAdmitted.data.trading_at) ?? "-",
          })}
        </p>
      )}
      {notifyAdmitted.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(notifyAdmitted.error)}
        </p>
      )}
      {open.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(open.error)}
        </p>
      )}
      {generateProtocol.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(generateProtocol.error)}
        </p>
      )}

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
          <TabsTrigger value="meeting">{m.tab_meeting()}</TabsTrigger>
          <TabsTrigger value="applications">{m.tab_applications()}</TabsTrigger>
          <TabsTrigger value="protocols">{m.tab_protocols()}</TabsTrigger>
          <TabsTrigger value="auction">{m.tab_auction()}</TabsTrigger>
          <TabsTrigger value="risk">{m.tab_risk()}</TabsTrigger>
        </TabsList>

        {/* Одна панель на выбранную вкладку: разделы ниже сами ходят
            в сеть, и держать разметку всех шести ради вкладки, которую
            сейчас не смотрят, незачем */}
        <TabsContent value={tab}>
          {tab === "overview" && (
            <div className="flex flex-col gap-4">
              <Panel title={m.tender_facts_title()} titleAs="h3">
                <dl className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
                  <Term
                    label={m.tender_deadline()}
                    value={formatDateTime(tender.submission_deadline) ?? "-"}
                    mono
                  />
                  <Term
                    label={m.meeting_scheduled_at()}
                    value={formatDateTime(tender.opening_at) ?? "-"}
                    mono
                  />
                  <Term
                    label={m.tender_opened_label()}
                    value={
                      formatDateTime(tender.opened_at) ?? m.tender_date_tbd()
                    }
                    mono
                  />
                  <Term
                    label={m.tender_trading_label()}
                    value={formatDateTime(tender.trading_at) ?? "-"}
                    mono
                  />
                  <Term
                    label={m.tender_lots_title()}
                    value={String(tender.lots.length)}
                    mono
                  />
                  <Term
                    label={m.tender_applications_title()}
                    value={String(applications.length)}
                    mono
                  />
                </dl>
              </Panel>

              <Panel title={m.meeting_title()} titleAs="h3">
                <dl className="grid grid-cols-1 gap-4 sm:grid-cols-3">
                  <Term
                    label={m.meeting_opened_at()}
                    value={
                      formatDateTime(meeting?.opened_at) ??
                      m.meeting_not_opened()
                    }
                    mono
                  />
                  <Term
                    label={m.meeting_quorum()}
                    value={quorumLabel(meeting)}
                    mono
                  />
                  <Term
                    label={m.attendance_legend()}
                    value={String(
                      (meeting?.attendance ?? []).filter((row) => row.present)
                        .length
                    )}
                    mono
                  />
                </dl>
              </Panel>
            </div>
          )}

          {tab === "meeting" && (
            <div className="flex flex-col gap-4">
              <MeetingPanel
                tenderId={tenderId}
                meeting={meeting}
                onChanged={refresh}
              />

              <Panel
                title={m.journal_title()}
                titleAs="h3"
                contentClassName="px-0"
              >
                {journal.length === 0 ? (
                  <p className="px-(--card-spacing) text-muted-foreground">
                    {m.journal_empty()}
                  </p>
                ) : (
                  <div className="overflow-x-auto">
                    <Table>
                      <TableHeader>
                        <TableRow>
                          <TableHead scope="col">{m.journal_seq()}</TableHead>
                          <TableHead scope="col">{m.journal_kind()}</TableHead>
                          <TableHead scope="col">{m.journal_time()}</TableHead>
                          <TableHead scope="col">
                            {m.journal_application()}
                          </TableHead>
                        </TableRow>
                      </TableHeader>
                      <TableBody>
                        {journal.map((entry) => (
                          <TableRow key={entry.seq}>
                            <TableCell className="font-medium tabular-nums">
                              {entry.seq}
                            </TableCell>
                            <TableCell>
                              {ENTRY_KIND_LABELS[entry.entry_kind]?.() ??
                                entry.entry_kind}
                            </TableCell>
                            <TableCell
                              className="tabular-nums"
                              suppressHydrationWarning
                            >
                              {formatDateTime(entry.occurred_at)}
                            </TableCell>
                            <TableCell className="text-muted-foreground tabular-nums">
                              {entry.application_id?.slice(0, 8) ?? "-"}
                            </TableCell>
                          </TableRow>
                        ))}
                      </TableBody>
                    </Table>
                  </div>
                )}
              </Panel>
            </div>
          )}

          {tab === "applications" && (
            <div className="flex flex-col gap-4">
              <Panel
                title={m.tender_applications_title()}
                titleAs="h3"
                description={sealed ? m.prices_sealed_note() : undefined}
                contentClassName="px-0"
              >
                {applications.length === 0 ? (
                  <p className="px-(--card-spacing) text-muted-foreground">
                    {m.tender_applications_empty()}
                  </p>
                ) : (
                  <div className="overflow-x-auto">
                    <Table>
                      <TableHeader>
                        <TableRow>
                          <TableHead scope="col">
                            {m.application_card_short()}
                          </TableHead>
                          <TableHead scope="col">
                            {m.object_status_label()}
                          </TableHead>
                          <TableHead scope="col">
                            {m.application_submitted_at()}
                          </TableHead>
                          <TableHead scope="col" className="text-right">
                            {m.application_price()}
                          </TableHead>
                          <TableHead scope="col">
                            {m.application_files_title()}
                          </TableHead>
                        </TableRow>
                      </TableHeader>
                      <TableBody>
                        {applications.map((application) => (
                          <TableRow key={application.id}>
                            <TableCell className="font-medium tabular-nums">
                              {application.id.slice(0, 8)}
                            </TableCell>
                            <TableCell>
                              <div className="flex flex-col gap-1">
                                <ApplicationStatusBadge
                                  status={application.status}
                                />
                                {application.rejection_reason != null && (
                                  <span className="text-xs text-muted-foreground">
                                    {reasonLabel(
                                      reasons,
                                      application.rejection_reason
                                    )}
                                  </span>
                                )}
                              </div>
                            </TableCell>
                            <TableCell
                              className="tabular-nums"
                              suppressHydrationWarning
                            >
                              {formatDateTime(application.submitted_at)}
                            </TableCell>
                            <TableCell
                              className="text-right tabular-nums"
                              suppressHydrationWarning
                            >
                              {application.price_amount != null
                                ? formatTenge(application.price_amount)
                                : m.application_price_sealed()}
                            </TableCell>
                            <TableCell>
                              {application.files.length === 0 ? (
                                <span className="text-muted-foreground">0</span>
                              ) : sealed ? (
                                <span className="text-muted-foreground">
                                  {application.files.length}
                                </span>
                              ) : (
                                <div className="flex flex-col gap-0.5">
                                  {application.files.map((file) => (
                                    <a
                                      key={file.id}
                                      href={`/api/v1/applications/${application.id}/files/${file.id}`}
                                      className="text-sm underline-offset-4 hover:underline"
                                    >
                                      {file.filename}
                                    </a>
                                  ))}
                                </div>
                              )}
                            </TableCell>
                          </TableRow>
                        ))}
                      </TableBody>
                    </Table>
                  </div>
                )}
              </Panel>

              {meeting !== null && decidable.length > 0 && (
                <section
                  aria-labelledby="decisions"
                  className="flex flex-col gap-4"
                >
                  <h3
                    id="decisions"
                    className="font-heading text-lg font-semibold"
                  >
                    {m.decisions_title()}
                  </h3>
                  {decidable.map((application) => (
                    <DecisionForm
                      key={application.id}
                      application={application}
                      reasons={reasons}
                      onDecided={refresh}
                    />
                  ))}
                </section>
              )}
            </div>
          )}

          {tab === "protocols" && (
            <div>
              <ProtocolsPanel tenderId={tenderId} canPublish />
            </div>
          )}

          {tab === "auction" && (
            <div>
              {protocol === null ? (
                <p className="text-muted-foreground">
                  {m.auction_needs_protocol()}
                </p>
              ) : (
                <AuctionLotsPanel tenderId={tenderId} lots={tender.lots} />
              )}
            </div>
          )}

          {tab === "risk" && (
            <div className="flex flex-col gap-4">
              <DossierPanel subject={{ kind: "tender", id: tenderId }} />
              <EvasionPanel tenderId={tenderId} canGenerateProtocol />
              <FailurePanel
                tenderId={tenderId}
                canDeclare
                canRepeat={false}
                onChanged={refresh}
              />
            </div>
          )}
        </TabsContent>
      </Tabs>
    </div>
  )
}

/** Кворум одной строкой: «нет заседания» и «кворум не собран» - разные вещи. */
function quorumLabel(meeting: MeetingDto | null): string {
  if (meeting?.quorum_present == null) return m.meeting_not_opened()
  return m.meeting_quorum_value({
    present: meeting.quorum_present,
    required: meeting.quorum_required ?? 0,
  })
}

/** Пара «подпись - значение» в строке ключевых сведений под заголовком. */
function Fact({ label, value }: { label: string; value: ReactNode }) {
  return (
    <span className="flex items-baseline gap-1.5">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-medium tabular-nums" suppressHydrationWarning>
        {value}
      </span>
    </span>
  )
}

/** Строка списка определений на вкладке обзора. */
function Term({
  label,
  value,
  mono = false,
}: {
  label: string
  value: ReactNode
  mono?: boolean
}) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-sm text-muted-foreground">{label}</dt>
      <dd
        className={cn("font-medium", mono && "tabular-nums")}
        suppressHydrationWarning
      >
        {value}
      </dd>
    </div>
  )
}
