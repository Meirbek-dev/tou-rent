import { queryOptions } from "@tanstack/react-query"
import { redirect } from "@tanstack/react-router"
import { createApiClient } from "@tou/api-client"
import { deLocalizeUrl, getLocale } from "#/paraglide/runtime"

import type { components } from "@tou/api-client"

export type TenderDto = components["schemas"]["TenderDto"]
export type LotDto = components["schemas"]["LotDto"]
export type TenderStatus = components["schemas"]["TenderStatusDto"]
export type ObjectDto = components["schemas"]["ObjectDto"]
export type ObjectStatus = components["schemas"]["ObjectStatusDto"]
export type ObjectKind = components["schemas"]["ObjectKindDto"]
export type SiteAnnouncementDto = components["schemas"]["SiteAnnouncementDto"]

// SSR-загрузчики ходят в api напрямую (в проде - тот же хост за Caddy),
// браузер - на свой origin: в dev /api проксирует vite (vite.config.ts),
// в проде - Caddy (арх. § 7).
const baseUrl = import.meta.env.SSR
  ? (process.env["API_ORIGIN"] ?? "http://localhost:8080")
  : ""

export const api = createApiClient(baseUrl)

/** Свои маршруты аутентификации: там 401 - штатный ответ, а не конец сессии. */
const AUTH_ROUTE_PREFIX = "/api/v1/auth/"

/**
 * Текущий адрес кабинета без префикса локали; `undefined` - мы не в кабинете.
 *
 * Возврат нужен роутеру, а он живет на де-локализованных путях (`rewrite`
 * в `router.tsx`): `/kk/app/participant` и `/app/participant` - один и тот же
 * маршрут, и в параметр должен попасть второй.
 */
function currentCabinetPath(): string | undefined {
  if (typeof window === "undefined") return undefined
  const url = deLocalizeUrl(window.location.href)
  if (!url.pathname.startsWith("/app")) return undefined
  return `${url.pathname}${url.search}`
}

/**
 * Истекшая сессия внутри кабинета - на страницу входа, а не на экран сбоя
 * (FR-1501).
 *
 * `beforeLoad` маршрута `/app` спрашивает про сессию один раз - на вход
 * в поддерево. Сессия, кончившаяся после этого, доезжала до пользователя
 * отказом дочернего загрузчика: общий экран «Что-то пошло не так» и кнопка
 * «Повторить», которая повторяет тот же отказ. Перехват стоит на самом
 * клиенте api, потому что ниже некуда: каждый из полусотни загрузчиков иначе
 * обязан различать 401 сам, а большинство из них заворачивает любой отказ
 * в `new Error("failed to load ...")`, где от кода ответа не остается ничего.
 *
 * Бросок, а не возврат: `onResponse` понимает возвращенный `Response` как
 * подмену ответа, а `redirect()` - это как раз `Response`.
 */
api.use({
  onResponse({ request, response }) {
    if (response.status !== 401) return undefined
    if (new URL(request.url).pathname.startsWith(AUTH_ROUTE_PREFIX)) {
      return undefined
    }
    const from = currentCabinetPath()
    if (from === undefined) return undefined
    throw redirect({
      to: "/auth/login",
      search: { redirect: from },
      replace: true,
    })
  },
})

/** Выбирает сохраненную локализованную строку без машинного перевода. */
export function localizedTenderTitle(
  tender: Pick<TenderDto, "title" | "title_kk">
): string {
  return getLocale() === "kk" ? tender.title_kk : tender.title
}

/** Объявление на главной: 404 означает, что администратор его еще не опубликовал. */
export const siteAnnouncementQuery = queryOptions({
  queryKey: ["site-announcement"],
  queryFn: async () => {
    const { data, error, response } = await api.GET("/api/v1/site-announcement")
    if (response.status === 404) return null
    if (error !== undefined || data === undefined) {
      throw (error as unknown) ?? new Error("failed to load site announcement")
    }
    return data
  },
})

/**
 * Способы входа (FR-1502): кнопка внешнего провайдера появляется, только если
 * он настроен на стенде. Запрос идет в SSR-загрузчике - страница входа
 * работает без JS (NFR-04).
 */
export const authProvidersQuery = queryOptions({
  queryKey: ["auth", "providers"],
  queryFn: async () => {
    const { data, error } = await api.GET("/api/v1/auth/providers")
    if (error !== undefined || data === undefined) {
      throw new Error("failed to load auth providers")
    }
    return data
  },
})

/** Страница публичного реестра (FR-1401): гостю API отдает только опубликованные. */
export const tendersPageQuery = (after?: string) =>
  queryOptions({
    queryKey: ["tenders", after ?? null],
    queryFn: async () => {
      const { data, error } = await api.GET("/api/v1/tenders", {
        params: { query: after ? { after } : {} },
      })
      if (error !== undefined || data === undefined) {
        throw new Error("failed to load tenders")
      }
      return data
    },
  })

/**
 * Витрина свободных площадей (FR-102): фильтрация и пагинация - на сервере,
 * страница отдается SSR без авторизации.
 */
export const objectsPageQuery = (params: {
  status?: string | undefined
  kind?: string | undefined
  q?: string | undefined
  area_min?: number | undefined
  area_max?: number | undefined
  after?: string | undefined
}) =>
  queryOptions({
    queryKey: ["objects", params],
    queryFn: async () => {
      const { data, error } = await api.GET("/api/v1/objects", {
        params: {
          query: Object.fromEntries(
            Object.entries(params)
              .filter(([, value]) => value !== undefined)
              .map(([key, value]) => [key, String(value)])
          ),
        },
      })
      if (error !== undefined || data === undefined) {
        throw new Error("failed to load objects")
      }
      return data
    },
  })

/** Объект, к которому привязан лот: название, адрес и площадь. */
export const objectQuery = (id: string) =>
  queryOptions({
    queryKey: ["object", id],
    queryFn: async () => {
      const { data, error, response } = await api.GET("/api/v1/objects/{id}", {
        params: { path: { id } },
      })
      if (response.status === 404) return null
      if (error !== undefined || data === undefined) {
        throw new Error("failed to load object")
      }
      return data
    },
  })

/** Карточка тендера; null - не найден или не опубликован (404 конвертируем в notFound на уровне роута). */
export const tenderQuery = (id: string) =>
  queryOptions({
    queryKey: ["tender", id],
    queryFn: async () => {
      const { data, error, response } = await api.GET("/api/v1/tenders/{id}", {
        params: { path: { id } },
      })
      // 404 - нет/не опубликован; 400/422 - id не UUID: для гостя это одно и то же
      if ([400, 404, 422].includes(response.status)) return null
      if (error !== undefined || data === undefined) {
        throw new Error("failed to load tender")
      }
      return data
    },
  })

export const tenderDocumentsQuery = (id: string) =>
  queryOptions({
    queryKey: ["tender-documents", id],
    queryFn: async () => {
      const { data, error } = await api.GET("/api/v1/tenders/{id}/documents", {
        params: { path: { id } },
      })
      if (error !== undefined || data === undefined) {
        throw new Error("failed to load tender documents")
      }
      return data
    },
  })
