import { queryOptions } from "@tanstack/react-query"

// `#/` вместо `@/`: оба псевдонима ведут в `src/`, но `#/` объявлен полем
// `imports` в package.json и потому разрешается и раннером тестов - без
// этого `participant.test.ts` не поднимается (см. тот же комментарий в
// `lib/auth.ts`)
import { m } from "#/paraglide/messages"
import { getLocale } from "#/paraglide/runtime"
import { api } from "#/lib/api"
import { serverLabel } from "#/lib/server-label"

import type { components } from "@tou/api-client"

export type ApplicationDto = components["schemas"]["ApplicationDto"]
export type ApplicationStatus = components["schemas"]["ApplicationStatusDto"]
export type ApplicantKind = components["schemas"]["ApplicantKindDto"]
export type ApplicationFile = components["schemas"]["ApplicationFileDto"]
export type JournalEntry = components["schemas"]["JournalEntryDto"]

/**
 * Чем заявка называется в списках участника: предмет, а не идентификатор.
 *
 * «Заявка 01a063dd» - это восемь шестнадцатеричных цифр, по которым нельзя
 * отличить одну заявку от другой, а в кабинете их было по несколько на разные
 * тендеры. Название тендера, номер лота и объект приезжают теперь в самой
 * `ApplicationDto`, и строка собирается здесь одна на все места вызова.
 *
 * Казахское название берется сохраненным (`tender_title_kk`), а не машинным
 * переводом: заголовок тендера - реквизит объявления, и подменять его нельзя.
 */
export function applicationSubject(
  application: Pick<
    ApplicationDto,
    "tender_title" | "tender_title_kk" | "lot_seq" | "object_name"
  >
): string {
  const title =
    getLocale() === "kk"
      ? application.tender_title_kk
      : application.tender_title
  return m.application_subject({
    title,
    lot: application.lot_seq,
    object: application.object_name,
  })
}

/** Мои заявки (FR-401/404): своя цена видна всегда (RLS INV-040). */
export const myApplicationsQuery = queryOptions({
  queryKey: ["applications", "my"],
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/applications/my")
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load applications")
    }
    return data
  },
})

/** Заявки тендера глазами секретаря/комиссии: цены до вскрытия - null. */
export const tenderApplicationsQuery = (tenderId: string) =>
  queryOptions({
    queryKey: ["applications", "tender", tenderId],
    queryFn: async () => {
      const { data, error } = await api.GET(
        "/api/v1/tenders/{id}/applications",
        {
          params: { path: { id: tenderId } },
        }
      )
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load applications")
      }
      return data
    },
  })

export type MeetingDto = components["schemas"]["MeetingDto"]
export type RejectionReason = components["schemas"]["RejectionReasonDto"]
/** Факт формирования протокола допуска (FR-503): номер и время. */
export type GeneratedProtocolDto = components["schemas"]["GeneratedProtocolDto"]
export type VoteValue = components["schemas"]["VoteValueDto"]

/** Заседание допуска (после вскрытия): состав комиссии для голосов (FR-503). */
export const meetingQuery = (tenderId: string) =>
  queryOptions({
    queryKey: ["meeting", tenderId],
    queryFn: async () => {
      const { data, response, error } = await api.GET(
        "/api/v1/tenders/{id}/meeting",
        { params: { path: { id: tenderId } } }
      )
      if (response.status === 404) return null
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load meeting")
      }
      return data
    },
  })

/** Закрытый перечень оснований отклонения (FR-502, п. 52). */
export const rejectionReasonsQuery = queryOptions({
  queryKey: ["rejection-reasons"],
  staleTime: 3_600_000,
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/refdata/rejection-reasons")
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load reasons")
    }
    return data
  },
})

/** Реквизиты протокола допуска; null - еще не сформирован. */
export const admissionProtocolQuery = (tenderId: string) =>
  queryOptions({
    queryKey: ["admission-protocol", tenderId],
    queryFn: async () => {
      const { data, response, error } = await api.GET(
        "/api/v1/tenders/{id}/admission-protocol",
        { params: { path: { id: tenderId } } }
      )
      if (response.status === 404) return null
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load protocol")
      }
      return data
    },
  })

/** Локализованная подпись основания отклонения (FR-502, п. 52). */
export function reasonLabel(
  reasons: RejectionReason[],
  code: string | null | undefined
): string | null {
  if (code == null) return null
  const reason = reasons.find((r) => r.code === code)
  if (reason === undefined) return code
  return serverLabel(reason)
}

/** Журнал регистрации (Прил. 12, FR-402). */
export const tenderJournalQuery = (tenderId: string) =>
  queryOptions({
    queryKey: ["journal", tenderId],
    queryFn: async () => {
      const { data, error } = await api.GET("/api/v1/tenders/{id}/journal", {
        params: { path: { id: tenderId } },
      })
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load journal")
      }
      return data
    },
  })

/**
 * Состояние обязательного пакета заявки (FR-401, п. 46).
 *
 * `package_complete` отвечает не «пять PDF на месте», а «требование пакета
 * выполнено»: у заявки с `package_required = false` (демо-данные, заявка
 * особого порядка) признак поднят при нуле вложений, и карточка уверяла
 * участника, что пакет собран, рядом со строкой «Файлы не приложены».
 * Самого `package_required` в контракте нет, поэтому «пакет не требуется»
 * выводится из пары признак+вложения - это ровно тот случай, который
 * отличим снаружи.
 */
export type PackageState = "complete" | "not_required" | "incomplete"

export function applicationPackageState(application: {
  package_complete: boolean
  files: readonly unknown[]
}): PackageState {
  if (!application.package_complete) return "incomplete"
  return application.files.length > 0 ? "complete" : "not_required"
}

export type ApplicationDocumentKind =
  components["schemas"]["ApplicationDocumentKindDto"]

/**
 * Обязательный пакет заявки (FR-401, п. 46): пять типов PDF.
 *
 * Перечень тот же, что и в `core.application_package_complete`, и порядок
 * тот же, в каком документы названы Правилами - это и есть порядок формы
 * загрузки. `legacy` в него не входит: этот тип носят вложения заявок,
 * поданных до появления пакета, и требованием он никогда не был.
 */
export const REQUIRED_DOCUMENT_KINDS = [
  "application_form",
  "registration_certificate",
  "tax_clearance",
  "guarantee_payment",
  "qualification_documents",
] as const satisfies readonly ApplicationDocumentKind[]

/**
 * Сколько позиций пакета закрыто - «n из 5» на карточке заявки.
 *
 * Считаются разные типы, а не файлы: сервер требует именно пять разных
 * (`count(DISTINCT document_kind) = 5`), и пять копий одной справки пакета
 * не собирают. До сих пор карточка знала только «собран / не собран», и
 * участник, приложивший три документа из пяти, не видел, сколько осталось.
 */
export function applicationPackageProgress(application: {
  files: readonly { document_kind: string }[]
}): { done: number; total: number } {
  const attached = new Set(application.files.map((file) => file.document_kind))
  return {
    done: REQUIRED_DOCUMENT_KINDS.filter((kind) => attached.has(kind)).length,
    total: REQUIRED_DOCUMENT_KINDS.length,
  }
}
