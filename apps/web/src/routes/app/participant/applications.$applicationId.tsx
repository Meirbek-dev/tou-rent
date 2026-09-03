import { useRef, useState } from "react"
import { Link, createFileRoute, notFound } from "@tanstack/react-router"
import { DownloadIcon, FileTextIcon } from "lucide-react"
import {
  useMutation,
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { AmendmentsBanner } from "@/components/amendments-banner"
import { ApplicationStatusBadge } from "@/components/application-status-badge"
import { ConfirmAction } from "@/components/confirm-action"
import { PageHeader } from "@/components/page-header"
import { Panel } from "@/components/panel"
import { QueryBoundary } from "@/components/query-boundary"
import { Badge } from "@/components/ui/badge"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button, buttonVariants } from "@/components/ui/button"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { Skeleton } from "@/components/ui/skeleton"
import { api, tenderQuery } from "@/lib/api"
import { lotAuctionQuery } from "@/lib/auctions"
import { problemMessage } from "@/lib/auth"
import { formatDateTime, formatTenge } from "@/lib/format"
import {
  myApplicationsQuery,
  reasonLabel,
  rejectionReasonsQuery,
} from "@/lib/participant"
import { notifyError, notifySuccess } from "@/lib/toast"
import { cn } from "@/lib/utils"
import { uploadError } from "@/lib/validation"

import type { ApplicationDto } from "@/lib/participant"
import type { ReactNode } from "react"

type ApplicationDocumentKind =
  | "application_form"
  | "registration_certificate"
  | "tax_clearance"
  | "guarantee_payment"
  | "qualification_documents"
  | "price_proposal_form"
  | "qualification_form"

const DOCUMENT_KINDS: readonly ApplicationDocumentKind[] = [
  "application_form",
  "registration_certificate",
  "tax_clearance",
  "guarantee_payment",
  "qualification_documents",
  "price_proposal_form",
  "qualification_form",
]

function documentKindLabel(kind: string): string {
  switch (kind) {
    case "application_form":
      return m.application_document_application_form()
    case "registration_certificate":
      return m.application_document_registration_certificate()
    case "tax_clearance":
      return m.application_document_tax_clearance()
    case "guarantee_payment":
      return m.application_document_guarantee_payment()
    case "qualification_documents":
      return m.application_document_qualification_documents()
    case "price_proposal_form":
      return m.application_document_price_proposal_form()
    case "qualification_form":
      return m.application_document_qualification_form()
    default:
      return m.application_document_legacy()
  }
}

// Карточка своей заявки: файлы (FR-401) и отзыв до дедлайна (FR-404).
export const Route = createFileRoute(
  "/app/participant/applications/$applicationId"
)({
  loader: async ({ context, params }) => {
    const list = await context.queryClient.ensureQueryData(myApplicationsQuery)
    const application = list.find((a) => a.id === params.applicationId)
    if (application === undefined) throw notFound()
    await Promise.all([
      context.queryClient.ensureQueryData(rejectionReasonsQuery),
      // Предмет заявки - лот тендера: без него заголовком страницы остается
      // короткий идентификатор, по которому заявку не узнать
      context.queryClient.ensureQueryData(tenderQuery(application.tender_id)),
    ])
  },
  head: () => ({
    meta: [{ title: `${m.application_card_short()} - ToU Rent` }],
  }),
  component: ApplicationPage,
})

/** Комната лота открыта или уже идет: в обоих случаях участнику туда пора. */
const LIVE_AUCTION_STATUSES = new Set(["scheduled", "running"])

function ApplicationPage() {
  const { applicationId } = Route.useParams()
  const { data: applications } = useSuspenseQuery(myApplicationsQuery)
  const application = applications.find((a) => a.id === applicationId)

  if (application === undefined) throw notFound()
  return <ApplicationCard application={application} />
}

function ApplicationCard({ application }: { application: ApplicationDto }) {
  const queryClient = useQueryClient()
  const fileInput = useRef<HTMLInputElement>(null)
  const [documentKind, setDocumentKind] =
    useState<ApplicationDocumentKind>("application_form")
  // Досье под Object Lock на пять лет (INV-042), поэтому api отвергает файл
  // не того формата или крупнее 10 МБ. Раньше форма об этом не знала, и
  // участник узнавал об отказе, отправив файл целиком
  const [fileError, setFileError] = useState<string | undefined>(undefined)
  const { data: reasons } = useSuspenseQuery(rejectionReasonsQuery)
  const { data: tender } = useSuspenseQuery(tenderQuery(application.tender_id))
  const lot = tender?.lots.find((item) => item.id === application.lot_id)

  const refresh = () =>
    queryClient.invalidateQueries({ queryKey: myApplicationsQuery.queryKey })

  const withdraw = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST(
        "/api/v1/applications/{id}/withdraw",
        { params: { path: { id: application.id } } }
      )
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("withdraw failed")
      }
      return data
    },
    onSuccess: async () => {
      notifySuccess(m.application_detail_withdrawn_toast())
      await refresh()
    },
  })

  const upload = useMutation({
    mutationFn: async () => {
      const file = fileInput.current?.files?.[0]
      if (file === undefined) throw new Error(m.file_not_selected())

      const body = new FormData()
      body.append("file", file)
      const { data, error } = await api.POST(
        "/api/v1/applications/{id}/files",
        {
          params: {
            path: { id: application.id },
            query: { document_kind: documentKind },
          },
          // Контракт описывает multipart как бинарное тело - отдаем FormData как есть
          body: body as unknown as number[],
          bodySerializer: (b: unknown) => b as FormData,
        }
      )
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("upload failed")
      }
      return data
    },
    onSuccess: async () => {
      if (fileInput.current) fileInput.current.value = ""
      setFileError(undefined)
      notifySuccess(m.application_detail_file_uploaded())
      await refresh()
    },
    onError: (error: unknown) => notifyError(problemMessage(error)),
  })

  const isSubmitted = application.status === "submitted"

  return (
    <div className="flex max-w-3xl flex-col gap-6">
      <nav>
        <Link
          to="/app/participant"
          className="text-sm text-muted-foreground underline-offset-4 hover:underline"
        >
          ← {m.back_to_cabinet()}
        </Link>
      </nav>

      <PageHeader
        title={
          tender?.title ??
          m.application_card_title({ id: application.id.slice(0, 8) })
        }
        description={m.application_card_title({
          id: application.id.slice(0, 8),
        })}
        badge={<ApplicationStatusBadge status={application.status} />}
        facts={
          <>
            {lot !== undefined && (
              <Fact label={m.lot_seq()}>
                <span className="tabular-nums">{lot.seq}</span> · {lot.purpose}
              </Fact>
            )}
            <Fact label={m.application_submitted_at()}>
              <span className="tabular-nums" suppressHydrationWarning>
                {formatDateTime(application.submitted_at)}
              </span>
            </Fact>
            <Fact label={m.application_price()}>
              <span className="tabular-nums" suppressHydrationWarning>
                {application.price_amount != null
                  ? formatTenge(application.price_amount)
                  : m.application_price_sealed()}
              </span>
            </Fact>
            {application.withdrawn_at != null && (
              <Fact label={m.application_withdrawn_at()}>
                <span className="tabular-nums" suppressHydrationWarning>
                  {formatDateTime(application.withdrawn_at)}
                </span>
              </Fact>
            )}
            {application.rejection_reason != null && (
              <Fact label={m.rejection_reason_label()}>
                {reasonLabel(reasons, application.rejection_reason)}
              </Fact>
            )}
          </>
        }
        actions={
          isSubmitted ? (
            // Отзыв заявки назад не отыгрывается (FR-404, п. 44)
            <ConfirmAction
              title={m.application_detail_withdraw_confirm_title()}
              description={m.application_detail_withdraw_confirm_description()}
              confirmLabel={m.application_withdraw()}
              onConfirm={() => withdraw.mutate()}
              disabled={withdraw.isPending}
              trigger={
                <Button variant="destructive">
                  {m.application_withdraw()}
                </Button>
              }
            />
          ) : undefined
        }
      />

      {withdraw.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(withdraw.error)}
        </p>
      )}

      {/* FR-304, FR-1004: условия изменились - можно отказаться (п. 26.5) */}
      <AmendmentsBanner
        tenderId={application.tender_id}
        applicationId={
          application.withdrawn_at == null ? application.id : undefined
        }
      />

      {application.status === "admitted" && (
        <AuctionRoomCta lotId={application.lot_id} />
      )}

      <Panel
        title={m.application_files_title()}
        contentClassName="flex flex-col gap-4"
      >
        <RequiredApplicationForms />
        <p className="text-sm text-muted-foreground">
          {application.package_complete
            ? m.application_package_complete()
            : m.application_package_incomplete()}
        </p>
        {application.files.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {m.application_files_empty()}
          </p>
        ) : (
          <ul className="flex flex-col gap-2">
            {application.files.map((file) => (
              <li key={file.id}>
                {tender?.opened_at != null ? (
                  <a
                    href={`/api/v1/applications/${application.id}/files/${file.id}`}
                    className={cn(
                      buttonVariants({ variant: "link" }),
                      "h-auto px-0"
                    )}
                  >
                    {documentKindLabel(file.document_kind)} — {file.filename}
                  </a>
                ) : (
                  <span className="text-sm font-medium">
                    {documentKindLabel(file.document_kind)} — {file.filename}
                  </span>
                )}
                <span className="ml-2 text-sm text-muted-foreground tabular-nums">
                  {Math.ceil(file.size_bytes / 1024)} KiB
                </span>
              </li>
            ))}
          </ul>
        )}

        {isSubmitted && (
          <form
            className="flex flex-wrap items-end gap-3"
            onSubmit={(event) => {
              event.preventDefault()
              const problem = uploadError(fileInput.current?.files?.[0])
              setFileError(problem)
              if (problem !== undefined) return
              upload.mutate()
            }}
          >
            <FieldGroup className="min-w-72 flex-1 gap-3">
              <Field>
                <FieldLabel htmlFor="application-document-kind">
                  {m.application_document_kind()}
                </FieldLabel>
                <NativeSelect
                  id="application-document-kind"
                  value={documentKind}
                  onChange={(event) =>
                    setDocumentKind(
                      event.target.value as ApplicationDocumentKind
                    )
                  }
                >
                  {DOCUMENT_KINDS.map((kind) => (
                    <NativeSelectOption key={kind} value={kind}>
                      {documentKindLabel(kind)}
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
              </Field>
              <Field data-invalid={fileError !== undefined}>
                <FieldLabel htmlFor="application-file">
                  {m.file_upload_label()}
                </FieldLabel>
                <Input
                  id="application-file"
                  type="file"
                  required
                  accept="application/pdf,.pdf"
                  aria-invalid={fileError !== undefined}
                  ref={fileInput}
                  onChange={() => {
                    const file = fileInput.current?.files?.[0]
                    setFileError(
                      file === undefined ? undefined : uploadError(file)
                    )
                  }}
                />
              </Field>
            </FieldGroup>
            <Button type="submit" disabled={upload.isPending}>
              {m.file_upload_submit()}
            </Button>
            {fileError !== undefined && (
              <p role="alert" className="w-full text-sm text-destructive">
                {fileError}
              </p>
            )}
            {upload.isError && (
              <p role="alert" className="w-full text-sm text-destructive">
                {problemMessage(upload.error)}
              </p>
            )}
          </form>
        )}
      </Panel>
    </div>
  )
}

const REQUIRED_FORM_TEMPLATES = [
  {
    href: "/templates/application-appendix-2.docx",
    label: () => m.application_document_application_form(),
    description: () => m.application_appendix_2_description(),
  },
  {
    href: "/templates/price-proposal-appendix-9.docx",
    label: () => m.application_document_price_proposal_form(),
    description: () => m.application_appendix_9_description(),
  },
  {
    href: "/templates/qualification-appendix-11.docx",
    label: () => m.application_document_qualification_form(),
    description: () => m.application_appendix_11_description(),
  },
] as const

function RequiredApplicationForms() {
  return (
    <Alert className="px-4 py-4">
      <FileTextIcon />
      <AlertTitle>{m.application_required_forms_title()}</AlertTitle>
      <AlertDescription className="mt-2 flex flex-col gap-4 text-pretty">
        {REQUIRED_FORM_TEMPLATES.map((template) => (
          <div key={template.href} className="flex flex-col gap-2">
            <p>
              <span className="font-medium text-foreground">
                {template.label()}.
              </span>{" "}
              {template.description()}
            </p>
            <a
              href={template.href}
              download
              className={cn(
                buttonVariants({ variant: "outline", size: "sm" }),
                "w-fit no-underline"
              )}
            >
              <DownloadIcon data-icon="inline-start" />
              {m.application_template_download()}
            </a>
          </div>
        ))}
      </AlertDescription>
    </Alert>
  )
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <span className="flex items-center gap-1.5">
      <span className="text-muted-foreground">{label}:</span>
      <span className="font-medium">{children}</span>
    </span>
  )
}

/**
 * Вход в комнату торгов допущенного участника (FR-601).
 *
 * Раньше это была строчка-ссылка в конце шапки, и день торгов участник
 * пропускал, не заметив ее. Комната открыта или уже идет - это главное,
 * что есть на экране заявки, поэтому здесь она заявлена полосой с кнопкой.
 */
function AuctionRoomCta({ lotId }: { lotId: string }) {
  const auction = useQuery(lotAuctionQuery(lotId))

  return (
    <QueryBoundary
      query={auction}
      skeleton={<Skeleton className="h-20 w-full rounded-xl" />}
    >
      {(data) =>
        data != null && LIVE_AUCTION_STATUSES.has(data.status) ? (
          <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-primary/25 bg-primary/10 px-4 py-3">
            <div className="flex flex-col gap-1">
              <div className="flex flex-wrap items-center gap-2">
                <p className="font-heading text-base font-semibold text-primary">
                  {m.application_detail_auction_title()}
                </p>
                <Badge variant="info">
                  {data.status === "running"
                    ? m.auction_status_running()
                    : m.auction_status_scheduled()}
                </Badge>
              </div>
              <p className="text-sm text-primary/90">
                {m.auction_lot({
                  seq: data.lot_seq,
                  purpose: data.lot_purpose,
                })}
              </p>
            </div>
            <Link
              to="/app/auctions/$auctionId"
              params={{ auctionId: data.id }}
              data-testid="auction-room-link"
              className={cn(buttonVariants())}
            >
              {m.auction_go_to_room()} →
            </Link>
          </div>
        ) : null
      }
    </QueryBoundary>
  )
}
