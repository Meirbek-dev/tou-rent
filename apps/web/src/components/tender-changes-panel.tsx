import { useState } from "react"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"

import { m } from "#/paraglide/messages"
import { ConfirmAction } from "@/components/confirm-action"
import { FormAlert } from "@/components/form-alert"
import {
  LotDraftFields,
  emptyLot,
  lotDraftToRequest,
} from "@/components/lot-draft-fields"
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
import { formatDateTime } from "@/lib/format"
import { fromAlmatyInput, objectsQuery } from "@/lib/organizer"
import { notifySuccess } from "@/lib/toast"

import type { LotDraft } from "@/components/lot-draft-fields"

/**
 * Изменение документации и отмена тендера (FR-304, FR-305) в кабинете
 * организатора: новая редакция обязана описать изменения и продлить прием
 * заявок (п. 27), отмена - назвать нарушение (п. 78). Сроки и окно правки
 * проверяет сервер, форма лишь собирает данные.
 *
 * Той же редакцией организатор добавляет лоты и переназначает вскрытие и
 * торги: объявленный тендер иначе пришлось бы отменять и публиковать
 * заново, теряя поданные заявки (п. 78-79).
 */
export function TenderChangesPanel({
  tenderId,
  lotCount,
  openingAt,
  tradingAt,
  onChanged,
}: {
  tenderId: string
  /** Уже опубликованные лоты: новые нумеруются подряд за ними */
  lotCount: number
  openingAt: string | null
  tradingAt: string | null
  onChanged: () => Promise<void>
}) {
  const queryClient = useQueryClient()
  const { data: objectsPage } = useSuspenseQuery(objectsQuery)
  const [summary, setSummary] = useState("")
  const [deadline, setDeadline] = useState("")
  const [opening, setOpening] = useState("")
  const [trading, setTrading] = useState("")
  const [lots, setLots] = useState<LotDraft[]>([])
  const [reason, setReason] = useState("")

  const firstObjectId = objectsPage.items[0]?.id ?? ""

  const refresh = async () => {
    await queryClient.invalidateQueries({
      queryKey: tenderAmendmentsQuery(tenderId).queryKey,
    })
    await onChanged()
  }

  const amend = useMutation({
    mutationFn: () => {
      // Кнопка выключена при пустом сроке, так что ветка недостижима - но
      // `string | null` закрывается здесь, а не приведением типа
      const newDeadline = fromAlmatyInput(deadline)
      if (newDeadline === null) {
        throw new Error("deadline is required")
      }
      return amendTender(
        tenderId,
        summary,
        {
          newDeadline,
          newOpeningAt: fromAlmatyInput(opening),
          newTradingAt: fromAlmatyInput(trading),
        },
        lots.map(lotDraftToRequest)
      )
    },
    onSuccess: async () => {
      setSummary("")
      setDeadline("")
      setOpening("")
      setTrading("")
      setLots([])
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

  const patchLot = (index: number, patch: Partial<LotDraft>) => {
    setLots((current) =>
      current.map((lot, i) => (i === index ? { ...lot, ...patch } : lot))
    )
  }

  return (
    <Panel
      titleAs="h3"
      title={m.amend_title()}
      description={m.amend_hint()}
      contentClassName="flex flex-col gap-4"
    >
      <div className="flex flex-col gap-4" data-testid="tender-changes">
        <form
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault()
            amend.mutate()
          }}
        >
          <div className="flex flex-col gap-3 sm:flex-row sm:flex-wrap sm:items-end">
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
          </div>

          {/* Вскрытие и торги переносятся той же редакцией. Пустое поле - не
              «сбросить», а «оставить как есть»: без нового вскрытия БД
              сдвигает прежнее вслед за продленным сроком (п. 27) */}
          <div className="flex flex-col gap-3 sm:flex-row sm:flex-wrap sm:items-end">
            <div className="flex w-full flex-col gap-1.5 sm:w-auto">
              <Label htmlFor="amend-opening">{m.amend_opening_label()}</Label>
              <Input
                id="amend-opening"
                type="datetime-local"
                value={opening}
                onChange={(event) => setOpening(event.target.value)}
              />
              <p className="text-xs text-muted-foreground">
                {m.amend_schedule_current({
                  value: formatDateTime(openingAt) ?? m.amend_schedule_unset(),
                })}
              </p>
            </div>
            <div className="flex w-full flex-col gap-1.5 sm:w-auto">
              <Label htmlFor="amend-trading">{m.amend_trading_label()}</Label>
              <Input
                id="amend-trading"
                type="datetime-local"
                value={trading}
                onChange={(event) => setTrading(event.target.value)}
              />
              <p className="text-xs text-muted-foreground">
                {m.amend_schedule_current({
                  value: formatDateTime(tradingAt) ?? m.amend_schedule_unset(),
                })}
              </p>
            </div>
          </div>

          {/* Добавление лотов - редкая половина формы, поэтому свернута:
              обычная редакция правит только сроки */}
          <details className="rounded-lg border p-3">
            <summary className="cursor-pointer text-sm font-medium">
              {m.amend_lots_toggle({ n: lots.length })}
            </summary>
            <div className="flex flex-col gap-4 pt-3">
              <p className="text-sm text-muted-foreground">
                {m.amend_lots_hint()}
              </p>
              {lots.map((lot, index) => (
                <LotDraftFields
                  key={index}
                  lot={lot}
                  n={lotCount + index + 1}
                  idPrefix={`amend-lot-${index}`}
                  objects={objectsPage.items}
                  onChange={(patch) => patchLot(index, patch)}
                  onRemove={() =>
                    setLots((current) => current.filter((_, i) => i !== index))
                  }
                />
              ))}
              {objectsPage.items.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  {m.tender_new_no_objects()}
                </p>
              ) : (
                <div>
                  <Button
                    type="button"
                    variant="outline"
                    data-testid="amend-add-lot"
                    onClick={() =>
                      setLots((current) => [
                        ...current,
                        emptyLot(firstObjectId),
                      ])
                    }
                  >
                    {m.tender_lot_add()}
                  </Button>
                </div>
              )}
            </div>
          </details>

          <div>
            <Button
              type="submit"
              data-testid="amend-tender"
              disabled={amend.isPending || summary === "" || deadline === ""}
            >
              {m.amend_publish()}
            </Button>
          </div>
        </form>
        {amend.isError && <FormAlert>{problemMessage(amend.error)}</FormAlert>}

        {/* Отмена тендера (п. 78-79) назад не отыгрывается: заявки
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
