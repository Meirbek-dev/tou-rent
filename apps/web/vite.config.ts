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

const config = defineConfig({
  resolve: { tsconfigPaths: true },
  plugins: lazyPlugins(() => [
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
  ]),
})

export default config
