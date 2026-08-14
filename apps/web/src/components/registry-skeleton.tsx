import { m } from "#/paraglide/messages"
import { PublicShell } from "@/components/public-shell"
import { Skeleton } from "@/components/ui/skeleton"

/**
 * Заглушка реестра на время загрузки маршрута.
 *
 * Геометрия повторяет строку реестра (содержимое, срок справа), чтобы
 * список не прыгал при появлении данных. На первую отрисовку SSR это
 * не влияет - загрузчик уже отработал; заглушка видна при переходах внутри
 * приложения.
 */
export function RegistrySkeleton({ rows = 5 }: { rows?: number }) {
  return (
    <ul className="flex flex-col gap-2" aria-hidden="true">
      {Array.from({ length: rows }, (_, index) => (
        <li
          key={index}
          className="grid grid-cols-[minmax(0,1fr)] overflow-hidden rounded-xl border bg-card sm:grid-cols-[minmax(0,1fr)_auto]"
        >
          <div className="flex flex-col gap-3 p-4 sm:p-5">
            <div className="flex items-center gap-3">
              <Skeleton className="h-6 w-28 rounded-full" />
              <Skeleton className="h-4 w-16" />
            </div>
            <Skeleton className="h-5 w-2/3" />
          </div>
          <div className="flex flex-col gap-2 border-t px-4 pt-3 pb-4 sm:col-start-2 sm:w-[9rem] sm:border-t-0 sm:border-l sm:px-5 sm:py-5">
            <Skeleton className="h-3 w-20" />
            <Skeleton className="h-4 w-28" />
          </div>
        </li>
      ))}
    </ul>
  )
}

/** Ожидание маршрута реестра целиком: каркас страницы плюс строки-заглушки. */
export function RegistryPending({ title }: { title: string }) {
  return (
    <PublicShell>
      <div className="flex flex-col gap-6">
        <h1 className="text-3xl font-semibold tracking-tight">{title}</h1>
        <p className="sr-only" role="status">
          {m.registry_loading()}
        </p>
        <RegistrySkeleton />
      </div>
    </PublicShell>
  )
}
