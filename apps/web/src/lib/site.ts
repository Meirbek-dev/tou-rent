/**
 * Публичный адрес портала - для разметки, которую читают роботы.
 *
 * Краулер соцсети или мессенджера не исполняет JS и не разворачивает
 * относительный путь: `og:image` и `og:url` он обязан получить абсолютными,
 * иначе карточка ссылки остается без обложки и без адреса.
 *
 * Домен задан один раз в `infra/compose/.env` (`TOU_DOMAIN`, тот же в
 * Caddyfile), но в сборку web не попадает: сервису `web` в
 * `docker-compose.prod.yml` такой переменной не передают, а объявлять
 * `VITE_*`, которую никто не читает, здесь запрещено (см. `src/env.ts`).
 * Поэтому origin - константа, и она обязана совпадать с `TOU_DOMAIN`:
 * переезд домена правится в двух местах.
 */
export const SITE_ORIGIN = "https://rent.tou.edu.kz"

/** Абсолютный адрес по корневому пути: `/tenders` -> `<origin>/tenders`. */
export function absoluteUrl(path: string): string {
  return `${SITE_ORIGIN}${path.startsWith("/") ? path : `/${path}`}`
}
