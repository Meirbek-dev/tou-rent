import { cn } from "@/lib/utils"

import type { ReactNode } from "react"

/**
 * Полоса содержимого страницы кабинета: ширина, поля, вертикальный ритм.
 *
 * До сих пор те же три числа были вписаны в каждый макет кабинета отдельно
 * (`mx-auto max-w-6xl px-6 py-8`), и расходились: страница отчетности жила
 * в `max-w-6xl`, страница уведомлений - в `max-w-3xl`, а страница торгов -
 * в `max-w-4xl`. Ширина - решение о читаемости, а не о вкусе каждого файла.
 *
 * `narrow` - для экранов с одной колонкой текста и формы (мера строки
 * держится в пределах 68ch), `default` - для реестров и таблиц.
 */
export function PageShell({
  children,
  width = "default",
  className,
}: {
  children: ReactNode
  width?: "default" | "narrow"
  className?: string | undefined
}) {
  return (
    <div
      data-slot="page-shell"
      className={cn(
        "mx-auto flex w-full flex-col gap-6 px-4 py-6 sm:px-6 lg:py-8",
        width === "narrow" ? "max-w-3xl" : "max-w-6xl",
        className
      )}
    >
      {children}
    </div>
  )
}
