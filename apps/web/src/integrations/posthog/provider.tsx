import posthog from "posthog-js"
import { PostHogProvider as BasePostHogProvider } from "@posthog/react"

import { sanitizeProperties } from "../../../telemetry.mjs"
import { env } from "@/env"

import type { ReactNode } from "react"

// NFR-07, T71: во внешнюю аналитику уходят только идентификаторы и имена
// действий. Автозахват выключен не из экономии событий, а потому что он
// подписывает клик текстом элемента: на экране админа это имена и адреса
// почты пользователей, в реестре заявок — заявители. По той же причине
// выключена запись сессии, а свойства проходят через общую очистку
// (см. telemetry.mjs) — она же чистит строку запроса из адреса страницы,
// куда фильтры реестров кладут поисковый ввод.
// Переменные читаются через схему `src/env.ts`, а не напрямую из
// `import.meta.env`: до сих пор схему не импортировал никто, и объявление
// в ней ничего не проверяло - ровно та беда, которую она заводилась лечить.
// Модуль подключен из `__root.tsx`, то есть схема исполняется при старте
// и в браузере, и на SSR.
if (typeof window !== "undefined" && env.VITE_POSTHOG_KEY) {
  posthog.init(env.VITE_POSTHOG_KEY, {
    api_host: env.VITE_POSTHOG_HOST ?? "https://us.i.posthog.com",
    person_profiles: "identified_only",
    capture_pageview: false,
    autocapture: false,
    disable_session_recording: true,
    // Если запись сессии когда-нибудь включат в настройках проекта, она
    // включится без правки кода — маскировка должна быть заранее
    session_recording: { maskAllInputs: true, maskTextSelector: "*" },
    sanitize_properties: sanitizeProperties,
    defaults: "2025-11-30",
  })
}

interface PostHogProviderProps {
  children: ReactNode
}

export default function PostHogProvider({ children }: PostHogProviderProps) {
  return <BasePostHogProvider client={posthog}>{children}</BasePostHogProvider>
}
