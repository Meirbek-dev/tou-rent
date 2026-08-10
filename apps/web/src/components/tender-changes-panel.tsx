import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  amendTender,
  cancelTender,
  tenderAmendmentsQuery,
} from "@/lib/amendments"
import { problemMessage } from "@/lib/auth"

/**
 * Изменение документации и отмена тендера (FR-304, FR-305) в кабинете
 * организатора: новая редакция обязана описать изменения и продлить прием
 * заявок (п. 27), отмена - назвать нарушение (п. 78). Сроки и окно правки
 * проверяет сервер, форма лишь собирает данные.
 */
export function TenderChangesPanel({
  tenderId,
  onChanged,
}: {
  tenderId: string
  onChanged: () => Promise<void>
}) {
  const queryClient = useQueryClient()
  const [summary, setSummary] = useState("")
  const [deadline, setDeadline] = useState("")
  const [reason, setReason] = useState("")

  const refresh = async () => {
    await queryClient.invalidateQueries({
      queryKey: tenderAmendmentsQuery(tenderId).queryKey,
    })
    await onChanged()
  }

  const amend = useMutation({
    mutationFn: () =>
      amendTender(tenderId, summary, new Date(deadline).toISOString()),
    onSuccess: async () => {
      setSummary("")
      setDeadline("")
      await refresh()
    },
  })

  const cancel = useMutation({
    mutationFn: () => cancelTender(tenderId, reason),
    onSuccess: async () => {
      setReason("")
      await refresh()
    },
  })

  return (
    <section
      aria-labelledby="tender-changes"
      className="flex flex-col gap-4 rounded-lg border p-4"
      data-testid="tender-changes"
    >
      <h3 id="tender-changes" className="font-heading text-lg font-semibold">
        {m.amend_title()}
      </h3>
      <p className="text-sm text-muted-foreground">{m.amend_hint()}</p>

      <form
        className="flex flex-wrap items-end gap-3"
        onSubmit={(event) => {
          event.preventDefault()
          amend.mutate()
        }}
      >
        <div className="flex min-w-64 flex-1 flex-col gap-1.5">
          <Label htmlFor="amend-summary">{m.amend_summary_label()}</Label>
          <Input
            id="amend-summary"
            value={summary}
            onChange={(event) => setSummary(event.target.value)}
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="amend-deadline">{m.amend_deadline_label()}</Label>
          <Input
            id="amend-deadline"
            type="datetime-local"
            value={deadline}
            onChange={(event) => setDeadline(event.target.value)}
          />
        </div>
        <Button
          type="submit"
          data-testid="amend-tender"
          disabled={amend.isPending || summary === "" || deadline === ""}
        >
          {m.amend_publish()}
        </Button>
      </form>
      {amend.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(amend.error)}
        </p>
      )}

      <form
        className="flex flex-wrap items-end gap-3 border-t pt-4"
        onSubmit={(event) => {
          event.preventDefault()
          cancel.mutate()
        }}
      >
        <div className="flex min-w-64 flex-1 flex-col gap-1.5">
          <Label htmlFor="cancel-reason">{m.cancel_reason_label()}</Label>
          <Input
            id="cancel-reason"
            value={reason}
            onChange={(event) => setReason(event.target.value)}
          />
        </div>
        <Button
          type="submit"
          variant="destructive"
          data-testid="cancel-tender"
          disabled={cancel.isPending || reason === ""}
        >
          {m.tender_cancel()}
        </Button>
      </form>
      {cancel.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(cancel.error)}
        </p>
      )}
    </section>
  )
}
