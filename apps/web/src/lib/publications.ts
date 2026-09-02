import { queryOptions } from "@tanstack/react-query"

import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type ProtocolDto = components["schemas"]["ProtocolDto"]
export type DossierItemDto = components["schemas"]["DossierItemDto"]

async function mutate<T>(promise: Promise<{ data?: T; error?: unknown }>) {
  const { data, error } = await promise
  if (error !== undefined || data === undefined) {
    throw error ?? new Error("request failed")
  }
  return data
}

/** Протоколы тендера: публичные - всем, остальные - участникам и комиссии. */
export const tenderProtocolsQuery = (tenderId: string) =>
  queryOptions({
    queryKey: ["protocols", "tender", tenderId],
    queryFn: () =>
      mutate(
        api.GET("/api/v1/tenders/{id}/protocols", {
          params: { path: { id: tenderId } },
        })
      ),
  })

/**
 * Копии протоколов в кабинете участника (FR-703, п. 56).
 *
 * Отдается страница целиком: протокол - доказательство хода тендера, и
 * недосчитаться его копии участник должен не молча.
 */
export const myProtocolsQuery = queryOptions({
  queryKey: ["protocols", "my"],
  queryFn: () => mutate(api.GET("/api/v1/protocols/my")),
})

/**
 * Предмет досье (FR-1602, FR-1206): тендер либо решение особого порядка.
 * Механизм сборки у них общий, различаются срок хранения (INV-042) и права.
 */
export type DossierSubject = {
  kind: "tender" | "special-request"
  id: string
}

/** Состав досье: тендера (FR-1602, п. 16) или решения (FR-1206, п. 97). */
export const dossierQuery = (subject: DossierSubject) =>
  queryOptions({
    queryKey: ["dossier", subject.kind, subject.id],
    queryFn: () =>
      subject.kind === "tender"
        ? mutate(
            api.GET("/api/v1/tenders/{id}/dossier", {
              params: { path: { id: subject.id } },
            })
          )
        : mutate(
            api.GET("/api/v1/special-requests/{id}/dossier", {
              params: { path: { id: subject.id } },
            })
          ),
  })

/** Публикация протокола: публичный доступ на шесть месяцев (FR-702, п. 75). */
export const publishProtocol = (protocolId: string) =>
  mutate(
    api.POST("/api/v1/protocols/{id}/publish", {
      params: { path: { id: protocolId } },
    })
  )
