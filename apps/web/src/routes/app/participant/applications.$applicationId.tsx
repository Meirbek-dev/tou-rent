import { useRef } from "react"
import { Link, createFileRoute, notFound } from "@tanstack/react-router"
import {
  useMutation,
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { AmendmentsBanner } from "@/components/amendments-banner"
import { ApplicationStatusBadge } from "@/components/application-status-badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { api } from "@/lib/api"
import { lotAuctionQuery } from "@/lib/auctions"
import { problemMessage } from "@/lib/auth"
import { formatDateTime, formatTenge } from "@/lib/format"
import {
  myApplicationsQuery,
  reasonLabel,
  rejectionReasonsQuery,
} from "@/lib/participant"
import { cn } from "@/lib/utils"
import { buttonVariants } from "@/components/ui/button"

import type { ApplicationDto } from "@/lib/participant"

// Карточка своей заявки: файлы (FR-401) и отзыв до дедлайна (FR-404).
export const Route = createFileRoute(
  "/app/participant/applications/$applicationId"
)({
  loader: async ({ context, params }) => {
    const list = await context.queryClient.ensureQueryData(myApplicationsQuery)
    if (!list.some((a) => a.id === params.applicationId)) throw notFound()
    await context.queryClient.ensureQueryData(rejectionReasonsQuery)
  },
  component: ApplicationPage,
})

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
  const { data: reasons } = useSuspenseQuery(rejectionReasonsQuery)

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
    onSuccess: refresh,
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
          params: { path: { id: application.id } },
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
      await refresh()
    },
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

      {/* FR-304, FR-1004: условия изменились - можно отказаться (п. 26.5) */}
      <AmendmentsBanner
        tenderId={application.tender_id}
        applicationId={
          application.withdrawn_at == null ? application.id : undefined
        }
      />

      <header className="flex flex-col gap-2">
        <div className="flex flex-wrap items-center gap-3">
          <ApplicationStatusBadge status={application.status} />
          <span className="text-sm text-muted-foreground">
            {m.application_card_title({ id: application.id.slice(0, 8) })}
          </span>
        </div>
        <dl className="grid grid-cols-1 gap-3 rounded-lg border p-4 sm:grid-cols-3">
          <div className="flex flex-col gap-0.5">
            <dt className="text-sm text-muted-foreground">
              {m.application_price()}
            </dt>
            <dd className="font-medium" suppressHydrationWarning>
              {application.price_amount != null
                ? formatTenge(application.price_amount)
                : m.application_price_sealed()}
            </dd>
          </div>
          <div className="flex flex-col gap-0.5">
            <dt className="text-sm text-muted-foreground">
              {m.application_submitted_at()}
            </dt>
            <dd className="font-medium" suppressHydrationWarning>
              {formatDateTime(application.submitted_at)}
            </dd>
          </div>
          {application.withdrawn_at != null && (
            <div className="flex flex-col gap-0.5">
              <dt className="text-sm text-muted-foreground">
                {m.application_withdrawn_at()}
              </dt>
              <dd className="font-medium" suppressHydrationWarning>
                {formatDateTime(application.withdrawn_at)}
              </dd>
            </div>
          )}
          {application.rejection_reason != null && (
            <div className="flex flex-col gap-0.5">
              <dt className="text-sm text-muted-foreground">
                {m.rejection_reason_label()}
              </dt>
              <dd className="font-medium">
                {reasonLabel(reasons, application.rejection_reason)}
              </dd>
            </div>
          )}
        </dl>
        {isSubmitted && (
          <div>
            <Button
              variant="destructive"
              onClick={() => withdraw.mutate()}
              disabled={withdraw.isPending}
            >
              {m.application_withdraw()}
            </Button>
          </div>
        )}
        {withdraw.isError && (
          <p role="alert" className="text-sm text-destructive">
            {problemMessage(withdraw.error)}
          </p>
        )}
        {application.status === "admitted" && (
          <AuctionRoomLink lotId={application.lot_id} />
        )}
      </header>

      <section aria-labelledby="application-files">
        <h2
          id="application-files"
          className="mb-3 font-heading text-lg font-semibold"
        >
          {m.application_files_title()}
        </h2>
        {application.files.length === 0 ? (
          <p className="text-muted-foreground">{m.application_files_empty()}</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {application.files.map((file) => (
              <li key={file.id}>
                <a
                  href={`/api/v1/applications/${application.id}/files/${file.id}`}
                  className={cn(
                    buttonVariants({ variant: "link" }),
                    "h-auto px-0"
                  )}
                >
                  {file.filename}
                </a>
                <span className="ml-2 text-sm text-muted-foreground">
                  {Math.ceil(file.size_bytes / 1024)} KiB
                </span>
              </li>
            ))}
          </ul>
        )}

        {isSubmitted && (
          <form
            className="mt-4 flex flex-wrap items-end gap-3"
            onSubmit={(event) => {
              event.preventDefault()
              upload.mutate()
            }}
          >
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="application-file">{m.file_upload_label()}</Label>
              <Input
                id="application-file"
                type="file"
                required
                ref={fileInput}
              />
            </div>
            <Button type="submit" disabled={upload.isPending}>
              {m.file_upload_submit()}
            </Button>
            {upload.isError && (
              <p role="alert" className="w-full text-sm text-destructive">
                {problemMessage(upload.error)}
              </p>
            )}
          </form>
        )}
      </section>
    </div>
  )
}

/** Вход в комнату торгов допущенного участника (FR-601): появляется, когда
 * секретарь открыл комнату по лоту. */
function AuctionRoomLink({ lotId }: { lotId: string }) {
  const { data: auction } = useQuery(lotAuctionQuery(lotId))
  if (!auction) return null

  return (
    <Link
      to="/app/auctions/$auctionId"
      params={{ auctionId: auction.id }}
      data-testid="auction-room-link"
      className={cn(buttonVariants({ variant: "outline" }), "self-start")}
    >
      {m.auction_go_to_room()} →
    </Link>
  )
}
