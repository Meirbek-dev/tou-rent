import { AxeBuilder } from "@axe-core/playwright"
import { expect, test } from "@playwright/test"

import type { AxeResults, Result } from "axe-core"
import type { Page } from "@playwright/test"

/**
 * Гейт G17: доступность публичного контура (NFR-04, NFR-15).
 *
 * ТЗ v1 обещает WCAG 2.1 AA на публичных страницах, но проверялось это один
 * раз руками (Lighthouse на карточке тендера). Разовый замер стареет с первой
 * же правкой разметки, поэтому здесь — гейт: 0 нарушений уровней A и AA
 * на каждом публичном маршруте.
 *
 * Проверяются обе темы. Самое частое нарушение AA — контраст, а он у светлой
 * и темной темы разный: пройти в одной и упасть в другой — обычное дело,
 * и без второго прохода половина интерфейса осталась бы непроверенной.
 *
 * Отдельно проверяется работа без JS (NFR-04): публичный портал обязан
 * оставаться пригодным, а фильтры реестров — уходить нативной GET-формой.
 */

/** Уровни, обещанные NFR-04: WCAG 2.0/2.1, A и AA. */
const WCAG_AA = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"]

/** Публичные маршруты (ТЗ § 8): гость видит их без авторизации. */
const PUBLIC_ROUTES = [
  { path: "/", name: "главная" },
  { path: "/tenders", name: "реестр объявлений" },
  { path: "/objects", name: "свободные площади" },
  { path: "/land-plots", name: "земельные участки" },
  { path: "/special-orders", name: "особый порядок" },
  { path: "/how-to", name: "как участвовать" },
  { path: "/auth/login", name: "вход" },
  { path: "/auth/register", name: "регистрация" },
] as const

/**
 * Нарушения одним читаемым списком: без этого падение гейта — простыня JSON,
 * по которой непонятно, что чинить.
 */
function describe(results: AxeResults): string {
  return results.violations
    .map((violation: Result) => {
      const where = violation.nodes
        .slice(0, 3)
        .map((node) => node.target.join(" "))
        .join("; ")
      return `${violation.id} (${violation.impact}): ${violation.help}\n    ${where}`
    })
    .join("\n  ")
}

async function scan(page: Page): Promise<AxeResults> {
  return await new AxeBuilder({ page }).withTags(WCAG_AA).analyze()
}

for (const theme of ["light", "dark"] as const) {
  for (const route of PUBLIC_ROUTES) {
    test(`доступность (${theme}): ${route.name}`, async ({ page }) => {
      // Тему приложение поднимает из localStorage до первого кадра
      // (`themeInitializer` в разметке), поэтому ключ ставится до перехода:
      // иначе axe померил бы не ту палитру. Функция выполняется в странице,
      // а раннер типизирован без DOM (tsconfig `lib: ESNext`) — отсюда
      // явное объявление вместо lib.dom
      await page.addInitScript((value: string) => {
        ;(
          globalThis as unknown as {
            localStorage: { setItem(key: string, value: string): void }
          }
        ).localStorage.setItem("tou-rent-theme", value)
      }, theme)

      await page.goto(route.path)
      await expect(page.locator("h1")).toBeVisible()

      const results = await scan(page)
      expect(
        results.violations.length,
        `${route.path} (${theme}):\n  ${describe(results)}`
      ).toBe(0)
    })
  }
}

/**
 * Карточка тендера — самая насыщенная публичная страница (таблица лотов,
 * сроки, ссылки на PDF), поэтому проверяется на настоящем объявлении,
 * а не на пустой заготовке.
 */
test("доступность: карточка опубликованного тендера", async ({
  page,
  request,
}) => {
  const response = await request.get("/api/v1/tenders?limit=1")
  expect(response.status(), "реестр тендеров доступен гостю").toBe(200)
  const page1 = (await response.json()) as { items: Array<{ id: string }> }

  // Пропуск условный, а не глушитель: проверять карточку не на чем, пока
  // на стенде нет ни одного опубликованного тендера. Прогон с пустым
  // реестром - это отсутствие предмета проверки, а не зеленый результат.
  // Условие и причина вынесены, чтобы вызов уместился в строку вместе
  // с токеном: гейт G2 ищет токен именно на строке с `.skip(`
  const nothingPublished = page1.items.length === 0
  const reason = "на стенде нет опубликованных тендеров — нечего проверять"
  test.skip(nothingPublished, reason) // ALLOWED-BY-ENGINEER:T60 предусловие

  await page.goto(`/tenders/${page1.items[0]?.id}`)
  await expect(page.locator("h1")).toBeVisible()

  const results = await scan(page)
  expect(
    results.violations.length,
    `карточка тендера:\n  ${describe(results)}`
  ).toBe(0)
})

/**
 * NFR-04: публичный портал работает без JS. Проверяется не факт отрисовки,
 * а то, ради чего это требование существует, — что реестром можно
 * пользоваться: фильтр уходит нативной GET-формой и меняет выдачу.
 */
test.describe("без JavaScript", () => {
  test.use({ javaScriptEnabled: false })

  test("реестр объявлений фильтруется нативной формой", async ({ page }) => {
    await page.goto("/tenders")
    await expect(page.locator("h1")).toBeVisible()

    const form = page.locator("form").first()
    await expect(
      form,
      "фильтры — обычная форма, а не обработчик на JS"
    ).toHaveAttribute("method", /get/i)

    await form.locator("#filter-status").selectOption("accepting")
    await form.getByRole("button", { name: /Применить|Қолдану|Apply/ }).click()

    await expect(page).toHaveURL(/status=accepting/)
    await expect(page.locator("h1")).toBeVisible()
  })

  test("карточка объекта и статичные страницы читаются", async ({ page }) => {
    for (const path of ["/", "/objects", "/how-to"]) {
      await page.goto(path)
      await expect(page.locator("h1"), `${path} без JS`).toBeVisible()
    }
  })
})
