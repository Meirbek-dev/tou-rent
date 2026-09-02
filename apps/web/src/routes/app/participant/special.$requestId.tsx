import { useRef, useState } from "react"
import { Link, createFileRoute, notFound } from "@tanstack/react-router"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { SpecialProgress } from "@/components/special-progress"
import { Button, buttonVariants } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { api } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import { formatDateTime } from "@/lib/format"
import {
  localeLabel,
  mySpecialRequestsQuery,
  reviewTermLabel,
  specialCategoriesQuery,
  specialStatusLabel,
} from "@/lib/special"
import { cn } from "@/lib/utils"
import { UPLOAD_ACCEPT, uploadError } from "@/lib/validation"

import type { SpecialRequest } from "@/lib/special"

// Карточка заявки особого порядка (FR-1201): состояние, документы по позициям
// перечня категории (п. 88), печатная форма Прил. 3 и отзыв до решения.
export const Route = createFileRoute("/app/participant/special/$requestId")({
  loader: async ({ context, params }) => {
    const [list] = await Promise.all([
      context.queryClient.ensureQueryData(mySpecialRequestsQuery),
      context.queryClient.ensureQueryData(specialCategoriesQuery),
    ])
    if (!list.some((request) => request.id === params.requestId)) {
      throw notFound()
    }
  },
  head: () => ({
    meta: [{ title: `${m.special_requests_title()} - ToU Rent` }],
  }),
  component: SpecialRequestPage,
})

function SpecialRequestPage() {
  const { requestId } = Route.useParams()
  const { data: requests } = useSuspenseQuery(mySpecialRequestsQuery)
  const request = requests.find((item) => item.id === requestId)

  if (request === undefined) throw notFound()
  return <SpecialRequestCard request={request} />
}

function SpecialRequestCard({ request }: { request: SpecialRequest }) {
  const queryClient = useQueryClient()
  const fileInput = useRef<HTMLInputElement>(null)
  const [documentCode, setDocumentCode] = useState("")
  // Ограничения досье (INV-042, `upload.rs`): белый список форматов и потолок
  // 10 МБ. Форма проверяет их до отправки - отказ после выгрузки файла
  // целиком заявитель ждал впустую
  const [fileError, setFileError] = useState<string | undefined>(undefined)
  const { data: categories } = useSuspenseQuery(specialCategoriesQuery)
  const category = categories.find((item) => item.code === request.category)

  const refresh = () =>
    queryClient.invalidateQueries({ queryKey: mySpecialRequestsQuery.queryKey })

  const withdraw = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST(
        "/api/v1/special-requests/{id}/withdraw",
        { params: { path: { id: request.id } } }
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
        "/api/v1/special-requests/{id}/files",
        {
          params: {
            path: { id: request.id },
            query: documentCode === "" ? {} : { document_code: documentCode },
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
      await refresh()
    },
  })

  const isOpen =
    request.status === "submitted" || request.status === "under_review"

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

      <header className="flex flex-col gap-2">
        <div className="flex flex-wrap items-center gap-3">
          <span className="rounded-md border px-2 py-0.5 text-sm">
            {specialStatusLabel(request.status)}
          </span>
          <span className="text-sm text-muted-foreground">
            {m.special_card_title({ id: request.id.slice(0, 8) })}
          </span>
        </div>
        <dl className="grid grid-cols-1 gap-3 rounded-lg border p-4 sm:grid-cols-3">
          <div className="flex flex-col gap-0.5">
            <dt className="text-sm text-muted-foreground">
              {m.special_category_label()}
            </dt>
            <dd className="font-medium">
              {category ? localeLabel(category) : request.category_label}
            </dd>
          </div>
          <div className="flex flex-col gap-0.5">
            <dt className="text-sm text-muted-foreground">
              {m.application_submitted_at()}
            </dt>
            <dd className="font-medium" suppressHydrationWarning>
              {formatDateTime(request.submitted_at)}
            </dd>
          </div>
          {category && (
            <div className="flex flex-col gap-0.5">
              <dt className="text-sm text-muted-foreground">
                {m.special_review_label()}
              </dt>
              <dd className="font-medium">{reviewTermLabel(category)}</dd>
            </div>
          )}
          {request.object_name != null && (
            <div className="flex flex-col gap-0.5">
              <dt className="text-sm text-muted-foreground">
                {m.special_object_label()}
              </dt>
              <dd className="font-medium">{request.object_name}</dd>
            </div>
          )}
          {request.requested_months != null && (
            <div className="flex flex-col gap-0.5">
              <dt className="text-sm text-muted-foreground">
                {m.special_months_label()}
              </dt>
              <dd className="font-medium">{request.requested_months}</dd>
            </div>
          )}
          {request.withdrawn_at != null && (
            <div className="flex flex-col gap-0.5">
              <dt className="text-sm text-muted-foreground">
                {m.application_withdrawn_at()}
              </dt>
              <dd className="font-medium" suppressHydrationWarning>
                {formatDateTime(request.withdrawn_at)}
              </dd>
            </div>
          )}
        </dl>

        <p className="rounded-lg border p-4 text-sm">{request.purpose}</p>

        {/* FR-1203 (п. 86): вопрос переведен в общий порядок - заявка ушла
            в тендер вместе с конкурирующими */}
        {request.tender_id != null && (
          <p className="rounded-lg border border-dashed p-3 text-sm">
            {m.special_redirected_to_tender()}
          </p>
        )}

        <div className="flex flex-wrap gap-3">
          <a
            href={`/api/v1/special-requests/${request.id}/application.pdf`}
            className={cn(buttonVariants({ variant: "outline" }))}
          >
            {m.special_pdf()}
          </a>
          {isOpen && (
            <Button
              variant="destructive"
              onClick={() => withdraw.mutate()}
              disabled={withdraw.isPending}
            >
              {m.special_withdraw()}
            </Button>
          )}
        </div>
        {withdraw.isError && (
          <p role="alert" className="text-sm text-destructive">
            {problemMessage(withdraw.error)}
          </p>
        )}
      </header>

      <SpecialProgress requestId={request.id} />

      <section aria-labelledby="special-files">
        <h2
          id="special-files"
          className="mb-3 font-heading text-lg font-semibold"
        >
          {m.special_documents_title()}
        </h2>
        {request.files.length === 0 ? (
          <p className="text-muted-foreground">{m.application_files_empty()}</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {request.files.map((file) => (
              <li key={file.id}>
                <a
                  href={`/api/v1/special-requests/${request.id}/files/${file.id}`}
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

        {isOpen && (
          <form
            className="mt-4 flex flex-wrap items-end gap-3"
            onSubmit={(event) => {
              event.preventDefault()
              const problem = uploadError(fileInput.current?.files?.[0])
              setFileError(problem)
              if (problem !== undefined) return
              upload.mutate()
            }}
          >
            <div className="flex min-w-56 flex-col gap-1.5">
              <Label htmlFor="special-document-code">
                {m.special_document_position()}
              </Label>
              <NativeSelect
                id="special-document-code"
                value={documentCode}
                onChange={(event) => setDocumentCode(event.target.value)}
              >
                <NativeSelectOption value="">
                  {m.special_document_other()}
                </NativeSelectOption>
                {(category?.documents ?? []).map((document) => (
                  <NativeSelectOption key={document.code} value={document.code}>
                    {localeLabel(document)}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="special-file">{m.file_upload_label()}</Label>
              <Input
                id="special-file"
                type="file"
                required
                accept={UPLOAD_ACCEPT}
                aria-invalid={fileError !== undefined}
                ref={fileInput}
                onChange={() => {
                  const file = fileInput.current?.files?.[0]
                  setFileError(
                    file === undefined ? undefined : uploadError(file)
                  )
                }}
              />
            </div>
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
      </section>
    </div>
  )
}
