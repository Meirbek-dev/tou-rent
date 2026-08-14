import { useQuery } from "@tanstack/react-query"

import { unreadCountQuery } from "@/lib/notifications"
import { pendingSpecialRequestsQuery } from "@/lib/special"

/**
 * Счетчики очередей у пунктов боковой навигации.
 *
 * Правило отбора одно: счетчик показывается только там, где число уже
 * получается из запроса, который кабинет и так делает. Заводить ради значка
 * новый маршрут или тянуть страницу реестра, чтобы посчитать в ней строки, -
 * значит платить сетевым запросом на каждом экране за украшение шапки.
 *
 * Поэтому счетчиков ровно два:
 *   - непрочитанные уведомления (`/notifications/unread-count` - счетчик и
 *     есть весь ответ; колокольчик в той же шапке уже держит его в кеше);
 *   - заявки особого порядка в рассмотрении - общий рабочий список
 *     организатора и Правления (`/special-requests`, один ответ без
 *     листания; обе страницы грузят его же).
 *
 * Осознанно пропущены:
 *   - комиссия: «мои неподанные голоса» отдельным маршрутом не существуют,
 *     их пришлось бы собирать по заявкам каждого тендера;
 *   - финансы: подтверждение взноса делается по идентификатору заявки
 *     вручную (FR-405), очереди на сервере нет;
 *   - секретарь: список тендеров - страница с листанием, а не очередь;
 *   - участник: свои заявки - не рабочая очередь, а история.
 * TODO-ENGINEER: когда появятся маршруты-счетчики для комиссии и финансов,
 * их место здесь.
 *
 * Отказ запроса гасит счетчик: значок - подсказка, и ошибка сети не должна
 * ни ронять навигацию, ни рисовать «0» там, где число неизвестно.
 */
export function useQueueCounts(
  roles: readonly string[]
): Record<string, number> {
  const wantsSpecial = roles.includes("organizer") || roles.includes("board")

  const unread = useQuery({ ...unreadCountQuery, staleTime: 30_000 })
  const special = useQuery({
    ...pendingSpecialRequestsQuery,
    enabled: wantsSpecial,
    staleTime: 60_000,
    select: (items) => items.length,
  })

  const counts: Record<string, number> = {}

  if (typeof unread.data === "number" && unread.data > 0) {
    counts["/app/notifications"] = unread.data
  }

  const pending = special.data ?? 0
  if (pending > 0) {
    if (roles.includes("organizer")) counts["/app/organizer/special"] = pending
    if (roles.includes("board")) counts["/app/board"] = pending
  }

  return counts
}
