import { m } from "#/paraglide/messages"
import { formatDateTime } from "@/lib/format"
import { BanIcon } from "lucide-react"

import type { TenderDto } from "@/lib/api"

/**
 * Отмена тендера словами (п. 82); `null` - тендер не отменен.
 *
 * Значка «Отменен» мало: карточка отмененного тендера выглядела живым
 * объявлением - те же сроки, те же лоты, та же кнопка «Подать заявку» рядом
 * в реестре, - и единственное слово в углу не отвечало ни «когда», ни
 * «почему». Причину пишет тот, кто отменяет, и она едет в `cancel_reason`.
 *
 * Поле необязательно: у тендеров, отмененных до появления причины, его нет,
 * и вместо выдумки здесь стоит прямая оговорка.
 */
export function TenderCancelledBanner({
  tender,
}: {
  tender: Pick<TenderDto, "status" | "cancelled_at" | "cancel_reason">
}) {
  if (tender.status !== "cancelled") return null

  const at = formatDateTime(tender.cancelled_at)

  return (
    <div
      role="status"
      data-testid="tender-cancelled-banner"
      className="flex flex-col gap-1.5 rounded-xl border border-destructive/25 bg-destructive/10 p-4"
    >
      <p className="flex items-center gap-2 font-heading font-semibold text-destructive">
        <BanIcon aria-hidden="true" className="size-4 shrink-0" />
        {m.tender_cancelled_banner_title()}
        {at !== null && (
          <span
            className="font-normal text-destructive/90 tabular-nums"
            suppressHydrationWarning
          >
            · {m.tender_cancelled_at({ date: at })}
          </span>
        )}
      </p>
      <p className="text-sm text-destructive/90">
        {tender.cancel_reason == null || tender.cancel_reason === "" ? (
          m.tender_cancel_reason_missing()
        ) : (
          <>
            <span className="font-medium">{m.cancel_reason_label()}:</span>{" "}
            {tender.cancel_reason}
          </>
        )}
      </p>
    </div>
  )
}
