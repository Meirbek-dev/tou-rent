import { Link } from "@tanstack/react-router"
import { useQuery } from "@tanstack/react-query"

import { m } from "#/paraglide/messages"
import { Button } from "@/components/ui/button"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { formatDateTime } from "@/lib/format"
import { deadlineLabel } from "@/lib/obligation-labels"
import { myObligationsQuery } from "@/lib/obligations"
import { deadlineUrgency, formatRelative } from "@/lib/relative-time"
import { cn } from "@/lib/utils"
import { useNowMs } from "@/hooks/use-now"
import { AlarmClockIcon } from "lucide-react"

import type { ObligationDto } from "@/lib/obligations"

/** Сколько сроков показывает всплывающий список; остальное - на «Мои сроки». */
const PREVIEW = 5

/**
 * Ближайший срок Правил - в шапке любого экрана кабинета (FR-1702).
 *
 * Раньше «Мои сроки» жили внизу дашборда: пользователь, ушедший в тендер или
 * в договор, о просроченном сроке узнавал из письма-эскалации. Здесь тот же
 * список, но всегда на виду, и тон подсказывает, горит ли он.
 *
 * Пока запрос идет, не рисуется ничего. Это не оплошность: значок - надстройка
 * над экраном, и «0 сроков» вместо «еще не знаем» - именно то ложное
 * состояние, которого стоит бояться. Отсутствие честнее.
 */
export function DeadlineChip() {
  const { data: page } = useQuery(myObligationsQuery)
  const now = useNowMs()
  const obligations = page?.items

  if (now === null || obligations === undefined || obligations.length === 0) {
    return null
  }

  const sorted = [...obligations].toSorted(
    (left, right) => Date.parse(left.due_at) - Date.parse(right.due_at)
  )
  const nearest = sorted[0]
  if (nearest === undefined) return null

  const urgency = deadlineUrgency(nearest.due_at, now)
  const relative = formatRelative(nearest.due_at, now)

  return (
    <Popover>
      <PopoverTrigger
        render={
          <Button
            variant="ghost"
            size="sm"
            data-testid="deadline-chip"
            aria-label={m.deadline_chip_label({
              when: relative,
              count: sorted.length,
            })}
            className={cn(
              "gap-1.5 px-2",
              urgency === "overdue" && "text-destructive",
              urgency === "soon" && "text-amber-700 dark:text-amber-400"
            )}
          />
        }
      >
        <AlarmClockIcon aria-hidden="true" className="size-4" />
        {/* Ниже sm остается один значок: подпись срока длиннее, чем место
            в шапке на 360 px, а сам факт «сроки есть» несет иконка и число */}
        <span
          className="hidden tabular-nums sm:inline"
          suppressHydrationWarning
        >
          {relative}
        </span>
        <span className="text-xs text-muted-foreground tabular-nums">
          {sorted.length}
        </span>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        className="w-[min(24rem,calc(100vw-2rem))] p-0"
      >
        <div className="border-b px-4 py-2 font-medium">
          {m.deadlines_title()}
        </div>
        <ul className="flex max-h-80 flex-col overflow-y-auto overscroll-contain">
          {sorted.slice(0, PREVIEW).map((obligation) => (
            <DeadlineRow
              key={obligation.id}
              obligation={obligation}
              now={now}
            />
          ))}
        </ul>
        <div className="border-t px-4 py-2">
          {/* Полный список - на дашборде: там же он и сортируется */}
          <Link
            to="/app"
            className="text-sm underline-offset-4 hover:underline"
          >
            {m.back_to_cabinet()}
          </Link>
        </div>
      </PopoverContent>
    </Popover>
  )
}

function DeadlineRow({
  obligation,
  now,
}: {
  obligation: ObligationDto
  now: number
}) {
  const urgency = deadlineUrgency(obligation.due_at, now)

  return (
    <li className="flex flex-col gap-1 border-b px-4 py-3 text-sm last:border-b-0">
      <span className="font-medium">{deadlineLabel(obligation.action)}</span>
      <span className="flex flex-wrap items-center gap-x-2 text-xs">
        <span
          className={cn(
            "tabular-nums",
            urgency === "overdue" && "font-medium text-destructive",
            urgency === "soon" &&
              "font-medium text-amber-700 dark:text-amber-400",
            urgency === "normal" && "text-muted-foreground"
          )}
          suppressHydrationWarning
        >
          {formatDateTime(obligation.due_at) ?? "-"}
        </span>
        <span className="text-muted-foreground">{obligation.rule_ref}</span>
      </span>
    </li>
  )
}
