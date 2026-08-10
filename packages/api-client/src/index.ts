/**
 * @tou/api-client - единственная точка доступа фронтенда к REST API (арх. § 7).
 *
 * Содержимое порождается цепочкой кодогенерации:
 *   Rust/utoipa → openapi.json → openapi-typescript → schema.d.ts (гейт G5).
 * Ручные DTO во фронте запрещены (гейт G7).
 */
import createClient from "openapi-fetch"
import createReactQuery from "openapi-react-query"

import type { paths } from "./schema"

export type { components, operations, paths } from "./schema"

const SAFE_METHODS = new Set(["GET", "HEAD", "OPTIONS"])

function readCookie(name: string): string | undefined {
  if (typeof document === "undefined") return undefined // SSR
  const prefix = `${name}=`
  return document.cookie
    .split("; ")
    .find((part) => part.startsWith(prefix))
    ?.slice(prefix.length)
}

/**
 * Типизированный fetch-клиент: cookie-сессии (credentials: include) и
 * автоматический заголовок CSRF double-submit на мутациях (арх. § 5).
 */
export function createApiClient(baseUrl: string) {
  const client = createClient<paths>({ baseUrl, credentials: "include" })
  client.use({
    onRequest({ request }) {
      if (!SAFE_METHODS.has(request.method)) {
        const token = readCookie("tou_csrf")
        if (token) request.headers.set("x-csrf-token", token)
      }
      return request
    },
  })
  return client
}

export type ApiClient = ReturnType<typeof createApiClient>

/** Хуки TanStack Query поверх клиента (openapi-react-query). */
export function createApiHooks(client: ApiClient) {
  return createReactQuery(client)
}
