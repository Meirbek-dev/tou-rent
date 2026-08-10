import { createFileRoute } from "@tanstack/react-router"

// Проба живости SSR-сервера для оркестратора (infra/compose/docker-compose.prod.yml).
// Отдельный маршрут, а не главная: страница портала на каждой пробе рендерилась бы
// целиком и ходила за данными в api, и недоступность api уводила бы `web`
// в unhealthy следом - Caddy перестал бы отдавать и то, что еще работает.
// Здесь проверяется ровно одно: процесс поднялся и слушает порт.
export const Route = createFileRoute("/healthz")({
  server: {
    handlers: {
      GET: () =>
        new Response("ok\n", {
          headers: {
            "content-type": "text/plain; charset=utf-8",
            "cache-control": "no-store",
          },
        }),
    },
  },
})
