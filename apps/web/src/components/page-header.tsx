import { cn } from "@/lib/utils"

import type { ReactNode } from "react"

/**
 * Шапка страницы кабинета: единственный `h1` экрана, состояние и действия.
 *
 * Раньше `h1` жил в макете кабинета и на всех его страницах читался
 * одинаково («Кабинет организатора»), а чем именно является открытая
 * страница, приходилось узнавать из `h2` в середине. Теперь имя кабинета -
 * это группа в боковой навигации, а `h1` принадлежит странице.
 *
 * `facts` - короткие пары «подпись/значение» под заголовком (номер тендера,
 * срок, площадь): их место - рядом с именем страницы, а не в первой попавшейся
 * панели.
 */
export function PageHeader({
  title,
  description,
  badge,
  facts,
  actions,
  breadcrumb,
  className,
}: {
  title: string
  description?: string | undefined
  /** Значок состояния сущности справа от заголовка */
  badge?: ReactNode | undefined
  facts?: ReactNode | undefined
  actions?: ReactNode | undefined
  /** Крошки страницы, если экран показывает их сам, а не в шапке каркаса */
  breadcrumb?: ReactNode | undefined
  className?: string | undefined
}) {
  return (
    <header
      data-slot="page-header"
      className={cn("flex flex-col gap-3", className)}
    >
      {breadcrumb}
      <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-3">
        <div className="flex min-w-0 flex-col gap-1.5">
          <div className="flex flex-wrap items-center gap-2.5">
            <h1 className="font-heading text-2xl font-semibold">{title}</h1>
            {badge}
          </div>
          {description !== undefined && (
            <p className="max-w-[68ch] text-sm text-pretty text-muted-foreground">
              {description}
            </p>
          )}
        </div>
        {actions !== undefined && (
          <div className="flex shrink-0 flex-wrap items-center gap-2">
            {actions}
          </div>
        )}
      </div>
      {facts !== undefined && (
        <div className="flex flex-wrap items-center gap-x-6 gap-y-2 text-sm">
          {facts}
        </div>
      )}
    </header>
  )
}
