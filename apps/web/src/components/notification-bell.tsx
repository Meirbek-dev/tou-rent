import { Link } from "@tanstack/react-router"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { HugeiconsIcon } from "@hugeicons/react"
import { Notification03Icon } from "@hugeicons/core-free-icons"
import { m } from "#/paraglide/messages"
import { deadlineLabel } from "@/lib/obligation-labels"
import { Button } from "@/components/ui/button"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { useNotificationStream } from "@/hooks/use-notification-stream"
import { formatDateTime, formatTenge } from "@/lib/format"
import {
  markRead,
  notificationsQuery,
  unreadCountQuery,
} from "@/lib/notifications"
import { decisionLabel } from "@/lib/special"

import type {
  ApplicationRejectedPayload,
  AuctionInvitationPayload,
  NotificationDto,
  ObligationOverduePayload,
  ProtocolPublishedPayload,
  RunnerUpOfferPayload,
  SpecialDecidedPayload,
  TenderAmendedPayload,
  TenderCancelledPayload,
} from "@/lib/notifications"

/**
 * Текст уведомления по типу события (enum NotificationKind контракта).
 *
 * Ветка нужна каждому виду: из семи здесь было два, и остальные пять -
 * просроченный срок, отмена тендера, новая редакция, предложение
 * участнику № 2, опубликованный протокол - показывались в колокольчике
 * машинным кодом вроде `protocol_published`.
 */
export function notificationText(notification: NotificationDto): string {
  switch (notification.kind) {
    case "auction_invitation": {
      const payload = notification.payload as AuctionInvitationPayload
      return m.notif_auction_invitation({
        tender: payload.tender_title,
        lot: payload.lot,
        bid: formatTenge(payload.starting_bid),
        date: formatDateTime(payload.trading_at) ?? "-",
      })
    }
    // FR-502 (п. 52, 56): решение комиссии по заявке - отказ с основанием
    case "application_rejected": {
      const payload = notification.payload as ApplicationRejectedPayload
      return m.notif_application_rejected({
        reason: payload.reason ?? payload.reason_code ?? "-",
      })
    }
    // FR-1702: срок Правил истек (п. 54, 57, 73, 75)
    case "obligation_overdue": {
      const payload = notification.payload as ObligationOverduePayload
      return m.notif_obligation_overdue({
        action: deadlineLabel(payload.action),
        rule: payload.rule_ref,
      })
    }
    // FR-903 (п. 117–118): договор предлагается участнику № 2
    case "runner_up_offer": {
      const payload = notification.payload as RunnerUpOfferPayload
      return m.notif_runner_up_offer({
        tender: payload.tender_title,
        lot: payload.lot,
        amount: formatTenge(payload.amount),
      })
    }
    // FR-304 (п. 27): опубликована новая редакция документации
    case "tender_amended": {
      const payload = notification.payload as TenderAmendedPayload
      return m.notif_tender_amended({
        tender: payload.tender_title,
        version: payload.version,
        deadline: formatDateTime(payload.new_deadline) ?? "-",
      })
    }
    // FR-305 (п. 78–79): тендер или лот отменен
    case "tender_cancelled": {
      const payload = notification.payload as TenderCancelledPayload
      return m.notif_tender_cancelled({
        tender: payload.tender_title,
        reason: payload.reason,
      })
    }
    // FR-702, FR-703 (п. 56, 75): протокол опубликован
    case "protocol_published": {
      const payload = notification.payload as ProtocolPublishedPayload
      return m.notif_protocol_published({
        tender: payload.tender_title,
        number: payload.protocol_number,
      })
    }
    // FR-1202 (п. 90): решение Правления по заявке особого порядка
    case "special_decided": {
      const payload = notification.payload as SpecialDecidedPayload
      return m.notif_special_decided({
        category: payload.category,
        decision: decisionLabel(payload.decision),
      })
    }
    default:
      return notification.kind
  }
}

/** Колокольчик центра уведомлений (FR-1301): счетчик непрочитанных,
 * последние события, SSE-доставка ≤1 с. */
export function NotificationBell() {
  useNotificationStream()
  const queryClient = useQueryClient()
  const { data: unread } = useQuery(unreadCountQuery)
  const { data: page } = useQuery(notificationsQuery)

  const readAll = useMutation({
    mutationFn: () => markRead(),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["notifications"] })
    },
  })

  const recent = page?.items.slice(0, 5) ?? []

  return (
    <Popover>
      <PopoverTrigger
        render={
          <Button
            variant="ghost"
            size="icon"
            aria-label={m.notif_bell_label()}
            className="relative"
          />
        }
      >
        <HugeiconsIcon icon={Notification03Icon} className="size-5" />
        {(unread ?? 0) > 0 && (
          <span
            data-testid="unread-badge"
            className="absolute -top-0.5 -right-0.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-destructive px-1 text-[10px] font-semibold text-white"
          >
            {unread}
          </span>
        )}
      </PopoverTrigger>
      <PopoverContent align="end" className="w-96 p-0">
        <div className="flex items-center justify-between gap-2 border-b px-4 py-2">
          <span className="font-medium">{m.notif_title()}</span>
          {(unread ?? 0) > 0 && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => readAll.mutate()}
              disabled={readAll.isPending}
            >
              {m.notif_mark_all_read()}
            </Button>
          )}
        </div>
        {recent.length === 0 ? (
          <p className="px-4 py-6 text-sm text-muted-foreground">
            {m.notif_empty()}
          </p>
        ) : (
          <ul className="flex max-h-80 flex-col overflow-y-auto">
            {recent.map((notification) => (
              <li
                key={notification.id}
                className="flex flex-col gap-1 border-b px-4 py-3 text-sm last:border-b-0"
              >
                <span
                  className={
                    notification.read_at == null
                      ? "font-medium"
                      : "text-muted-foreground"
                  }
                >
                  {notificationText(notification)}
                </span>
                <span
                  className="text-xs text-muted-foreground"
                  suppressHydrationWarning
                >
                  {formatDateTime(notification.created_at)}
                </span>
              </li>
            ))}
          </ul>
        )}
        <div className="border-t px-4 py-2">
          <Link
            to="/app/notifications"
            className="text-sm underline-offset-4 hover:underline"
          >
            {m.notif_view_all()}
          </Link>
        </div>
      </PopoverContent>
    </Popover>
  )
}
