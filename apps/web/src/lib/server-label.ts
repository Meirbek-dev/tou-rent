import { getLocale } from "#/paraglide/runtime"

/**
 * Подпись, пришедшая с сервера в трех колонках (`*_ru` / `*_kk` / `*_en`).
 *
 * Справочники предметной области переводятся не Paraglide, а базой: перечни
 * категорий, оснований возврата, видов документов ведет админ, и их состав
 * меняется без пересборки фронта. Один помощник на все такие поля вместо
 * трех одинаковых `localeLabel` рядом: расхождение в правиле отката (kk/en
 * пусты - показываем ru, NFR-01) было бы незаметным до первого казахского
 * справочника.
 *
 * `base` - общая часть имени колонки: `label` (по умолчанию), `title`,
 * `kind_title` и т.п.
 */
export function serverLabel(source: object, base: string = "label"): string {
  const record = source as Record<string, unknown>
  const locale = getLocale()

  const fallback = text(record[`${base}_ru`])
  if (locale === "ru") return fallback

  return text(record[`${base}_${locale}`]) || fallback
}

/** Колонка может прийти `null` - это «перевода нет», а не «подпись пустая». */
function text(value: unknown): string {
  return typeof value === "string" ? value : ""
}
