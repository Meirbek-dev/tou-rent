import { cn } from "@/lib/utils"

/**
 * Сообщение формы как поверхность, а не как красная строчка под полем.
 *
 * Ошибка озвучивается сразу (`role="alert"`), успех - не перебивая
 * (`role="status"`).
 */
export function FormAlert({
  id,
  tone = "error",
  className,
  children,
}: {
  id?: string
  tone?: "error" | "success"
  className?: string
  children: React.ReactNode
}) {
  const error = tone === "error"
  return (
    <p
      id={id}
      role={error ? "alert" : "status"}
      className={cn(
        "rounded-lg px-3 py-2 text-sm ring-1",
        error
          ? "bg-destructive/10 text-destructive ring-destructive/25"
          : "bg-emerald-500/10 text-emerald-700 ring-emerald-500/25 dark:text-emerald-400",
        className
      )}
    >
      {children}
    </p>
  )
}
