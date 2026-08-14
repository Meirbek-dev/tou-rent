import { cn } from "@/lib/utils"

import type { LucideIcon } from "lucide-react"
import type { ReactNode } from "react"

/**
 * Пустой результат как состояние страницы, а не как оброненный абзац.
 * Говорит три вещи: что именно пусто, почему и что можно сделать дальше.
 */
export function EmptyState({
  icon: Icon,
  title,
  titleAs: Title = "p",
  description,
  action,
  className,
}: {
  icon?: LucideIcon | undefined
  title: string
  /** Заголовок страницы, если пустое состояние заменяет собой всю страницу */
  titleAs?: "p" | "h1" | "h2"
  description?: string | undefined
  action?: ReactNode | undefined
  className?: string | undefined
}) {
  return (
    <div
      className={cn(
        "flex flex-col items-center gap-3 rounded-xl border border-dashed border-border bg-muted/50 px-6 py-16 text-center",
        className
      )}
    >
      {Icon !== undefined && (
        <Icon aria-hidden="true" className="size-8 text-muted-foreground" />
      )}
      <Title className="font-heading text-base font-semibold">{title}</Title>
      {description !== undefined && (
        <p className="max-w-[46ch] text-sm text-balance text-muted-foreground">
          {description}
        </p>
      )}
      {action !== undefined && <div className="mt-1">{action}</div>}
    </div>
  )
}
