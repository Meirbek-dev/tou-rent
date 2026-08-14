import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"

import { m } from "#/paraglide/messages"
import { ConfirmAction } from "@/components/confirm-action"
import { FormAlert } from "@/components/form-alert"
import { Panel } from "@/components/panel"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  amendTender,
  cancelTender,
  tenderAmendmentsQuery,
} from "@/lib/amendments"
import { problemMessage } from "@/lib/auth"
import { notifySuccess } from "@/lib/toast"

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
      notifySuccess(m.tender_change_published_toast())
      await refresh()
    },
  })

  const cancel = useMutation({
    mutationFn: () => cancelTender(tenderId, reason),
    onSuccess: async () => {
      setReason("")
      notifySuccess(m.tender_change_cancelled_toast())
      await refresh()
    },
  })

  return (
    <Panel
      titleAs="h3"
      title={m.amend_title()}
      description={m.amend_hint()}
      contentClassName="flex flex-col gap-4"
    >
      <div className="flex flex-col gap-4" data-testid="tender-changes">
        <form
          className="flex flex-col gap-3 sm:flex-row sm:flex-wrap sm:items-end"
          onSubmit={(event) => {
            event.preventDefault()
            amend.mutate()
          }}
        >
          <div className="flex w-full flex-col gap-1.5 sm:min-w-64 sm:flex-1">
            <Label htmlFor="amend-summary">{m.amend_summary_label()}</Label>
            <Input
              id="amend-summary"
              value={summary}
              onChange={(event) => setSummary(event.target.value)}
            />
          </div>
          <div className="flex w-full flex-col gap-1.5 sm:w-auto">
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
        {amend.isError && <FormAlert>{problemMessage(amend.error)}</FormAlert>}

        {/* Отмена тендера (п. 78–79) назад не отыгрывается: заявки
            аннулируются, взносы возвращаются, тендер закрыт. Поэтому здесь
            не форма с кнопкой отправки, а подтверждаемое действие */}
        <div className="flex flex-col gap-3 border-t pt-4 sm:flex-row sm:flex-wrap sm:items-end">
          <div className="flex w-full flex-col gap-1.5 sm:min-w-64 sm:flex-1">
            <Label htmlFor="cancel-reason">{m.cancel_reason_label()}</Label>
            <Input
              id="cancel-reason"
              value={reason}
              onChange={(event) => setReason(event.target.value)}
            />
          </div>
          <ConfirmAction
            title={m.tender_change_cancel_confirm_title()}
            description={m.tender_change_cancel_confirm_description()}
            confirmLabel={m.tender_cancel()}
            variant="destructive-solid"
            disabled={cancel.isPending || reason === ""}
            onConfirm={() => cancel.mutate()}
            trigger={
              <Button
                type="button"
                variant="destructive"
                data-testid="cancel-tender"
              >
                {m.tender_cancel()}
              </Button>
            }
          />
        </div>
        {cancel.isError && (
          <FormAlert>{problemMessage(cancel.error)}</FormAlert>
        )}
      </div>
    </Panel>
  )
}
