import { useId } from "react"

import { Card, CardAction, CardContent, CardHeader } from "@/components/ui/card"
import { cn } from "@/lib/utils"

import type { ReactNode } from "react"

/**
 * Раздел экрана как поверхность с заголовком.
 *
 * Кабинеты набраны десятками копий `<section className="rounded-lg border
 * p-4">` с `<h3>` внутри - у каждой свой отступ, своя рамка и свой уровень
 * заголовка. Здесь одна карточка, один радиус (панель - `rounded-xl`) и
 * связь `aria-labelledby` между областью и ее заголовком, которой в тех
 * копиях не было.
 *
 * `data-slot` проставлены вручную вместо `CardTitle`/`CardDescription`:
 * сетка `CardHeader` опирается на эти атрибуты, а заголовок обязан быть
 * настоящим `h2`/`h3`, а не `div` с классом.
 */
export function Panel({
  title,
  titleAs: Title = "h2",
  description,
  actions,
  children,
  className,
  contentClassName,
}: {
  title: string
  titleAs?: "h2" | "h3"
  description?: string | undefined
  /** Кнопки раздела: правый верхний угол шапки */
  actions?: ReactNode | undefined
  children: ReactNode
  className?: string | undefined
  contentClassName?: string | undefined
}) {
  const headingId = useId()

  return (
    <Card
      role="region"
      aria-labelledby={headingId}
      className={cn("gap-4", className)}
    >
      <CardHeader>
        <Title
          id={headingId}
          data-slot="card-title"
          className="font-heading text-base leading-snug font-medium"
        >
          {title}
        </Title>
        {description !== undefined && (
          <div
            data-slot="card-description"
            className="text-sm text-muted-foreground"
          >
            {description}
          </div>
        )}
        {actions !== undefined && <CardAction>{actions}</CardAction>}
      </CardHeader>
      <CardContent className={contentClassName}>{children}</CardContent>
    </Card>
  )
}
