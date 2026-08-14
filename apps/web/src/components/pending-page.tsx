import { Skeleton } from "@/components/ui/skeleton"

/**
 * Заглушка страницы на время работы загрузчика маршрута.
 *
 * Без нее переход между страницами кабинета выглядел как зависший интерфейс:
 * старый экран стоял на месте, пока не приедут данные нового, и щелчок
 * казался непринятым. Форма заглушки повторяет форму страницы - заголовок
 * и несколько карточек, - чтобы содержимое встало на место, а не сдвинуло
 * все вниз (CLS).
 */
export function PendingPage() {
  return (
    <div
      className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-4 py-6 sm:px-6 lg:py-8"
      aria-hidden="true"
      data-testid="pending-page"
    >
      <Skeleton className="h-8 w-64" />
      <div className="flex flex-col gap-3">
        <Skeleton className="h-24 w-full rounded-xl" />
        <Skeleton className="h-24 w-full rounded-xl" />
        <Skeleton className="h-24 w-full rounded-xl" />
      </div>
    </div>
  )
}
