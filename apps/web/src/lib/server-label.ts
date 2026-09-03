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
 *
 * Русская колонка приходит двумя способами: у справочников это `<base>_ru`,
 * а у DTO, где подпись была одна и переводы добавили позже (участок -
 * `designation_label`, категория особого порядка - `category_label`,
 * заголовок публикации - `title`), русская подпись осталась в самом
 * `<base>`. Правило отката одно на оба случая - иначе на каждый такой DTO
 * заводился бы свой `localeLabel` мимо `serverLabel` (G-075, G-066).
 */
export function serverLabel(source: object, base: string = "label"): string {
  const record = source as Record<string, unknown>
  const locale = getLocale()

  const fallback = text(record[`${base}_ru`]) || text(record[base])
  if (locale === "ru") return fallback

  return text(record[`${base}_${locale}`]) || fallback
}

/** Колонка может прийти `null` - это «перевода нет», а не «подпись пустая». */
function text(value: unknown): string {
  return typeof value === "string" ? value : ""
}
