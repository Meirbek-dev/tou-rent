import { queryOptions } from "@tanstack/react-query"

import { m } from "#/paraglide/messages"
// `#/` вместо `@/`: этот псевдоним разрешает и раннер тестов (см. `lib/auth.ts`)
import { api } from "#/lib/api"
import { serverLabel } from "#/lib/server-label"

import type { components } from "@tou/api-client"

export type PublicRecord = components["schemas"]["PublicRecordDto"]
export type PendingPublication = components["schemas"]["PendingPublicationDto"]

async function mutate<T>(promise: Promise<{ data?: T; error?: unknown }>) {
  const { data, error } = await promise
  if (error !== undefined || data === undefined) {
    throw error ?? new Error("request failed")
  }
  return data
}

/**
 * Реестр публикаций особого порядка (FR-1403, п. 90, 92, 97) - портал.
 *
 * Отдается страница целиком: публикация материала - обязанность из Правил,
 * и молча показать часть реестра значит соврать о полноте публикации.
 */
export const publicRecordsQuery = queryOptions({
  queryKey: ["public-records"],
  queryFn: () => mutate(api.GET("/api/v1/public-records")),
})

/**
 * Что ждет публикации (п. 97): рабочий список подразделения - страницей.
 *
 * Отдается страница целиком: `truncated` показывает панель публикаций,
 * и для рабочего списка это не формальность - поднятый признак значит,
 * что материалов накопилось больше, чем подразделение видит.
 */
export const pendingPublicationsQuery = queryOptions({
  queryKey: ["public-records", "pending"],
  queryFn: () => mutate(api.GET("/api/v1/public-records/pending")),
})

/** Публикация материала на портале (FR-1403, п. 97). */
export const publishRecord = (kind: string, sourceId: string) =>
  mutate(
    api.POST("/api/v1/public-records", {
      body: { kind, source_id: sourceId },
    })
  )

/**
 * Подпись вида публикации (FR-1403, п. 90, 92, 97).
 *
 * Вид закрыт перечнем `PublicRecordKind`, но в DTO рядом с ним едет только
 * `kind_title_ru`: подпись раздела по-русски для делопроизводства. Портал
 * рисовал ее напрямую, и на /en/ и /kk/ бейдж карточки оставался русским -
 * колонок `*_kk`/`*_en` у публикаций нет и не будет, вид не справочник
 * админа, а константа домена (NFR-01).
 *
 * Неизвестный вид откатывается на серверную подпись: контракт может
 * опередить интерфейс, и пустой бейдж хуже русского.
 */
const KIND_LABELS: Record<string, (() => string) | undefined> = {
  decision: m.public_record_kind_decision,
  rate: m.public_record_kind_rate,
  investment_act: m.public_record_kind_investment_act,
}

export function publicRecordKindLabel(record: {
  kind: string
  kind_title_ru: string
}): string {
  return KIND_LABELS[record.kind]?.() ?? record.kind_title_ru
}

/**
 * Заголовок материала на портале (FR-1403, п. 90, 97).
 *
 * У результата рассмотрения он собирается из подписи категории справочника
 * в момент публикации, и переводы едут колонками `title_kk` / `title_en`;
 * пусто - перевода нет, показываем `title` (NFR-01, G-066).
 */
export function publicRecordTitle(record: {
  title: string
  title_kk?: string | null
  title_en?: string | null
}): string {
  return serverLabel(record, "title")
}

/**
 * Чем очередь публикаций отличает материал от соседнего (п. 97).
 *
 * Заголовок у обоснований ставки один на все договоры особого порядка, и
 * очередь из нескольких строк была неразличима (G-134). Различают договор
 * его регистрационный номер, объект и вторая сторона - те же реквизиты,
 * которыми материал называют в делопроизводстве. Чего в заявке нет
 * (номер до регистрации, объект у заявки без объекта), в подпись не идет.
 */
export function publicationSubject(item: {
  reference?: string | null
  object_name?: string | null
  party_name?: string | null
}): string {
  return [item.reference, item.object_name, item.party_name]
    .map((part) => part?.trim() ?? "")
    .filter((part) => part !== "")
    .join(" · ")
}
