/**
 * Рассинхрон манифеста: вкладка открыта до выката, ассеты обновились.
 *
 * SSR при этом отдает целую страницу, а гидратация падает на импорте чанка,
 * которого на диске больше нет. Штатный `reset()` роутера повторяет тот же
 * импорт того же исчезнувшего файла - экран отказа не менялся никогда.
 * Отличить этот отказ от всех прочих можно только по тексту исключения:
 * типа у него нет ни в одном браузере.
 */

/** Тексты браузеров об отказе динамического импорта. */
const STALE_CHUNK_MARKERS = [
  // Chromium: "Failed to fetch dynamically imported module: https://..."
  "dynamically imported module",
  // Safari: "Importing a module script failed."
  "Importing a module script failed",
  // Firefox: "error loading dynamically imported module"
  "error loading dynamically imported module",
]

/** Отметка о попытке перезагрузки; живет ровно во вкладке. */
const RELOAD_MARK = "tou.stale-chunk-reload"
/** Окно, внутри которого вторая перезагрузка была бы петлей. */
const RELOAD_WINDOW_MS = 30_000

export function isStaleChunkError(error: unknown): boolean {
  // Тип роутера обещает Error, но сюда доезжает все, что бросили: у строки
  // или простого объекта `message` нет, и `.includes` уронил бы сам экран
  const message: unknown = (error as { message?: unknown } | null | undefined)
    ?.message
  return (
    typeof message === "string" &&
    STALE_CHUNK_MARKERS.some((marker) => message.includes(marker))
  )
}

/** Перезагружались ли только что: защита от петли перезагрузок. */
export function reloadTriedRecently(): boolean {
  try {
    const at = Number(window.sessionStorage.getItem(RELOAD_MARK))
    return at > 0 && Date.now() - at < RELOAD_WINDOW_MS
  } catch {
    // Хранилище запрещено настройками браузера: защиты от петли нет,
    // но одна лишняя перезагрузка лучше тупика.
    return false
  }
}

export function markReloadTried(): void {
  try {
    window.sessionStorage.setItem(RELOAD_MARK, String(Date.now()))
  } catch {
    // См. reloadTriedRecently: отсутствие хранилища не повод не чинить экран.
  }
}
