import { createFileRoute } from "@tanstack/react-router"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { notificationText } from "@/components/notification-bell"
import { Button } from "@/components/ui/button"
import { formatDateTime } from "@/lib/format"
import { markRead, notificationsQuery } from "@/lib/notifications"

// Страница истории уведомлений (FR-1301): полный список получателя,
// непрочитанные выделены, отметка о прочтении.
export const Route = createFileRoute("/app/notifications")({
  loader: async ({ context }) => {
    await context.queryClient.ensureQueryData(notificationsQuery)
  },
  component: NotificationsPage,
})

function NotificationsPage() {
  const queryClient = useQueryClient()
  const { data: page } = useSuspenseQuery(notificationsQuery)

  const readAll = useMutation({
    mutationFn: () => markRead(),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["notifications"] })
    },
  })

  const hasUnread = page.items.some((n) => n.read_at == null)

  return (
    <main className="mx-auto flex w-full max-w-3xl flex-col gap-6 px-6 py-8">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="font-heading text-2xl font-semibold">
          {m.notif_title()}
        </h2>
        {hasUnread && (
          <Button
            variant="outline"
            size="sm"
            onClick={() => readAll.mutate()}
            disabled={readAll.isPending}
          >
            {m.notif_mark_all_read()}
          </Button>
        )}
      </header>

      {page.items.length === 0 ? (
        <p className="text-muted-foreground">{m.notif_empty()}</p>
      ) : (
        <ul className="flex flex-col gap-3">
          {page.items.map((notification) => (
            <li
              key={notification.id}
              className="flex flex-col gap-1 rounded-lg border p-4"
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
    </main>
  )
}
