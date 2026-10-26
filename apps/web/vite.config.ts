import { defineConfig, lazyPlugins } from "vite-plus"
import { devtools } from "@tanstack/devtools-vite"
import { paraglideVitePlugin } from "@inlang/paraglide-js"

import { tanstackStart } from "@tanstack/react-start/plugin/vite"

import viteReact, { reactCompilerPreset } from "@vitejs/plugin-react"
import babel from "@rolldown/plugin-babel"
import tailwindcss from "@tailwindcss/vite"
import { nitro } from "nitro/vite"

// Браузер ходит в api через свой origin; в проде ту же роль играет Caddy (арх. § 7).
// Прокси именно в nitro: dev-запросы обслуживает его обработчик, а не vite server.proxy.
const apiOrigin = process.env["API_ORIGIN"] ?? "http://localhost:8080"

// `lazyPlugins` возвращает undefined, когда конфиг загружают только ради
// не-плагинного блока: фабрика плагинов в этом случае намеренно не
// выполняется (см. withConfigMetadataResolution в vite-plus). При
// exactOptionalPropertyTypes «ключа нет» и «ключ есть, значение undefined» -
// разные типы, а `plugins` объявлен просто необязательным. Поэтому ключ
// не ставится вовсе - это ровно тот же смысл, что и пустой список, но без
// приведения типа.
const plugins = lazyPlugins(() => [
  devtools(),
  paraglideVitePlugin({
    project: "./project.inlang",
    outdir: "./src/paraglide",
    strategy: ["url", "baseLocale"],
  }),
  nitro({
    rollupConfig: { external: [/^@sentry\//] },
    routeRules: {
      "/api/**": {
        proxy: {
          to: `${apiOrigin}/api/**`,
          // Редирект отдаем браузеру, а не идем по нему сами: вход через
          // провайдера (FR-1502) уводит на его домен, которого dev-сервер
          // не знает - переход обязан выполнить браузер.
          fetchOptions: { redirect: "manual" },
        },
      },
    },
  }),
  tailwindcss(),
  tanstackStart(),
  viteReact(),
  babel({ presets: [reactCompilerPreset()] }),
])

const config = defineConfig({
  test: {
    // Vitest v4 compatibility: preserve mock call history.
    // Remove after tests no longer rely on calls from setup or earlier tests.
    // https://viteplus.dev/guide/vitest-v5#remove-unneeded-compatibility-settings
    // https://vitest.dev/guide/migration/#clearmocks-is-enabled-by-default
    clearMocks: false,
  },
  resolve: { tsconfigPaths: true },
  // TOU_DEV_WEB_PORT moves the dev server off :3000 where that port is taken;
  // it must match the value given to infra/compose/docker-compose.dev.yml.
  // Loopback and strictPort: a server that drifts to the next free port breaks
  // the OIDC redirects and WebSocket origins tied to this one. An explicit host
  // also lets it share the port with a listener on another address (Tailscale
  // Serve binds the tailnet IP), which the default "localhost" probe refuses.
  server: {
    host: "127.0.0.1",
    port: Number(process.env["TOU_DEV_WEB_PORT"] ?? 3000),
    strictPort: true,
  },
  ...(plugins ? { plugins } : {}),
})

export default config
