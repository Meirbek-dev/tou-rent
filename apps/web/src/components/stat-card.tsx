import { Link } from "@tanstack/react-router"

import { Card } from "@/components/ui/card"
import { cn } from "@/lib/utils"

import type { DeadlineUrgency } from "@/lib/relative-time"
import type { ReactNode } from "react"

/**
 * Одно число обзорной страницы: подпись, сама величина и, если она о сроке, -
 * тон срочности.
 *
 * Кабинеты открывались списком ссылок, и «сколько у меня сейчас работы»
 * приходилось узнавать, пройдя по каждой. Карточка отвечает на этот вопрос
 * до перехода. Величина набирается `.num` - на обзоре рядом стоят числа
 * разной длины, и они обязаны выстраиваться в столбец.
 *
 * Тон берется из `deadlineUrgency`, а не задается словом на месте вызова:
 * порог «скоро» один на весь портал (@/lib/relative-time).
 */
const TONE: Record<DeadlineUrgency, string> = {
  overdue: "border-destructive/30 bg-destructive/10",
  soon: "border-amber-500/40 bg-amber-500/10",
  normal: "",
}

const FIGURE_TONE: Record<DeadlineUrgency, string> = {
  overdue: "text-destructive",
  soon: "text-amber-700 dark:text-amber-400",
  normal: "",
}

export function StatCard({
  label,
  value,
  hint,
  urgency = "normal",
  to,
  className,
  "data-testid": testId,
}: {
  label: string
  /** Величина: число, сумма, срок - уже отформатированные вызывающим */
  value: ReactNode
  /** Уточнение под величиной: единица, оговорка о неполноте счета */
  hint?: string | undefined
  urgency?: DeadlineUrgency | undefined
  /** Адрес раздела, к которому относится число - готовой строкой */
  to?: string | undefined
  className?: string | undefined
  "data-testid"?: string | undefined
}) {
  const body = (
    <>
      <span className="text-sm text-muted-foreground">{label}</span>
      <span
        className={cn(
          "text-3xl leading-none font-semibold tabular-nums",
          FIGURE_TONE[urgency]
        )}
      >
        {value}
      </span>
      {hint !== undefined && (
        <span className="text-xs text-muted-foreground">{hint}</span>
      )}
    </>
  )

  return (
    <Card
      data-slot="stat-card"
      data-testid={testId}
      className={cn("gap-1.5 px-(--card-spacing)", TONE[urgency], className)}
    >
      {to === undefined ? (
        <span className="flex flex-col gap-1.5">{body}</span>
      ) : (
        // Ссылка накрывает карточку целиком: число и есть кнопка перехода
        <Link
          to={to}
          className="flex flex-col gap-1.5 rounded-lg outline-offset-4 hover:[&>span:nth-child(2)]:underline"
        >
          {body}
        </Link>
      )}
    </Card>
  )
}
