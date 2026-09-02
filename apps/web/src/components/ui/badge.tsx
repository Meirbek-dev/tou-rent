import { mergeProps } from "@base-ui/react/merge-props"
import { useRender } from "@base-ui/react/use-render"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

const badgeVariants = cva(
  "group/badge inline-flex h-5 w-fit shrink-0 items-center justify-center gap-1 overflow-hidden rounded-4xl border border-transparent px-2 py-0.5 text-xs font-medium whitespace-nowrap transition-all focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 aria-invalid:border-destructive aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 [&>svg]:pointer-events-none [&>svg]:size-3!",
  {
    variants: {
      variant: {
        default: "bg-primary text-primary-foreground [a]:hover:bg-primary/80",
        secondary:
          "bg-secondary text-secondary-foreground [a]:hover:bg-secondary/80",
        destructive:
          // Подложка в темной теме осветлена до 10 %: на 20 % красный текст
          // давал 4.09:1 против 4.63:1 сейчас (NFR-04, гейт G17). Светлее делать
          // сам цвет нельзя — он же фон сплошной кнопки со светлой подписью
          "bg-destructive/10 text-destructive focus-visible:ring-destructive/20 dark:bg-destructive/10 dark:focus-visible:ring-destructive/40 [a]:hover:bg-destructive/20",
        outline:
          "border-border text-foreground [a]:hover:bg-muted [a]:hover:text-muted-foreground",
        ghost:
          "hover:bg-muted hover:text-muted-foreground dark:hover:bg-muted/50",
        link: "text-primary underline-offset-4 hover:underline",
        // Семантические статусы доменных бейджей (tender/application/object):
        // те же токены, что и в панелях, но в стоковом визуальном языке
        // бейджа - подложка-заливка на 10-15 %, как у destructive выше
        success:
          "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400 [a]:hover:bg-emerald-500/20",
        // В светлой теме текст на ступень темнее (amber-800): amber-700 на
        // 15-процентной подложке давал ровно 4.5:1 и в замере axe (гейт G17)
        // округлялся вниз - бейджи «идет тендер» и «в тендере» отбивали
        // страницы реестра и свободных площадей
        warning:
          "bg-amber-500/15 text-amber-800 dark:text-amber-400 [a]:hover:bg-amber-500/25",
        info: "bg-primary/10 text-primary [a]:hover:bg-primary/20",
        neutral:
          "bg-muted text-muted-foreground dark:bg-muted/50 [a]:hover:bg-muted/80",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
)

function Badge({
  className,
  variant = "default",
  render,
  ...props
}: useRender.ComponentProps<"span"> & VariantProps<typeof badgeVariants>) {
  return useRender({
    defaultTagName: "span",
    props: mergeProps<"span">(
      {
        className: cn(badgeVariants({ variant }), className),
      },
      props
    ),
    render,
    state: {
      slot: "badge",
      variant,
    },
  })
}

export { Badge, badgeVariants }
