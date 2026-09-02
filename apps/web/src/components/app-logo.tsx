import { m } from "#/paraglide/messages"
import { cn } from "@/lib/utils"

/**
 * Знак портала: официальная эмблема университета плюс текстовая подпись.
 *
 * Эмблема лежит двумя файлами - темно-синим и белым (`/brand/tou-emblem*.png`,
 * 121×192, прозрачный фон): на просвет знак не читается ни на светлом, ни на
 * темном фоне, поэтому вариант выбирается темой через `display`. Скрытая
 * картинка выпадает из дерева доступности, и подпись `alt` стоит у обеих -
 * иначе в одной из тем у ссылки на главную не осталось бы имени (гейт G17,
 * link-name).
 *
 * Подпись набрана текстом, а не картинкой: горизонтальный локап университета
 * существует только в белом варианте и годится лишь на фирменной синей полосе
 * (см. подвал), а текст переводится вместе с остальным интерфейсом и остается
 * четким на любой плотности экрана.
 */

/**
 * Где стоит знак: боковая панель кабинета (`default` - эмблема и две строки
 * подписи), публичная шапка (`header` - без строки с названием университета:
 * с ней знак вместе с пятью разделами и кнопками справа не помещается
 * в контейнер 72rem, и подписи разделов ломались на две строки), карточка
 * входа (`auth`, крупнее), свернутая до 3rem панель кабинета (`compact` -
 * только эмблема, подпись в колонку не влезает).
 */
export type AppLogoVariant = "default" | "header" | "auth" | "compact"

/** Высота эмблемы; ширина следует пропорции файла 121:192. */
const EMBLEM_HEIGHT: Record<AppLogoVariant, string> = {
  default: "h-9",
  header: "h-9",
  auth: "h-11",
  compact: "h-8",
}

const TITLE_SIZE: Record<Exclude<AppLogoVariant, "compact">, string> = {
  default: "text-[0.9375rem]",
  header: "text-[0.9375rem]",
  auth: "text-lg",
}

export function AppLogo({
  variant = "default",
  className,
}: {
  variant?: AppLogoVariant
  className?: string
}) {
  const emblem = cn("w-auto shrink-0", EMBLEM_HEIGHT[variant])

  return (
    <span className={cn("flex items-center gap-2.5", className)}>
      <img
        src="/brand/tou-emblem.png"
        alt={m.app_name()}
        width={121}
        height={192}
        className={cn(emblem, "dark:hidden")}
      />
      <img
        src="/brand/tou-emblem-white.png"
        alt={m.app_name()}
        width={121}
        height={192}
        className={cn(emblem, "hidden dark:block")}
      />

      {variant !== "compact" && (
        <span className="flex min-w-0 flex-col leading-tight">
          <span
            className={cn(
              "truncate font-semibold tracking-tight",
              TITLE_SIZE[variant]
            )}
          >
            {m.app_name()}
          </span>
          {variant !== "header" && (
            <span className="truncate text-[0.6875rem] tracking-wide text-muted-foreground uppercase">
              {m.footer_university()}
            </span>
          )}
        </span>
      )}
    </span>
  )
}
