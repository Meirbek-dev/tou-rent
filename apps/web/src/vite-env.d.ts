/// <reference types="vite-plus/client" />

// `strictImportMetaEnv` снимает индексную сигнатуру с `ImportMetaEnv`
// (см. vite/types/importMeta.d.ts): без нее `import.meta.env` - словарь
// `Record<string, any>`, где любое имя существует и имеет тип any. Отсюда
// то самое, на что жалуется env.ts: опечатка в имени не ломает сборку,
// а тихо выключает то, что от нее зависит, - так телеметрия и адрес сокета
// молча пропадали в проде. Со снятой сигнатурой необъявленное имя - ошибка
// компиляции, а не пустое значение в рантайме.
//
// Перечень обязан совпадать с `client` в src/env.ts: там valibot стережет
// значение в рантайме, здесь компилятор стережет имя. Читать переменные
// через точку - обязательно: в прод-сборке Vite подставляет значение
// статически именно по `import.meta.env.ИМЯ`, а обращение по ключу остается
// обращением к объекту, которого в бандле нет (см. hooks/use-auction-room.ts).
interface ViteTypeOptions {
  strictImportMetaEnv: unknown
}

interface ImportMetaEnv {
  /** Хост WS-комнаты торгов (FR-603); пусто - тот же origin */
  readonly VITE_API_WS_HOST?: string
  /** Пусто - Sentry не поднимается (см. instrument.server.mjs) */
  readonly VITE_SENTRY_DSN?: string
  /** Пусто - PostHog не поднимается (см. integrations/posthog) */
  readonly VITE_POSTHOG_KEY?: string
  readonly VITE_POSTHOG_HOST?: string
}
