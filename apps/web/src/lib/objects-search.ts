import type { ObjectKind, ObjectStatus } from "@/lib/api"

/** Статусы витрины (FR-102/FR-103): вычисляются, вручную не редактируются. */
export const OBJECT_STATUSES: ObjectStatus[] = ["free", "in_tender", "leased"]

/** Виды имущества университета (п. 6). */
export const OBJECT_KINDS: ObjectKind[] = [
  "premises",
  "building",
  "structure",
  "land_plot",
]

export type ObjectsSearch = {
  status?: ObjectStatus
  kind?: ObjectKind
  q?: string
  area_min?: number
  area_max?: number
  after?: string
}

/**
 * Площадь фильтра - положительное число; иначе параметр отбрасывается.
 * Роутер разбирает `area_min=10` в число, нативная GET-форма дает строку -
 * принимаем оба вида и отдаем число, чтобы адрес оставался читаемым
 * (строка уехала бы в URL в кавычках).
 */
function area(value: unknown): number | undefined {
  const raw =
    typeof value === "number"
      ? String(value)
      : typeof value === "string"
        ? value.trim()
        : ""
  if (raw === "" || !/^\d+(\.\d+)?$/.test(raw)) return undefined
  const parsed = Number(raw)
  return parsed > 0 ? parsed : undefined
}

/**
 * FR-102: фильтры витрины живут в URL - ссылку можно переслать, страница
 * работает без JS (нативная GET-форма, NFR-04). «Сырые» параметры
 * нормализуются: мусор отбрасывается, а не ломает страницу.
 */
export function validateObjectsSearch(
  search: Record<string, unknown>
): ObjectsSearch {
  const status = search["status"]
  const kind = search["kind"]
  const query = search["q"]

  return {
    status: OBJECT_STATUSES.includes(status as ObjectStatus)
      ? (status as ObjectStatus)
      : undefined,
    kind: OBJECT_KINDS.includes(kind as ObjectKind)
      ? (kind as ObjectKind)
      : undefined,
    q: typeof query === "string" && query.trim() !== "" ? query : undefined,
    area_min: area(search["area_min"]),
    area_max: area(search["area_max"]),
    after:
      typeof search["after"] === "string" && search["after"] !== ""
        ? search["after"]
        : undefined,
  }
}
