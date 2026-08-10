import { queryOptions } from "@tanstack/react-query"

import { api } from "@/lib/api"

import type { components } from "@tou/api-client"

export type ContractDto = components["schemas"]["ContractDto"]
export type ChecklistItemDto = components["schemas"]["ChecklistItemDto"]
export type ActDto = components["schemas"]["ActDto"]

async function mutate<T>(promise: Promise<{ data?: T; error?: unknown }>) {
  const { data, error } = await promise
  if (error !== undefined || data === undefined) {
    throw error ?? new Error("request failed")
  }
  return data
}

/** Договоры тендера (FR-901): по одному на лот с победителем. */
export const tenderContractsQuery = (tenderId: string) =>
  queryOptions({
    queryKey: ["contracts", "tender", tenderId],
    queryFn: () =>
      mutate(
        api.GET("/api/v1/tenders/{id}/contracts", {
          params: { path: { id: tenderId } },
        })
      ),
  })

/**
 * Мои договоры (кабинет нанимателя, FR-902): своя сторона конвейера.
 *
 * TODO-ENGINEER: маршрут отдает страницу `{ items, next_after, truncated }`
 * (ТЗ § 7), сюда возвращаются только строки - экран нанимателя
 * (`routes/app/participant/contracts.tsx`) признак `truncated` пока
 * не показывает. Договоров у нанимателя единицы, поэтому потолок здесь
 * почти недостижим, но молчать о нем экран не должен.
 */
export const myContractsQuery = queryOptions({
  queryKey: ["contracts", "my"],
  queryFn: async () => (await mutate(api.GET("/api/v1/contracts/my"))).items,
})

/** Депозит по договору (FR-1003, п. 132–136): счет и остаток. */
export const contractDepositQuery = (contractId: string) =>
  queryOptions({
    queryKey: ["contracts", "deposit", contractId],
    queryFn: async () => {
      const { data, error, response } = await api.GET(
        "/api/v1/contracts/{id}/deposit",
        { params: { path: { id: contractId } } }
      )
      // Счета нет, пока договор не заключен - это не ошибка страницы
      if (response.status === 404) return null
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("failed to load deposit")
      }
      return data
    },
  })

/** Внесение депозита (п. 132): сумму проверяет сервер. */
export const payDeposit = (
  contractId: string,
  amount: string,
  paidAt: string,
  note?: string
) =>
  mutate(
    api.POST("/api/v1/contracts/{id}/deposit", {
      params: { path: { id: contractId } },
      body: { amount, paid_at: paidAt, note: note ?? null },
    })
  )

/** Возврат депозита после возврата объекта (п. 136). */
export const refundDeposit = (contractId: string, note?: string) =>
  mutate(
    api.POST("/api/v1/contracts/{id}/deposit/refund", {
      params: { path: { id: contractId } },
      body: { note: note ?? null },
    })
  )

/** Скан подписанного экземпляра (п. 111): загружает сторона договора. */
export const uploadContractScan = async (contractId: string, file: File) => {
  const body = new FormData()
  body.append("file", file)
  return mutate(
    api.POST("/api/v1/contracts/{id}/scan", {
      params: { path: { id: contractId } },
      // Контракт описывает multipart как строковое тело - отдаем FormData
      body: body as unknown as string,
      bodySerializer: (b: unknown) => b as FormData,
    })
  )
}

/** Чек-лист сверки документов договора (п. 113, INV-115). */
export const contractChecklistQuery = (contractId: string) =>
  queryOptions({
    queryKey: ["contracts", "checklist", contractId],
    queryFn: () =>
      mutate(
        api.GET("/api/v1/contracts/{id}/checklist", {
          params: { path: { id: contractId } },
        })
      ),
  })

/** Составление договора по итогам торгов лота (п. 108, 110). */
export const draftContract = (lotId: string) =>
  mutate(
    api.POST("/api/v1/lots/{id}/contract", { params: { path: { id: lotId } } })
  )

/** Шаг конвейера п. 110–115. */
export const advanceContract = (contractId: string, stage: string) =>
  mutate(
    api.POST("/api/v1/contracts/{id}/advance", {
      params: { path: { id: contractId } },
      body: { stage },
    })
  )

/** Отметка позиции сверки документов (п. 113). */
export const checkChecklistItem = (
  contractId: string,
  itemCode: string,
  checked: boolean
) =>
  mutate(
    api.POST("/api/v1/contracts/{id}/checklist", {
      params: { path: { id: contractId } },
      body: { item_code: itemCode, checked },
    })
  )

/** Регистрация договора в журнале (FR-905, п. 126). */
export const registerContract = (contractId: string, regNumber: string) =>
  mutate(
    api.POST("/api/v1/contracts/{id}/register", {
      params: { path: { id: contractId } },
      body: { reg_number: regNumber },
    })
  )

/** Акты договора (FR-904, Прил. 7–8). */
export const contractActsQuery = (contractId: string) =>
  queryOptions({
    queryKey: ["contracts", "acts", contractId],
    queryFn: () =>
      mutate(
        api.GET("/api/v1/contracts/{id}/acts", {
          params: { path: { id: contractId } },
        })
      ),
  })

/** Составление акта: передача включает начисление платы, возврат закрывает договор. */
export const createAct = (
  contractId: string,
  kind: string,
  actDate: string,
  note?: string
) =>
  mutate(
    api.POST("/api/v1/contracts/{id}/acts", {
      params: { path: { id: contractId } },
      body: { kind, act_date: actDate, note: note ?? null },
    })
  )
