import { Link, createFileRoute, redirect } from "@tanstack/react-router"
import { useQuery } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { MyDeadlines } from "@/components/my-deadlines"
import {
  notificationTarget,
  notificationText,
} from "@/components/notification-bell"
import { PageHeader } from "@/components/page-header"
import { PageShell } from "@/components/page-shell"
import { QueueCard } from "@/components/queue-card"
import { StatCard } from "@/components/stat-card"
import { cabinetLabel, userCabinets } from "@/lib/auth"
import { notificationsQuery } from "@/lib/notifications"
import { useQueueCounts } from "@/lib/queues"

import type { QueueItem } from "@/components/queue-card"

// Единственная роль - сразу в свой кабинет; несколько - выбор (ТЗ § 8).
export const Route = createFileRoute("/app/")({
  beforeLoad: ({ context }) => {
    const cabinets = userCabinets(context.user.roles)
    const only = cabinets.length === 1 ? cabinets[0] : undefined
    if (only !== undefined) {
      throw redirect({ to: only.path })
    }
  },
  head: () => ({ meta: [{ title: `${m.app_dashboard_title()} - ToU Rent` }] }),
  component: AppHome,
})

/**
 * Сводка человека, у которого несколько ролей.
 *
 * Раньше это была решетка ссылок на кабинеты: чтобы узнать, есть ли где-то
 * работа, приходилось открыть каждый. Теперь сперва показываются очереди -
 * ровно те счетчики, которые кабинет и так держит в кеше
 * (@/lib/queues), - и только потом переход по кабинетам.
 *
 * Новых запросов страница не заводит: и счетчик непрочитанных, и список
 * уведомлений уже загружены колокольчиком в шапке каркаса.
 */
function AppHome() {
  const { user } = Route.useRouteContext()
  const cabinets = userCabinets(user.roles)
  const counts = useQueueCounts(user.roles)
  const notifications = useQuery(notificationsQuery)

  const stats = queueStats(user.roles, counts)
  const recent: QueueItem[] = (notifications.data?.items ?? [])
    .slice(0, 5)
    .map((notification) => ({
      id: notification.id,
      label: notificationText(notification),
      to: notificationTarget(notification),
      at: notification.created_at,
    }))

  return (
    <PageShell>
      <PageHeader
        title={m.app_dashboard_title()}
        description={m.dash_subtitle()}
      />

      <section aria-labelledby="dash-needs-you" className="flex flex-col gap-3">
        <h2 id="dash-needs-you" className="font-heading text-lg font-semibold">
          {m.dash_needs_you()}
        </h2>
        {stats.length === 0 ? (
          <p className="text-sm text-muted-foreground">{m.dash_all_clear()}</p>
        ) : (
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {stats.map((stat) => (
              <StatCard
                key={stat.to}
                label={stat.label}
                value={stat.count}
                to={stat.to}
                urgency={stat.count > 0 ? "soon" : "normal"}
              />
            ))}
          </div>
        )}
      </section>

      <MyDeadlines />

      <QueueCard
        title={m.dash_recent_notifications()}
        count={counts["/app/notifications"] ?? 0}
        items={recent}
        empty={m.notif_empty()}
        seeAll={{ to: "/app/notifications", label: m.notif_view_all() }}
      />

      <section aria-labelledby="dash-cabinets" className="flex flex-col gap-3">
        <h2 id="dash-cabinets" className="font-heading text-lg font-semibold">
          {m.dash_cabinets()}
        </h2>
        <ul className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {cabinets.map(({ role, path }) => (
            <li key={role}>
              <Link
                to={path}
                className="block rounded-xl border border-border bg-card p-4 font-medium shadow-xs transition-colors hover:bg-muted/50"
              >
                {cabinetLabel(role)}
              </Link>
            </li>
          ))}
        </ul>
      </section>
    </PageShell>
  )
}

/**
 * Очереди, которые уместно показать этому человеку.
 *
 * Перечень намеренно совпадает со счетчиками боковой навигации: заводить
 * ради обзора собственные подсчеты значило бы платить запросом за каждый
 * вход в кабинет (см. комментарий в @/lib/queues).
 */
function queueStats(
  roles: readonly string[],
  counts: Record<string, number>
): { to: string; label: string; count: number }[] {
  const stats = [
    { to: "/app/notifications", label: m.dash_stat_unread() },
    ...(roles.includes("organizer")
      ? [{ to: "/app/organizer/special", label: m.dash_stat_special() }]
      : []),
    ...(roles.includes("board")
      ? [{ to: "/app/board", label: m.dash_stat_board() }]
      : []),
  ]

  return stats.map((stat) => ({ ...stat, count: counts[stat.to] ?? 0 }))
}
