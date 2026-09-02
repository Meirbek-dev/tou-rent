# web - портал TOU.Rent

TanStack Start (React 19, SSR через Nitro): публичный портал и кабинеты ролей.
Стек: TanStack Router/Query/Form/Table, Tailwind 4, shadcn/ui (base-nova, Base UI +
hugeicons) - компоненты инлайн в `src/components/ui`, Paraglide i18n (kk/ru/en).

## Запуск

Все команды - из корня монорепозитория (тулчейн Vite+, см. корневой README):

```bash
vp install        # зависимости workspace
vp run stack:up   # дев-стенд + api на :8080
vp run dev        # dev-сервер на :3000 (turbo → vp dev)
```

Скрипты этого пакета (`vp run <script>` внутри `apps/web`):

- `dev` - vite dev через `vp dev` c Sentry-инструментацией (`instrument.server.mjs`)
- `build` - `vp build`, выход в `.output/` (самодостаточный SSR-сервер).
  Следом `bun build` собирает `instrument.server.mjs` в `.output/server`
  одним файлом вместе с `@sentry/*` и `telemetry.mjs`: рантайм-образ везёт
  только `.output` без `node_modules`, и копия исходника там не резолвится
- `start` - запуск собранного сервера. Рантайм именно bun, а не node: сервер
  nitro/srvx собирается под тот рантайм, в котором шла сборка, и в node падает
  с «Bun is not defined». `--preload` поднимает Sentry до приложения - иначе
  инструментирование не встает на уже загруженные модули. Тем же способом
  сервер запускается в образе (`infra/docker/web.Dockerfile`):

  ```bash
  bun --preload ./.output/server/instrument.server.mjs .output/server/index.mjs
  ```

- `generate-routes` - регенерация `routeTree.gen.ts` (TanStack Router CLI)
- `i18n:compile` - компиляция Paraglide-сообщений в `src/paraglide/`

## Конвенции

- **i18n**: каждая строка UI - через ключ Paraglide в трех локалях
  (kk/en допустим draft-перевод, регламент А.5). Сообщения - в
  `project.inlang/messages`, локаль в URL-префиксе (`/kk`, `/en`; базовая - ru)
  через `rewrite` в `router.tsx` + `paraglideMiddleware` в `src/server.ts`.
- **env**: переменные типизируются в `src/env.ts` (T3Env + Valibot) и
  в `src/vite-env.d.ts` - перечни обязаны совпадать; значения - в
  `.env.local` (`VITE_SENTRY_DSN`, `VITE_POSTHOG_KEY`, `VITE_API_WS_HOST`).
  Объявляются только те переменные, которые код действительно читает, и
  читаются они через `env` из схемы: схема, которую не импортирует никто,
  ничего не проверяет. Адрес api переменной не задается - на SSR это
  `API_ORIGIN`, в браузере свой origin (`src/lib/api.ts`).
- **shadcn/ui**: компоненты добавляются генератором и живут в
  `src/components/ui` (не выносить в packages/):

  ```bash
  bunx shadcn@latest add button
  ```

- **API**: типизированный клиент - `@tou/api-client` (сгенерирован из
  OpenAPI-контракта, руками не править; регенерация - `vp run codegen`).

Сгенерированные файлы (`routeTree.gen.ts`, `src/paraglide/`) исключены из
форматирования - править только через генераторы.
