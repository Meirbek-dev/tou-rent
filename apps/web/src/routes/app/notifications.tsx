import { useState } from "react"
import { Link, createFileRoute } from "@tanstack/react-router"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { EmptyState } from "@/components/empty-state"
import {
  NOTIFICATION_KINDS,
  notificationKindLabel,
  notificationTarget,
  notificationText,
} from "@/components/notification-bell"
import { PageHeader } from "@/components/page-header"
import { PageShell } from "@/components/page-shell"
import { QueryBoundary } from "@/components/query-boundary"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { problemMessage } from "@/lib/auth"
import { formatDateTime } from "@/lib/format"
import { markRead, notificationsQuery } from "@/lib/notifications"
import { notifyError, notifySuccess } from "@/lib/toast"
import { BellIcon, FilterXIcon } from "lucide-react"

import type { NotificationKind } from "@/components/notification-bell"

// Страница истории уведомлений (FR-1301): полный список получателя,
// непрочитанные выделены, отметка о прочтении.
export const Route = createFileRoute("/app/notifications")({
  loader: async ({ context }) => {
    await context.queryClient.ensureQueryData(notificationsQuery)
  },
  head: () => ({ meta: [{ title: `${m.notif_title()} - ToU Rent` }] }),
  component: NotificationsPage,
})

/** «Все виды» - не вид события, поэтому отдельным значением, а не пустой строкой. */
const ALL_KINDS = "all"

function NotificationsPage() {
  const queryClient = useQueryClient()
  const query = useSuspenseQuery(notificationsQuery)
  const [kind, setKind] = useState<NotificationKind | typeof ALL_KINDS>(
    ALL_KINDS
  )

  const invalidate = () =>
    queryClient.invalidateQueries({ queryKey: ["notifications"] })

  const readAll = useMutation({
    mutationFn: () => markRead(),
    onSuccess: async () => {
      notifySuccess(m.notif_marked_read())
      await invalidate()
    },
    onError: (error: unknown) => notifyError(problemMessage(error)),
  })

  // Отметка по одному: сервер принимает подмножество идентификаторов
  // (`POST /notifications/read` с массивом ids), и «прочитано» перестает
  // быть решением «все или ничего»
  const readOne = useMutation({
    mutationFn: (id: string) => markRead([id]),
    onSuccess: async () => {
      notifySuccess(m.notif_marked_read())
      await invalidate()
    },
    onError: (error: unknown) => notifyError(problemMessage(error)),
  })

  const hasUnread = query.data.items.some((n) => n.read_at == null)

  return (
    <PageShell width="narrow">
      <PageHeader
        title={m.notif_title()}
        actions={
          hasUnread ? (
            <Button
              variant="outline"
              size="sm"
              onClick={() => readAll.mutate()}
              disabled={readAll.isPending}
            >
              {m.notif_mark_all_read()}
            </Button>
          ) : undefined
        }
      />

      <QueryBoundary
        query={query}
        empty={{
          when: (page) => page.items.length === 0,
          icon: BellIcon,
          title: m.notif_empty_title(),
          description: m.notif_empty(),
        }}
      >
        {(page) => {
          const items =
            kind === ALL_KINDS
              ? page.items
              : page.items.filter((item) => item.kind === kind)

          return (
            <div className="flex flex-col gap-4">
              <div className="flex w-full flex-col gap-1.5 sm:w-64">
                <Label htmlFor="notif-kind">{m.notif_filter_label()}</Label>
                <NativeSelect
                  id="notif-kind"
                  className="w-full"
                  value={kind}
                  onChange={(event) =>
                    setKind(
                      event.target.value === ALL_KINDS
                        ? ALL_KINDS
                        : (event.target.value as NotificationKind)
                    )
                  }
                >
                  <NativeSelectOption value={ALL_KINDS}>
                    {m.notif_filter_all()}
                  </NativeSelectOption>
                  {NOTIFICATION_KINDS.map((value) => (
                    <NativeSelectOption key={value} value={value}>
                      {notificationKindLabel(value)}
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
              </div>

              {items.length === 0 ? (
                <EmptyState
                  icon={FilterXIcon}
                  title={m.notif_filter_empty_title()}
                  description={m.notif_filter_empty()}
                  action={
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => setKind(ALL_KINDS)}
                    >
                      {m.notif_filter_reset()}
                    </Button>
                  }
                />
              ) : (
                <ul className="flex flex-col gap-3">
                  {items.map((notification) => {
                    const unread = notification.read_at == null
                    const target = notificationTarget(notification)
                    const body = (
                      <>
                        <span className={unread ? "font-medium" : undefined}>
                          {notificationText(notification)}
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
                        data-unread={unread}
                        className="flex flex-wrap items-start gap-x-3 gap-y-2 rounded-lg border p-4 data-[unread=false]:text-muted-foreground"
                      >
                        {target === undefined ? (
                          <span className="flex min-w-0 flex-1 flex-col gap-1">
                            {body}
                          </span>
                        ) : (
                          // Уведомление ведет к своему предмету: до сих пор
                          // «вы допущены к торгам» было текстом, и заявку
                          // приходилось искать руками
                          <Link
                            to={target}
                            className="flex min-w-0 flex-1 flex-col gap-1 underline-offset-4 hover:underline"
                          >
                            {body}
                          </Link>
                        )}
                        {unread && (
                          <Button
                            variant="ghost"
                            size="sm"
                            onClick={() => readOne.mutate(notification.id)}
                            disabled={
                              readOne.isPending &&
                              readOne.variables === notification.id
                            }
                          >
                            {m.notif_mark_read()}
                          </Button>
                        )}
                      </li>
                    )
                  })}
                </ul>
              )}

              {/* Сервер отдает страницу в 50 записей и курсор продолжения,
                  но `notificationsQuery` его не запрашивает. Молчать об этом
                  нельзя: «уведомлений больше нет» и «показаны последние» -
                  разные утверждения */}
              {page.next_after != null && (
                <p className="text-sm text-muted-foreground">
                  {m.notif_page_truncated({ count: page.items.length })}
                </p>
              )}
            </div>
          )
        }}
      </QueryBoundary>
    </PageShell>
  )
}
