# Образ web (NFR-10): сборка бандла TanStack Start/nitro в bun, запуск - тоже
# в bun (см. пояснение у рантайм-стадии: сервер собирается под тот рантайм,
# в котором шла сборка).
# Браузер ходит в api через свой origin - маршрут /api/** отдает Caddy.
FROM oven/bun:1.4.0 AS build
WORKDIR /w

COPY package.json bun.lock bunfig.toml turbo.json vite.config.ts ./
COPY apps/web/package.json apps/web/package.json
COPY packages/api-client/package.json packages/api-client/package.json
# Манифест e2e бандлу не нужен, но рабочие области заданы шаблоном `apps/*`:
# без него bun видит другой их набор, чем записан в bun.lock, и
# `--frozen-lockfile` отбивает установку. Образ web с этого не собирался вовсе
COPY apps/e2e/package.json apps/e2e/package.json
# --ignore-scripts: корневой prepare (`vp config`) настраивает git-хуки
# разработчику и в образе без .git не нужен; бинарники esbuild/lightningcss
# приезжают платформенными пакетами, а не postinstall-скриптами
RUN bun install --frozen-lockfile --ignore-scripts

COPY . .
# SSR-загрузчики ходят в api по внутреннему имени сервиса compose
ENV API_ORIGIN=http://api:8080
RUN bun run --cwd apps/web build

# Рантайм - bun: сервер nitro/srvx собирается под тот рантайм, в котором шла
# сборка, и в node падает с «Bun is not defined»
FROM oven/bun:1.4.0
WORKDIR /app
COPY --from=build /w/apps/web/.output ./.output

USER bun

ENV NODE_ENV=production PORT=3000 API_ORIGIN=http://api:8080
EXPOSE 3000
# `--preload`: Sentry поднимается до приложения, иначе инструментирование
# не встанет на уже загруженные модули. Без этого файл лежал в образе
# мертвым грузом, а весь разбор ПДн в нем (T71, NFR-07) ничего не защищал -
# сам Sentry в проде не запускался. Файл приезжает из сборки уже бандлом
# (см. скрипт `build` в apps/web/package.json): образ везет только
# `.output`, и импорт `@sentry/*` из копии исходника здесь не резолвится
CMD ["bun", "--preload", "./.output/server/instrument.server.mjs", ".output/server/index.mjs"]
