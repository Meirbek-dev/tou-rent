import type { TenderStatus } from "@/lib/api"

/**
 * Срок приема заявок относительно «сейчас» (FR-401, п. 45).
 *
 * Момент отсчета приходит параметром, а не берется из `Date.now()` внутри:
 * серверное «сейчас» попало бы в разметку SSR и разошлось бы с браузерным
 * при гидратации (NFR-03, см. `useNowMs`). До первого кадра его нет вовсе -
 * `null`, - и это намеренно значит «срок не истек»: разметка сервера обязана
 * совпасть с первым рендером браузера.
 */

/** Срок вышел. `null` в `nowMs` - до монтирования, срок еще не считается. */
export function deadlinePassed(
  deadline: string | null | undefined,
  nowMs: number | null
): boolean {
  if (nowMs === null || deadline == null || deadline === "") return false
  const end = new Date(deadline).getTime()
  return Number.isFinite(end) && end <= nowMs
}

/**
 * Прием закрыт, а вскрытия еще не было: статус переводит процедура, и до нее
 * карточка утверждала два противоположных факта разом - зеленое «Прием
 * заявок» рядом с «Прием заявок закрыт» из блока срока (FR-501).
 */
export function intakeClosed(
  status: TenderStatus,
  deadline: string | null | undefined,
  nowMs: number | null
): boolean {
  return status === "accepting" && deadlinePassed(deadline, nowMs)
}
