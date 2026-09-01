import { m } from "#/paraglide/messages"
import { EmptyState } from "@/components/empty-state"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { problemDetail } from "@/lib/auth"
import { cn } from "@/lib/utils"
import { TriangleAlertIcon } from "lucide-react"

import type { LucideIcon } from "lucide-react"
import type { ReactNode } from "react"

/**
 * Ровно та часть результата `useQuery`, которая нужна для показа состояния.
 * Не `UseQueryResult<TData>`: тогда в сигнатуру пришлось бы тянуть тип ошибки
 * и тип select-а каждого вызова, а панели различаются именно ими.
 */
export type QueryLike<TData> = {
  data: TData | undefined
  isPending: boolean
  isError: boolean
  error: unknown
  refetch: () => unknown
}

type EmptyProps<TData> = {
  /** Когда данные пришли, но показывать нечего */
  when: (data: TData) => boolean
  icon?: LucideIcon | undefined
  title: string
  description?: string | undefined
  action?: ReactNode | undefined
}

/**
 * Загрузка, отказ и пустой результат запроса - тремя разными состояниями.
 *
 * Панели кабинетов писались по образцу `const { data } = useQuery(...)` и
 * `if (data == null) return <p>Ничего нет</p>`, а это ложь: пока запрос идет,
 * `data` тоже `undefined`, и экран сообщает «записей нет» вместо «грузим».
 * Отказ сети выглядел там же и так же - пустотой без единой кнопки. Здесь
 * три состояния разведены по построению, и обойти это на месте вызова
 * нельзя: `children` получают уже загруженные данные.
 */
export function QueryBoundary<TData>({
  query,
  skeleton,
  empty,
  children,
  className,
}: {
  query: QueryLike<TData>
  /** Заглушка под форму будущего содержимого; по умолчанию - три строки */
  skeleton?: ReactNode | undefined
  empty?: EmptyProps<TData> | undefined
  children: (data: TData) => ReactNode
  className?: string
}) {
  if (query.isPending) {
    return <>{skeleton ?? <DefaultSkeleton className={className} />}</>
  }

  // `data === undefined` при неподнятом isError - это отказ select-а или
  // отмененный запрос: показывать пустоту нельзя и здесь
  if (query.isError || query.data === undefined) {
    return <QueryError error={query.error} onRetry={query.refetch} />
  }

  const data = query.data
  if (empty !== undefined && empty.when(data)) {
    return (
      <EmptyState
        icon={empty.icon}
        title={empty.title}
        description={empty.description}
        action={empty.action}
        className={className}
      />
    )
  }

  return <>{children(data)}</>
}

function DefaultSkeleton({ className }: { className?: string | undefined }) {
  return (
    <div className={cn("flex flex-col gap-2", className)} aria-hidden="true">
      <Skeleton className="h-16 w-full rounded-xl" />
      <Skeleton className="h-16 w-full rounded-xl" />
      <Skeleton className="h-16 w-full rounded-xl" />
    </div>
  )
}

/**
 * Отказ запроса как поверхность с действием.
 *
 * Подробность показывается только тогда, когда сервер прислал разбираемую
 * причину (problem+json, RFC 9457). Служебный текст исключения наружу не
 * идет: он ничего не говорит пользователю и может нести адрес запроса.
 */
function QueryError({
  error,
  onRetry,
}: {
  error: unknown
  onRetry: () => unknown
}) {
  // Признак «есть что показать» - отдельный, а не сравнение с литералом:
  // раньше служебное слово отсеивалось строкой `detail !== "unknown_error"`,
  // и любая правка текста молча возвращала его на экран
  const detail = problemDetail(error)

  return (
    <div
      role="alert"
      data-testid="query-error"
      className="flex flex-col items-center gap-3 rounded-xl border border-destructive/25 bg-destructive/10 px-6 py-10 text-center"
    >
      <TriangleAlertIcon
        aria-hidden="true"
        className="size-6 text-destructive"
      />
      <p className="font-heading text-base font-semibold text-destructive">
        {m.query_error_title()}
      </p>
      {detail !== null && (
        <p className="max-w-[46ch] text-sm text-balance text-destructive/90">
          {detail}
        </p>
      )}
      <Button
        variant="outline"
        size="sm"
        className="mt-1"
        onClick={() => void onRetry()}
      >
        {m.query_error_retry()}
      </Button>
    </div>
  )
}
