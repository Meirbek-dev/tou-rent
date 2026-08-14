import { useEffect, useState } from "react"

import { m } from "#/paraglide/messages"
import { formatDateTime } from "@/lib/format"
import { cn } from "@/lib/utils"

const HOUR = 60 * 60 * 1000
/** Порог «горит»: меньше трех суток до конца приема заявок. */
const URGENT_HOURS = 72
/** Ниже этого срока считать в сутках бессмысленно - показываем часы. */
const HOURS_MODE = 48

/**
 * Остаток срока - только на клиенте.
 *
 * На сервере «осталось N дней» посчитать нечем: серверный `Date.now()` попал
 * бы в разметку и разошелся бы с клиентским при гидратации, а кэш SSR отдал бы
 * протухшее число следующему посетителю (NFR-03). Поэтому абсолютная дата -
 * всегда в разметке (она и есть юридически значимый срок и работает без JS,
 * NFR-04), а остаток дорисовывается после монтирования.
 */
function useRemainingMs(iso: string | null | undefined): number | null {
  const [remaining, setRemaining] = useState<number | null>(null)

  useEffect(() => {
    if (iso === null || iso === undefined || iso === "") return undefined
    const deadline = new Date(iso).getTime()
    if (!Number.isFinite(deadline)) return undefined

    const tick = () => setRemaining(deadline - Date.now())
    tick()
    // Минуты достаточно: подпись меняется в часах и сутках
    const timer = setInterval(tick, 60_000)
    return () => clearInterval(timer)
  }, [iso])

  return remaining
}

function remainingLabel(ms: number): string {
  if (ms <= 0) return m.tender_closed()
  const hours = Math.floor(ms / HOUR)
  if (hours < HOURS_MODE) {
    return m.tender_closes_in_hours({ hours: Math.max(hours, 1) })
  }
  return m.tender_closes_in_days({ days: Math.floor(hours / 24) })
}

/**
 * Срок приема заявок: подпись, абсолютная дата и (после монтирования)
 * остаток. Один и тот же язык в строке реестра и в карточке тендера -
 * `size="lg"` меняет только масштаб.
 */
export function DeadlineBlock({
  value,
  label,
  size = "default",
  className,
}: {
  value: string | null | undefined
  label?: string | undefined
  size?: "default" | "lg"
  className?: string
}) {
  const absolute = formatDateTime(value)
  const remaining = useRemainingMs(value)
  const urgent =
    remaining !== null && remaining > 0 && remaining <= URGENT_HOURS * HOUR

  return (
    <div className={cn("flex flex-col gap-0.5", className)}>
      <span className="text-xs text-muted-foreground">
        {label ?? m.tender_deadline()}
      </span>
      {/* Intl-вывод может отличаться между SSR и браузерами с урезанным ICU */}
      <span
        className={cn(
          "font-medium tabular-nums",
          size === "lg" ? "text-base" : "text-sm"
        )}
        suppressHydrationWarning
      >
        {absolute ?? m.tender_date_tbd()}
      </span>
      {remaining !== null && (
        <span
          className={cn(
            "text-xs",
            urgent
              ? "font-semibold text-amber-700 dark:text-amber-400"
              : "text-muted-foreground"
          )}
        >
          {remainingLabel(remaining)}
        </span>
      )}
    </div>
  )
}
