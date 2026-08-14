import { Link } from "@tanstack/react-router"

import { Panel } from "@/components/panel"
import { Badge } from "@/components/ui/badge"
import { useNowMs } from "@/hooks/use-now"
import { formatRelative } from "@/lib/relative-time"

import type { ReactNode } from "react"

/** Сколько строк очереди показывать на обзоре: остальное - по ссылке. */
const TOP = 3

export type QueueItem = {
  id: string
  label: string
  /** Адрес самой записи готовой строкой; без него строка остается текстом */
  to?: string | undefined
  /** Метка времени записи (ISO): рисуется относительной подписью */
  at?: string | null | undefined
  /** Приписка справа от подписи - статус, сумма */
  meta?: ReactNode | undefined
}

/**
 * Очередь работы на обзорной странице: сколько всего, три верхние записи и
 * ссылка на полный список.
 *
 * Смысл ровно в том, чтобы не открывать раздел ради вопроса «есть ли там
 * что-нибудь». Полный список остается на своей странице - карточка не
 * пытается стать реестром.
 *
 * Относительное время дорисовывается после монтирования: `Date.now()` в
 * разметке SSR разошелся бы с браузерным при гидратации, а кэш отдал бы
 * протухшую подпись следующему посетителю (NFR-03).
 */
export function QueueCard({
  title,
  count,
  items,
  seeAll,
  empty,
  className,
}: {
  title: string
  /** Число в значке: полный размер очереди, а не длина показанного среза */
  count: number
  items: readonly QueueItem[]
  seeAll?: { to: string; label: string } | undefined
  /** Подпись, когда очередь пуста */
  empty: string
  className?: string | undefined
}) {
  const nowMs = useNowMs()
  const shown = items.slice(0, TOP)

  return (
    <Panel
      title={title}
      titleAs="h3"
      className={className}
      actions={
        <Badge
          variant={count > 0 ? "info" : "neutral"}
          className="tabular-nums"
        >
          {count}
        </Badge>
      }
      contentClassName="flex flex-col gap-2"
    >
      {shown.length === 0 ? (
        <p className="text-sm text-muted-foreground">{empty}</p>
      ) : (
        <ul className="flex flex-col gap-1.5">
          {shown.map((item) => (
            <li
              key={item.id}
              className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 text-sm"
            >
              {item.to === undefined ? (
                <span className="min-w-0 flex-1 truncate">{item.label}</span>
              ) : (
                <Link
                  to={item.to}
                  className="min-w-0 flex-1 truncate underline-offset-4 hover:underline"
                >
                  {item.label}
                </Link>
              )}
              {item.meta}
              {nowMs !== null && item.at != null && (
                <span className="shrink-0 text-xs text-muted-foreground">
                  {formatRelative(item.at, nowMs)}
                </span>
              )}
            </li>
          ))}
        </ul>
      )}
      {seeAll !== undefined && (
        <Link
          to={seeAll.to}
          className="text-sm text-primary underline-offset-4 hover:underline"
        >
          {seeAll.label}
        </Link>
      )}
    </Panel>
  )
}
