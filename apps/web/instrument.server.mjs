import * as Sentry from "@sentry/tanstackstart-react"

import { scrubBreadcrumb, scrubEvent } from "./telemetry.mjs"

const sentryDsn =
  import.meta.env?.VITE_SENTRY_DSN ?? process.env.VITE_SENTRY_DSN

if (!sentryDsn) {
  console.warn("VITE_SENTRY_DSN is not defined. Sentry is not running.")
} else {
  Sentry.init({
    dsn: sentryDsn,
    // NFR-07: во внешний сервис не уходят персональные данные. Умолчания SDK
    // собирают cookie, заголовки и параметры запроса — для SSR это сессионная
    // cookie и строка поиска по реестрам, где посетитель набирает имена
    // и адреса. Каждая категория выключена явно: `sendDefaultPii` объявлен
    // устаревшим и в v11 исчезнет.
    dataCollection: {
      userInfo: false,
      cookies: false,
      httpHeaders: { request: false, response: false },
      httpBodies: [],
      urlQueryParams: false,
    },
    // Второй рубеж поверх `dataCollection`: адрес страницы приходит еще
    // и трассировкой, и хлебными крошками (T71)
    beforeBreadcrumb: scrubBreadcrumb,
    beforeSend: scrubEvent,
    tracesSampleRate: 0.1,
  })
}
