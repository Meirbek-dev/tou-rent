import { useId, useRef, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { Panel } from "@/components/panel"
import { QueryBoundary } from "@/components/query-boundary"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { Spinner } from "@/components/ui/spinner"
import { problemMessage } from "@/lib/auth"
import { formatDateTime } from "@/lib/format"
import { notifySuccess } from "@/lib/toast"
import {
  commissionDocumentsQuery,
  shareCommissionDocument,
  uploadCommissionDocument,
} from "@/lib/commission-documents"
import { documentsForApplication } from "@/lib/commission-document-placement"

type Props = {
  tenderId: string
  applicationId?: string | undefined
  manager?: boolean
}

export function ApplicationDocumentsDialog({
  tenderId,
  applicationId,
  label,
}: Props & { label: string }) {
  const [open, setOpen] = useState(false)
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button variant="outline" size="sm" />}>
        {m.cd_application_action()}
      </DialogTrigger>
      <DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{m.cd_application_action()}</DialogTitle>
          <DialogDescription>{label}</DialogDescription>
        </DialogHeader>
        {open && (
          <CommissionDocuments
            tenderId={tenderId}
            applicationId={applicationId}
            manager
          />
        )}
      </DialogContent>
    </Dialog>
  )
}

export function CommissionDocuments({
  tenderId,
  applicationId,
  manager = false,
}: Props) {
  const client = useQueryClient()
  const query = useQuery(commissionDocumentsQuery(tenderId))
  const refresh = () =>
    client.invalidateQueries({
      queryKey: commissionDocumentsQuery(tenderId).queryKey,
    })
  const [confirmId, setConfirmId] = useState<string | null>(null)
  const share = useMutation({
    mutationFn: ({ id, shared }: { id: string; shared: boolean }) =>
      shareCommissionDocument(id, shared),
    onSuccess: async () => {
      setConfirmId(null)
      await refresh()
      notifySuccess(m.cd_saved())
    },
  })
  return (
    <Panel title={m.cd_title()} contentClassName="flex flex-col gap-4">
      {manager && (
        <p className="text-sm text-muted-foreground">
          {applicationId ? m.cd_private_help() : m.cd_common_help()}
        </p>
      )}
      <QueryBoundary
        query={query}
        skeleton={<Skeleton className="h-16 w-full" />}
      >
        {(page) => {
          const items = documentsForApplication(
            page.items,
            applicationId,
            !manager
          )
          return (
            <>
              {page.truncated && (
                <p role="status">
                  {m.list_truncated({ count: page.items.length })}
                </p>
              )}
              {items.length === 0 ? (
                <p className="text-sm text-muted-foreground">{m.cd_empty()}</p>
              ) : (
                <ul className="flex flex-col gap-4">
                  {items.map((doc) => (
                    <li key={doc.id} className="flex min-w-0 flex-col gap-2">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="font-medium wrap-anywhere">
                          {doc.title}
                        </span>
                        <Badge variant="secondary">{m.cd_uploaded()}</Badge>
                      </div>
                      <p className="text-sm wrap-anywhere">
                        {m.cd_details({
                          number: doc.number,
                          date: doc.document_date,
                        })}
                      </p>
                      <a
                        className="text-sm wrap-anywhere underline"
                        href={`/api/v1/commission-documents/${doc.id}/pdf`}
                      >
                        {doc.filename}
                      </a>
                      {manager && (
                        <>
                          <p className="text-sm text-muted-foreground">
                            {doc.shared_at ? m.cd_visible() : m.cd_hidden()} ·{" "}
                            {formatDateTime(doc.uploaded_at)}
                          </p>
                          {confirmId === doc.id ? (
                            <div className="flex flex-col gap-2">
                              <p role="status" className="text-sm">
                                {doc.application_id
                                  ? m.cd_confirm_private()
                                  : m.cd_confirm_common()}
                              </p>
                              <div className="flex flex-wrap gap-2">
                                <Button
                                  disabled={share.isPending}
                                  onClick={() =>
                                    share.mutate({ id: doc.id, shared: true })
                                  }
                                >
                                  {m.cd_confirm()}
                                </Button>
                                <Button
                                  variant="outline"
                                  disabled={share.isPending}
                                  onClick={() => setConfirmId(null)}
                                >
                                  {m.confirm_cancel()}
                                </Button>
                              </div>
                            </div>
                          ) : (
                            <Button
                              variant="outline"
                              size="sm"
                              className="self-start"
                              disabled={share.isPending}
                              onClick={() =>
                                doc.shared_at
                                  ? share.mutate({ id: doc.id, shared: false })
                                  : setConfirmId(doc.id)
                              }
                            >
                              {doc.shared_at ? m.cd_hide() : m.cd_share()}
                            </Button>
                          )}
                        </>
                      )}
                    </li>
                  ))}
                </ul>
              )}
            </>
          )
        }}
      </QueryBoundary>
      {share.isError && (
        <p role="alert" className="text-destructive">
          {problemMessage(share.error)}
        </p>
      )}
      {manager && (
        <UploadDocument
          tenderId={tenderId}
          applicationId={applicationId}
          onSaved={refresh}
        />
      )}
    </Panel>
  )
}

function UploadDocument({
  tenderId,
  applicationId,
  onSaved,
}: Props & { onSaved: () => Promise<unknown> }) {
  const id = useId()
  const fileInput = useRef<HTMLInputElement>(null)
  const [expanded, setExpanded] = useState(false)
  const [title, setTitle] = useState<string>(
    applicationId ? m.cd_default_decision() : m.cd_default_protocol()
  )
  const [number, setNumber] = useState("")
  const [date, setDate] = useState("")
  const [error, setError] = useState<string | null>(null)
  const upload = useMutation({
    mutationFn: (file: File) =>
      uploadCommissionDocument(
        tenderId,
        applicationId,
        title.trim(),
        number.trim(),
        date,
        file
      ),
    onSuccess: async () => {
      if (fileInput.current) fileInput.current.value = ""
      await onSaved()
      setExpanded(false)
      notifySuccess(m.cd_saved())
    },
  })
  if (!expanded)
    return (
      <Button
        className="self-start"
        variant="outline"
        onClick={() => setExpanded(true)}
      >
        {m.cd_upload()}
      </Button>
    )
  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault()
        const file = fileInput.current?.files?.[0]
        if (
          !file ||
          !file.name.toLowerCase().endsWith(".pdf") ||
          file.size === 0 ||
          file.size > 10 * 1024 * 1024
        ) {
          setError(m.cd_file_error())
          return
        }
        setError(null)
        upload.mutate(file)
      }}
    >
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor={`${id}-title`}>{m.cd_name()}</FieldLabel>
          <Input
            id={`${id}-title`}
            required
            maxLength={200}
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            disabled={upload.isPending}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${id}-number`}>{m.cd_number()}</FieldLabel>
          <Input
            id={`${id}-number`}
            required
            maxLength={100}
            value={number}
            onChange={(e) => setNumber(e.target.value)}
            disabled={upload.isPending}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${id}-date`}>{m.cd_date()}</FieldLabel>
          <Input
            id={`${id}-date`}
            type="date"
            required
            value={date}
            onChange={(e) => setDate(e.target.value)}
            disabled={upload.isPending}
          />
        </Field>
        <Field data-invalid={error !== null}>
          <FieldLabel htmlFor={`${id}-file`}>{m.cd_file()}</FieldLabel>
          <Input
            id={`${id}-file`}
            type="file"
            accept="application/pdf,.pdf"
            required
            ref={fileInput}
            disabled={upload.isPending}
            aria-invalid={error !== null}
          />
          {error && (
            <p role="alert" className="text-sm text-destructive">
              {error}
            </p>
          )}
        </Field>
      </FieldGroup>
      <p className="text-sm text-muted-foreground">{m.cd_upload_help()}</p>
      {upload.isError && (
        <p role="alert" className="text-destructive">
          {problemMessage(upload.error)}
        </p>
      )}
      <div className="flex flex-wrap gap-2">
        <Button type="submit" disabled={upload.isPending}>
          {upload.isPending && <Spinner data-icon="inline-start" />}
          {m.cd_upload()}
        </Button>
        <Button
          type="button"
          variant="outline"
          disabled={upload.isPending}
          onClick={() => setExpanded(false)}
        >
          {m.confirm_cancel()}
        </Button>
      </div>
    </form>
  )
}
