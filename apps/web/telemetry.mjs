/**
 * Очистка телеметрии от персональных данных (T71, NFR-07, NFR-16).
 *
 * Sentry и PostHog — внешние сервисы: то, что туда ушло, из системы уже
 * не вернуть. На бэкенде ПДн закрыты типом `Redacted<T>`, во фронте закрывать
 * их нечем — значит закрываем на выходе.
 *
 * Модуль намеренно на чистом ESM без зависимостей: его импортируют и
 * приложение (TypeScript, сборка Vite), и `instrument.server.mjs`, который
 * Node запускает через `--import` до старта приложения и потому TypeScript
 * прочитать не может. Одни правила на оба входа, а не две копии, которые
 * разойдутся.
 */

/** Метка вместо вычищенного значения — по ней ищется утечка в отчетах. */
export const REDACTED = "[скрыто]"

/**
 * Хлебные крошки, которые несут введенный или отображенный текст, а не
 * событие: клик и ввод Sentry подписывает текстом элемента (на экране админа
 * это имена и адреса почты, в реестре заявок — заявители), а `console`
 * повторяет все, что приложение вывело в лог.
 */
const DROPPED_BREADCRUMB_CATEGORIES = new Set([
  "console",
  "ui.click",
  "ui.input",
])

/** Ключи свойств, куда PostHog кладет разметку и текст автозахвата. */
const DROPPED_PROPERTY_KEYS = new Set([
  "$elements",
  "$elements_chain",
  "$el_text",
  "$selected_content",
])

/** Свойства-адреса: чистятся, а не удаляются — путь нужен для разбора. */
const URL_PROPERTY_KEYS = ["$current_url", "$referrer", "$initial_current_url"]

/**
 * Адрес без строки запроса и фрагмента.
 *
 * Фильтры реестров живут в URL (NFR-04: портал работает без JS), поэтому
 * в строку запроса попадает то, что набрал посетитель: `?q=Иванов` в поиске
 * по объявлениям или по объектам. Путь остается целиком — идентификаторы
 * записей нужны для разбора и персональными данными не являются.
 *
 * @param {unknown} raw
 * @returns {unknown} адрес без параметров; не-строка и неразбираемое значение
 *   возвращаются как есть — гадать о формате хуже, чем оставить
 */
export function scrubUrl(raw) {
  if (typeof raw !== "string" || raw === "") return raw

  const cut = raw.search(/[?#]/)
  return cut === -1 ? raw : `${raw.slice(0, cut)}?${REDACTED}`
}

/**
 * @typedef {Record<string, unknown> & {
 *   category?: string,
 *   data?: Record<string, unknown> | null,
 * }} Breadcrumb
 */

/**
 * Хлебная крошка Sentry: несущие текст — выбрасываются, остальным чистится
 * адрес.
 *
 * @param {Breadcrumb | null | undefined} crumb
 * @returns {Breadcrumb | null} `null` — крошку не отправлять
 */
export function scrubBreadcrumb(crumb) {
  if (crumb === null || crumb === undefined) return null
  if (
    crumb.category !== undefined &&
    DROPPED_BREADCRUMB_CATEGORIES.has(crumb.category)
  ) {
    return null
  }
  if (crumb.data === undefined || crumb.data === null) return crumb

  const data = { ...crumb.data }
  for (const key of ["url", "from", "to"]) {
    if (key in data) data[key] = scrubUrl(data[key])
  }
  return { ...crumb, data }
}

/**
 * Событие Sentry перед отправкой — последний рубеж поверх `dataCollection`.
 *
 * Сам SDK уже не собирает cookie, заголовки, тела и параметры запроса (см.
 * `instrument.server.mjs`), но адрес страницы приходит еще и из заголовка
 * запроса, из трассировки и из крошек. Здесь он чистится везде сразу.
 *
 * @param {Record<string, any> | null} event
 * @returns {Record<string, any> | null}
 */
export function scrubEvent(event) {
  if (event === null || event === undefined) return event

  const scrubbed = { ...event }

  if (scrubbed.request !== undefined && scrubbed.request !== null) {
    const {
      data: _body,
      cookies: _cookies,
      headers: _headers,
      ...request
    } = scrubbed.request
    scrubbed.request = { ...request, url: scrubUrl(request.url) }
  }

  // Пользователь опознается идентификатором; почта и адрес — уже ПДн
  if (scrubbed.user !== undefined && scrubbed.user !== null) {
    scrubbed.user =
      scrubbed.user.id === undefined ? undefined : { id: scrubbed.user.id }
  }

  if (Array.isArray(scrubbed.breadcrumbs)) {
    scrubbed.breadcrumbs = scrubbed.breadcrumbs
      .map(scrubBreadcrumb)
      .filter((crumb) => crumb !== null)
  }

  return scrubbed
}

/**
 * Свойства события PostHog.
 *
 * Автозахват выключен (см. провайдер), но правило стоит и здесь: включить
 * его обратно — одна строка в настройках, а разметка страницы несет и имена,
 * и адреса почты.
 *
 * @template {Record<string, unknown> | null | undefined} T
 * @param {T} properties
 * @returns {T}
 */
export function sanitizeProperties(properties) {
  if (properties === null || typeof properties !== "object") return properties

  const sanitized = { ...properties }
  for (const key of DROPPED_PROPERTY_KEYS) delete sanitized[key]
  for (const key of URL_PROPERTY_KEYS) {
    if (key in sanitized) sanitized[key] = scrubUrl(sanitized[key])
  }
  return sanitized
}
