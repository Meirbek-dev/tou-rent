import { useState } from "react"
import { Link } from "@tanstack/react-router"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { Button } from "@/components/ui/button"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { useNotificationStream } from "@/hooks/use-notification-stream"
import { problemMessage } from "@/lib/auth"
import { formatDateTime, formatTenge } from "@/lib/format"
import {
  markRead,
  notificationsQuery,
  unreadCountQuery,
} from "@/lib/notifications"
import { deadlineLabel } from "@/lib/obligation-labels"
import { decisionLabel } from "@/lib/special"
import { notifyError, notifySuccess } from "@/lib/toast"
import { BellIcon } from "lucide-react"

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
 * Виды событий центра уведомлений - тот же перечень, что в
 * `domain::notification::NotificationKind`.
 *
 * Список нужен фильтру на странице истории: `kind` приходит с провода
 * строкой, и союза, по которому компилятор проверил бы полноту подписей,
 * из контракта не выводится. Полноту самого перечня стережет
 * `lib/notification-kinds.test.ts` - он сверяет ветки `case` этого файла
 * с каталогом Rust.
 */
export const NOTIFICATION_KINDS = [
  "auction_invitation",
  "application_rejected",
  "obligation_overdue",
  "runner_up_offer",
  "tender_amended",
  "tender_cancelled",
  "protocol_published",
  "special_decided",
] as const

export type NotificationKind = (typeof NOTIFICATION_KINDS)[number]

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
    // Все восемь видов каталога разобраны выше (это стережет
    // `notification-kinds.test.ts`), но сервер выкатывается раньше страницы:
    // девятый вид приедет в браузер, который его не знает, и машинный код
    // вроде `protocol_published` в колокольчике - не текст уведомления
    default:
      return m.notif_unknown()
  }
}

/** Подпись вида события для фильтра истории (FR-1301). */
export function notificationKindLabel(kind: NotificationKind): string {
  return KIND_LABELS[kind]()
}

const KIND_LABELS: Record<NotificationKind, () => string> = {
  auction_invitation: m.notif_kind_auction_invitation,
  application_rejected: m.notif_kind_application_rejected,
  obligation_overdue: m.notif_kind_obligation_overdue,
  runner_up_offer: m.notif_kind_runner_up_offer,
  tender_amended: m.notif_kind_tender_amended,
  tender_cancelled: m.notif_kind_tender_cancelled,
  protocol_published: m.notif_kind_protocol_published,
  special_decided: m.notif_kind_special_decided,
}

/**
 * Адрес предмета уведомления; `undefined` - предмет открыть нечем.
 *
 * До сих пор уведомление было мертвым текстом: участник читал «вы допущены
 * к торгам» и шел искать свою заявку руками. Одна реализация на список и
 * на колокольчик намеренно - иначе два перечня видов разъехались бы.
 *
 * Идентификаторы читаются из payload'а во время выполнения, а не берутся
 * из объявленных в `lib/notifications.ts` типов: на проводе их больше, чем
 * объявлено (`tender_amended`, `tender_cancelled`, `protocol_published` и
 * `runner_up_offer` несут `tender_id`, которого в типе нет), и приведение
 * типом здесь молча соврало бы в обе стороны. Чего в payload'е нет, того
 * не выдумываем: ссылки просто не будет.
 *
 * Цель выбирается независимой от роли: страница истории общая для всех
 * кабинетов, и вести оттуда в кабинет организатора того, кто открыл ее
 * участником, значит вести в 403. Тендер поэтому открывается публичной
 * карточкой, а заявка и заявка особого порядка - своими, они и так
 * принадлежат получателю.
 */
export function notificationTarget(
  notification: NotificationDto
): string | undefined {
  const payload = notification.payload

  switch (notification.kind) {
    // Приглашение адресовано заявителю; идентификатора торгов payload
    // не несет (FR-504), поэтому ведем в его заявку
    case "auction_invitation":
      return path("/app/participant/applications", payload, "application_id")
    case "application_rejected":
      return path("/app/participant/applications", payload, "application_id")
    // Срок Правил может быть и внетендерным - тогда открывать нечего
    case "obligation_overdue":
      return path("/tenders", payload, "tender_id")
    case "runner_up_offer":
      return path("/tenders", payload, "tender_id")
    case "tender_amended":
      return path("/tenders", payload, "tender_id")
    case "tender_cancelled":
      return path("/tenders", payload, "tender_id")
    case "protocol_published":
      return path("/tenders", payload, "tender_id")
    case "special_decided":
      return path("/app/participant/special", payload, "special_request_id")
    default:
      return undefined
  }
}

/** Адрес записи по идентификатору из payload'а; без него ссылки нет. */
function path(
  base: string,
  payload: unknown,
  field: string
): string | undefined {
  if (typeof payload !== "object" || payload === null) return undefined
  const value = (payload as Record<string, unknown>)[field]
  if (typeof value !== "string" || value === "") return undefined
  return `${base}/${value}`
}

/** Колокольчик центра уведомлений (FR-1301): счетчик непрочитанных,
 * последние события, SSE-доставка ≤1 с. */
export function NotificationBell() {
  useNotificationStream()
  const queryClient = useQueryClient()
  const [open, setOpen] = useState(false)
  const { data: unread } = useQuery(unreadCountQuery)
  const { data: page } = useQuery(notificationsQuery)

  const readAll = useMutation({
    mutationFn: () => markRead(),
    onSuccess: async () => {
      notifySuccess(m.notif_marked_read())
      await queryClient.invalidateQueries({ queryKey: ["notifications"] })
    },
    onError: (error: unknown) => notifyError(problemMessage(error)),
  })

  const recent = page?.items.slice(0, 5) ?? []
  const unreadCount = unread ?? 0

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger
        render={
          <Button
            variant="ghost"
            size="icon"
            // Значок с числом обязан называть это число вслух: «Уведомления»
            // без счетчика скрывало от чтения с экрана единственное, ради
            // чего на колокольчик и смотрят
            aria-label={
              unreadCount > 0
                ? m.notif_bell_label_with_count({ count: unreadCount })
                : m.notif_bell_label()
            }
            className="relative"
          />
        }
      >
        <BellIcon aria-hidden="true" className="size-5" />
        {unreadCount > 0 && (
          <span
            data-testid="unread-badge"
            className="absolute -top-0.5 -right-0.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-destructive px-1 text-[11px] font-semibold text-white tabular-nums ring-2 ring-background"
          >
            {unreadCount}
          </span>
        )}
        {/* Приход нового уведомления по SSE меняет только число в значке -
            без этой области он менялся молча */}
        <span aria-live="polite" className="sr-only">
          {m.notif_unread_live({ count: unreadCount })}
        </span>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        className="w-[min(24rem,calc(100vw-2rem))] p-0"
      >
        <div className="flex items-center justify-between gap-2 border-b px-4 py-2">
          <span className="font-medium">{m.notif_title()}</span>
          {unreadCount > 0 && (
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
          <ul className="flex max-h-80 flex-col overflow-y-auto overscroll-contain">
            {recent.map((notification) => {
              const unreadRow = notification.read_at == null
              const target = notificationTarget(notification)
              const body = (
                <>
                  <span className="flex items-start gap-2">
                    {/* Непрочитанное отличалось только насыщенностью шрифта -
                      различие, которого нет ни при слабом зрении, ни в
                      высококонтрастном режиме. Точка видна и там, и там */}
                    <span
                      aria-hidden="true"
                      className={
                        unreadRow
                          ? "mt-1.5 size-1.5 shrink-0 rounded-full bg-primary"
                          : "mt-1.5 size-1.5 shrink-0"
                      }
                    />
                    <span
                      className={
                        unreadRow ? "font-medium" : "text-muted-foreground"
                      }
                    >
                      {notificationText(notification)}
                    </span>
                  </span>
                  <span
                    className="text-xs text-muted-foreground"
                    suppressHydrationWarning
                  >
                    {formatDateTime(notification.created_at)}
                  </span>
                </>
              )

              return (
                <li
                  key={notification.id}
                  className="border-b text-sm last:border-b-0"
                >
                  {target === undefined ? (
                    <span className="flex flex-col gap-1 px-4 py-3">
                      {body}
                    </span>
                  ) : (
                    // Всплывающее окно закрывается само: после перехода оно
                    // осталось бы висеть поверх той самой страницы, куда вело
                    <Link
                      to={target}
                      onClick={() => setOpen(false)}
                      className="flex flex-col gap-1 px-4 py-3 hover:bg-muted/50"
                    >
                      {body}
                    </Link>
                  )}
                </li>
              )
            })}
          </ul>
        )}
        <div className="border-t px-4 py-2">
          <Link
            to="/app/notifications"
            onClick={() => setOpen(false)}
            className="text-sm underline-offset-4 hover:underline"
          >
            {m.notif_view_all()}
          </Link>
        </div>
      </PopoverContent>
    </Popover>
  )
}
