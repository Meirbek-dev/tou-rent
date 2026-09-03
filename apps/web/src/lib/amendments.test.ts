import { describe, expect, it } from "vite-plus/test"

import { unwrap } from "./amendments"

/**
 * Гаунтлет G-098: успешная отмена тендера и лота показывалась как отказ.
 *
 * `POST /tenders/{id}/cancel` и `/lots/{id}/cancel` отвечают 200 без тела -
 * openapi-fetch отдает `{ data: undefined }`. Хелпер считал такой ответ
 * провалом, форма выводила «Не удалось выполнить операцию», карточка не
 * обновлялась, а в базе отмена уже прошла. Признак успеха ровно один -
 * отсутствие `error`.
 */
describe("разбор ответа api", () => {
  it("пустое тело при 200 - успех, а не отказ", async () => {
    await expect(unwrap(Promise.resolve({}))).resolves.toBeUndefined()
  })

  it("тело ответа возвращается как есть", async () => {
    await expect(
      unwrap(Promise.resolve({ data: { id: "42" } }))
    ).resolves.toEqual({ id: "42" })
  })

  it("отказ бросает problem+json, а не подменяет его своей ошибкой", async () => {
    const problem = { code: "rule_violation", rule: "tender_cancellation" }
    await expect(unwrap(Promise.resolve({ error: problem }))).rejects.toEqual(
      problem
    )
  })

  it("пустое тело вместе с отказом остается отказом", async () => {
    await expect(
      unwrap(Promise.resolve({ data: undefined, error: { code: "forbidden" } }))
    ).rejects.toEqual({ code: "forbidden" })
  })
})
