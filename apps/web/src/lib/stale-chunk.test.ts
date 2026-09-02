import { describe, expect, it } from "vite-plus/test"

import { isStaleChunkError } from "./stale-chunk"

/**
 * Распознавание рассинхрона манифеста (NFR-15).
 *
 * Вкладка, открытая до выката, после него падает на импорте исчезнувшего
 * чанка: SSR отдает целую страницу, а гидратация заменяет `<main>` экраном
 * отказа, где «Повторить» вызывает `reset()` - тот же импорт того же
 * несуществующего файла. Экран не менялся никогда; лечит только полная
 * перезагрузка, а выбрать ее можно лишь по тексту исключения - типа у него
 * нет ни в одном браузере. Отсюда и тест: список текстов - единственное,
 * что отделяет починку от тупика.
 */

/** Настоящие сообщения браузеров об отказе динамического импорта. */
const STALE = [
  "Failed to fetch dynamically imported module: https://tou.example/_build/assets/login-CuWz0A5a.js",
  "error loading dynamically imported module: https://tou.example/_build/assets/login-CuWz0A5a.js",
  "Importing a module script failed.",
]

describe("рассинхрон манифеста", () => {
  it("узнается по сообщению любого из браузеров", () => {
    for (const message of STALE) {
      expect([message, isStaleChunkError(new Error(message))]).toEqual([
        message,
        true,
      ])
    }
  })

  it("обычный отказ за него не принимается - `reset()` там работает", () => {
    const ordinary = [
      new Error("failed to load tenders"),
      new TypeError("Failed to fetch"),
      new Error("Cannot read properties of undefined (reading 'id')"),
    ]
    for (const error of ordinary) {
      expect([error.message, isStaleChunkError(error)]).toEqual([
        error.message,
        false,
      ])
    }
  })

  it("не падает на том, что Error не является", () => {
    for (const thrown of [null, undefined, "boom", 42, {}, { message: 7 }]) {
      expect(isStaleChunkError(thrown)).toBe(false)
    }
  })
})
