/**
 * Кабинет Правления: принятые решения (FR-1202, п. 90).
 *
 * Кабинет показывал только очередь: как только решение принято, заявка
 * пропадала с экрана и открыть ее было негде - ни проверить формулировку,
 * ни дойти до публикации. Перечень состояний закрыт п. 90, а контракт
 * фильтрует список по одному состоянию за запрос, поэтому решенные
 * собираются из трех выборок и сортируются здесь.
 */

/** Состояния, в которых решение Правления уже принято (п. 90). */
export const DECIDED_STATUSES = ["granted", "refused", "redirected"] as const

/** Строка перечня в том объеме, в каком ее сортирует эта функция. */
export type DecidedRow = { id: string; submitted_at: string }

/**
 * Решенные заявки одним списком, свежие сверху.
 *
 * Выборки приходят порознь и могут прийти не все (запрос по одному
 * состоянию мог не ответить) - `undefined` пропускается, а не роняет
 * список. Совпадения по `id` схлопываются: одна и та же заявка не может
 * стоять в двух состояниях, но защита стоит копейки, а дубль в перечне
 * решений читается как два разных решения.
 */
export function mergeDecided<T extends DecidedRow>(
  lists: readonly (readonly T[] | undefined)[]
): T[] {
  const known = new Map<string, T>()
  for (const list of lists) {
    for (const row of list ?? []) {
      if (!known.has(row.id)) known.set(row.id, row)
    }
  }
  return [...known.values()].toSorted((left, right) =>
    right.submitted_at.localeCompare(left.submitted_at)
  )
}
