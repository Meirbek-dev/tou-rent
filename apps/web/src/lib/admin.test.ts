import { describe, expect, it } from "vite-plus/test"

import { matchesUser } from "./admin"

/**
 * Гаунтлет G-135: «Пользователи и роли» показывали пятьдесят записей из
 * девяноста шести без поиска и без признака усечения - заведенная сегодня
 * учетка была недостижима. Поиска на стороне контракта нет, поэтому отбор
 * идет по загруженным страницам, и правило отбора проверяется здесь.
 */

const USER = {
  full_name: "Асель Сериковна Нурланова",
  email: "finance@tou.demo",
}

describe("отбор учетной записи", () => {
  it("пустой запрос ничего не отсекает", () => {
    expect(matchesUser(USER, "")).toBe(true)
    expect(matchesUser(USER, "   ")).toBe(true)
  })

  it("ищет по имени без учета регистра", () => {
    expect(matchesUser(USER, "нурланова")).toBe(true)
    expect(matchesUser(USER, "АСЕЛЬ")).toBe(true)
  })

  it("ищет по почте - учетку помнят и по ней", () => {
    expect(matchesUser(USER, "finance@")).toBe(true)
    expect(matchesUser(USER, "tou.demo")).toBe(true)
  })

  it("чужая подстрока не проходит", () => {
    expect(matchesUser(USER, "организатор")).toBe(false)
  })
})
