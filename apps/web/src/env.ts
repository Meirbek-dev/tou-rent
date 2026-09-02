import { createEnv } from "@t3-oss/env-core"
import * as v from "valibot"

export const env = createEnv({
  server: {
    SERVER_URL: v.optional(v.pipe(v.string(), v.url())),
  },

  /**
   * The prefix that client-side variables must have. This is enforced both at
   * a type-level and at runtime.
   */
  clientPrefix: "VITE_",

  /**
   * Здесь объявлены все `VITE_*`, которые читает приложение. Необъявленная
   * переменная не проверяется вовсе: опечатка в имени не ломает сборку,
   * а тихо выключает то, что от нее зависит, - так телеметрия и адрес
   * сокета молча пропадали в проде.
   */
  client: {
    // `VITE_APP_TITLE` и `VITE_API_URL` отсюда убраны: их не читает никто.
    // Заголовок задают `head` маршрутов, адрес api - `API_ORIGIN` на SSR
    // и свой origin в браузере (`lib/api.ts`). Объявленная, но не читаемая
    // переменная хуже необъявленной: она создает уверенность, что значение
    // куда-то доедет.
    /** Хост WS-комнаты торгов (FR-603); пусто - тот же origin */
    VITE_API_WS_HOST: v.optional(v.pipe(v.string(), v.minLength(1))),
    /** Пусто - Sentry не поднимается (см. instrument.server.mjs) */
    VITE_SENTRY_DSN: v.optional(v.pipe(v.string(), v.url())),
    /** Пусто - PostHog не поднимается (см. integrations/posthog) */
    VITE_POSTHOG_KEY: v.optional(v.pipe(v.string(), v.minLength(1))),
    VITE_POSTHOG_HOST: v.optional(v.pipe(v.string(), v.url())),
  },

  /**
   * What object holds the environment variables at runtime. This is usually
   * `process.env` or `import.meta.env`.
   */
  // Разворот, а не сама `import.meta.env`: со снятой индексной сигнатурой
  // (`strictImportMetaEnv`, см. vite-env.d.ts) интерфейс перестал подходить
  // под `Record<string, ...>`, которого ждет createEnv. Содержимое то же -
  // в сборке Vite подставляет сюда объект целиком.
  runtimeEnv: { ...import.meta.env },

  /**
   * Treat empty strings as undefined so defaults and optionality behave
   * predictably for values like `PORT=` in a ".env" file.
   */
  emptyStringAsUndefined: true,
})
