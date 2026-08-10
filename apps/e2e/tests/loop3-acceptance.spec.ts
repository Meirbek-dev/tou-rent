import { expect, test } from "@playwright/test"

import { accounts, loginAsUi, provisionSpecialSite } from "./helpers"

/**
 * Приемка контура 3 (ТЗ § 10в, T44): два сквозных сценария раздела 12 -
 * «особый порядок: инвестор» и «особый порядок: почасовая».
 *
 * Площадку готовит `api demo-special-site`: у каждого прогона свой объект,
 * потому что сценарий инвестора доходит до договора, а один объект не
 * сдается на пересекающиеся периоды (INV-DB-02) - тот же довод, что
 * у площадок контура 2 (A-065). Там же помечается инвестиционная категория
 * (Q-013): без пометки правило приоритета большей суммы не включается
 * ни для одной категории и инвестиционный договор не заключить.
 *
 * Подготовка идет сборкой api в контейнере стенда, поэтому тайм-аут выше
 * обычного.
 */
test.describe.configure({ timeout: 300_000 })

/** Реквизиты заявителя Прил. 3 - одинаковы для обоих сценариев. */
async function fillApplicant(
  page: import("@playwright/test").Page,
  name: string
): Promise<void> {
  await page.locator("#special-name").fill(name)
  await page.locator("#special-idnum").fill("000000000000")
  await page.locator("#special-address").fill("г. Павлодар, ул. Ломова, 64")
  await page.locator("#special-phone").fill("+7 700 000 00 00")
}

/**
 * Подача заявки особого порядка (FR-1201, Прил. 3) участником: категория,
 * объект площадки и назначение. Возвращает id поданной заявки.
 */
async function submitSpecialRequest(
  page: import("@playwright/test").Page,
  options: {
    category: string
    objectName: string
    purpose: string
    months: string
    investment?: string
  }
): Promise<string> {
  await page.goto("/app/participant/special/new")

  await page.locator("#special-category").selectOption(options.category)
  await fillApplicant(page, "ТОО «Приемка контура 3»")
  // Объект площадки ищется по наименованию: список объектов страничный,
  // и без поиска свежий объект в него не попадает (находка приемки, T44)
  await page.locator("#special-object-search").fill(options.objectName)
  await expect(
    page
      .locator("#special-object option")
      .filter({ hasText: options.objectName })
  ).toHaveCount(1)

  // Опция подписана «наименование - адрес», поэтому значение берется
  // у опции, чей текст содержит наименование площадки
  const objectValue = await page
    .locator("#special-object option")
    .filter({ hasText: options.objectName })
    .first()
    .getAttribute("value")
  expect(objectValue, "объект площадки в списке").toBeTruthy()
  await page.locator("#special-object").selectOption(objectValue ?? "")
  await page.locator("#special-months").fill(options.months)
  if (options.investment !== undefined) {
    // Поле появляется только у инвестиционной категории (FR-1203, п. 97)
    await page.locator("#special-investment").fill(options.investment)
  }
  await page.locator("#special-purpose").fill(options.purpose)

  const submitted = page.waitForResponse(
    (response) =>
      response.url().endsWith("/api/v1/special-requests") &&
      response.request().method() === "POST"
  )
  await page.getByTestId("special-submit").click()
  const response = await submitted
  expect(response.status(), `заявка подана: ${await response.text()}`).toBe(201)

  const { id } = (await response.json()) as { id: string }
  return id
}

/** Заключение уполномоченного подразделения (FR-1202, п. 89). */
async function submitReview(
  page: import("@playwright/test").Page,
  requestId: string,
  recommendation: "grant" | "refuse"
): Promise<void> {
  await page.goto("/app/organizer/special")

  await page
    .locator(`#conclusion-${requestId}`)
    .fill(
      "Приемка контура 3: заявка соответствует требованиям категории (п. 88–89)"
    )
  await page
    .locator(`#recommendation-${requestId}`)
    .selectOption(recommendation)

  const reviewed = page.waitForResponse(
    (response) =>
      response.url().includes(`/special-requests/${requestId}/review`) &&
      response.request().method() === "POST"
  )
  await page
    .locator(`form:has(#conclusion-${requestId})`)
    .getByTestId("special-review-submit")
    .click()
  expect((await reviewed).status(), "заключение вынесено").toBe(201)
}

/**
 * Публикация результата рассмотрения (FR-1403, п. 97). Карточка ищется
 * по подпункту категории: в очереди могут ждать результаты прошлых прогонов.
 */
async function publishResult(
  page: import("@playwright/test").Page,
  ruleRef: string
): Promise<void> {
  await page.goto("/app/organizer/special")

  // Берется свое решение, а не первое подходящее: список отсортирован по
  // времени, и `.first()` цеплялся за самую старую запись стенда. Так тест
  // падал на решении из чужого прогона, у которого не записался PDF
  // (публикуется сформированный документ), а таблица решений append-only -
  // такая запись остается в очереди навсегда
  const pending = page
    .getByTestId("publications-pending")
    .locator("li")
    .filter({ hasText: `Результат по заявке` })
    .filter({ hasText: ruleRef })
    .last()
  await expect(pending).toBeVisible()

  const published = page.waitForResponse(
    (response) =>
      response.url().endsWith("/api/v1/public-records") &&
      response.request().method() === "POST"
  )
  await pending.getByTestId("publish-record").click()
  expect((await published).status(), "материал опубликован").toBe(201)
}

/** Решение Правления по заявке (FR-1202, п. 90). */
async function decide(
  page: import("@playwright/test").Page,
  requestId: string,
  decision: "grant" | "refuse"
): Promise<void> {
  await page.goto("/app/board")

  await page.locator(`#decision-${requestId}`).selectOption(decision)
  await page
    .locator(`#rationale-${requestId}`)
    .fill("Приемка контура 3: решение Правления с обоснованием (п. 90)")

  const decided = page.waitForResponse(
    (response) =>
      response.url().includes(`/special-requests/${requestId}/decision`) &&
      response.request().method() === "POST"
  )
  await page
    .locator(`form:has(#rationale-${requestId})`)
    .getByTestId("special-decide-submit")
    .click()
  expect((await decided).status(), "решение принято").toBe(201)
}

test("особый порядок: инвестор - заявка, заключение, решение, публикация и договор", async ({
  page,
  browser,
}) => {
  const objectName = `E2E контур 3 - инвестор (${Date.now()})`
  provisionSpecialSite(objectName)

  await loginAsUi(page, accounts.participant)
  const requestId =
    await test.step("заявка инвестиционной категории (FR-1201, п. 87–88)", async () =>
      submitSpecialRequest(page, {
        category: "category_10",
        objectName,
        purpose: "Приемка контура 3: инвестиционный проект на объекте площадки",
        months: "60",
        investment: "500000000",
      }))

  const staff = await browser.newContext()
  const staffPage = await staff.newPage()

  await test.step("заключение подразделения выносит заявку на Правление (FR-1202, п. 89)", async () => {
    await loginAsUi(staffPage, accounts.organizer)
    await submitReview(staffPage, requestId, "grant")

    const progress = await staffPage.request.get(
      `/api/v1/special-requests/${requestId}/progress`
    )
    expect(progress.status(), "ход рассмотрения").toBe(200)
    const state = (await progress.json()) as { review: unknown }
    expect(state.review, "заключение записано").not.toBeNull()
  })

  const board = await browser.newContext()
  const boardPage = await board.newPage()

  await test.step("Правление предоставляет объект (FR-1202, п. 90)", async () => {
    await loginAsUi(boardPage, accounts.board)
    await decide(boardPage, requestId, "grant")

    const request = await boardPage.request.get(
      `/api/v1/special-requests/${requestId}`
    )
    const state = (await request.json()) as { status: string }
    expect(state.status, "заявка удовлетворена").toBe("granted")
  })

  await test.step("досье решения собралось само (FR-1206, п. 97)", async () => {
    const dossier = await staffPage.request.get(
      `/api/v1/special-requests/${requestId}/dossier`
    )
    expect(dossier.status(), "состав досье решения").toBe(200)
    const kinds = ((await dossier.json()) as { kind: string }[]).map(
      (item) => item.kind
    )
    expect(kinds).toContain("application")
    expect(kinds).toContain("review")
    expect(kinds).toContain("decision")

    const archive = await staffPage.request.get(
      `/api/v1/special-requests/${requestId}/dossier.zip`
    )
    expect(archive.status(), "выгрузка досье архивом").toBe(200)
    expect(archive.headers()["content-type"]).toContain("application/zip")
  })

  await test.step("результат публикуется на портале (FR-1403, п. 97)", async () => {
    await publishResult(staffPage, "(п. 87.10)")

    // Портал открыт гостю (FR-1401): проверяется без сессии
    const guest = await browser.newContext()
    await guest.request.get("/")
    const portal = await guest.request.get("/api/v1/public-records")
    expect(portal.status(), "реестр публикаций портала").toBe(200)
    const records = (await portal.json()) as { kind: string; title: string }[]
    expect(
      records.some((record) => record.kind === "decision"),
      "результат особого порядка виден публично"
    ).toBe(true)
    await guest.close()
  })

  await test.step("инвестиционный договор по удовлетворенной заявке (FR-1204, п. 91)", async () => {
    await staffPage.goto("/app/organizer/investment")
    await staffPage.locator("#investment-request").selectOption(requestId)
    await staffPage.locator("#investment-term").fill("60")

    const drafted = staffPage.waitForResponse(
      (response) =>
        response.url().endsWith("/api/v1/investment-contracts") &&
        response.request().method() === "POST"
    )
    await staffPage.getByTestId("investment-draft-submit").click()
    const response = await drafted
    expect(
      response.status(),
      `договор составлен: ${await response.text()}`
    ).toBe(201)

    // Ставку считает сервер по Прил. 4, а комплект п. 91 еще не собран
    // (INV-091): договор не подписывается, пока приложений нет
    const contract = (await response.json()) as {
      monthly_rate: string
      missing_attachments: string[]
      rate_calculation: unknown
    }
    expect(Number(contract.monthly_rate)).toBeGreaterThan(0)
    expect(contract.rate_calculation, "снимок расчета заморожен").not.toBeNull()
    expect(
      contract.missing_attachments.length,
      "INV-091: комплект п. 91 не собран"
    ).toBeGreaterThan(0)
  })

  await staff.close()
  await board.close()
})

test("особый порядок: почасовая - ставка от 2 МРП/час, решение и публикация", async ({
  page,
  browser,
}) => {
  const objectName = `E2E контур 3 - почасовая (${Date.now()})`
  provisionSpecialSite(objectName)

  const staff = await browser.newContext()
  const staffPage = await staff.newPage()
  await loginAsUi(staffPage, accounts.organizer)

  await test.step("почасовая ставка считается от 2 МРП/час (FR-205, п. 97)", async () => {
    await staffPage.goto("/app/organizer/calculator")

    const previewed = staffPage.waitForResponse((response) =>
      response.url().includes("/rates/preview-hourly")
    )
    await staffPage.getByTestId("calc-hourly").click()
    expect((await previewed).status(), "расчет почасовой ставки").toBe(200)

    const breakdown = staffPage.getByTestId("hourly-breakdown")
    await expect(breakdown).toBeVisible()
    // Минимум Правил виден в расшифровке: ставка не опускается ниже него
    await expect(breakdown).toContainText(/2 МРП|минимум/i)
  })

  await loginAsUi(page, accounts.participant2)
  const requestId =
    await test.step("заявка на почасовое использование (FR-1201, п. 88)", async () =>
      submitSpecialRequest(page, {
        category: "category_1",
        objectName,
        purpose:
          "Приемка контура 3: почасовое использование помещения (п. 97), 4 часа в неделю",
        months: "12",
      }))

  await test.step("заключение и решение Правления (FR-1202, п. 89–90)", async () => {
    await submitReview(staffPage, requestId, "grant")

    const board = await browser.newContext()
    const boardPage = await board.newPage()
    await loginAsUi(boardPage, accounts.board)
    await decide(boardPage, requestId, "grant")
    await board.close()
  })

  await test.step("публикация результата и запись в досье (FR-1403, FR-1206)", async () => {
    await publishResult(staffPage, "(п. 87.1)")

    const dossier = await staffPage.request.get(
      `/api/v1/special-requests/${requestId}/dossier`
    )
    const items = (await dossier.json()) as { kind: string }[]
    expect(
      items.map((item) => item.kind),
      "факт публикации лежит в досье решения"
    ).toContain("publication")
  })

  await test.step("реестр решений видит решение (арх. § 9, T43)", async () => {
    const registry = await staffPage.request.get(
      "/api/v1/reports/decisions?from=" + new Date().toISOString().slice(0, 10)
    )
    expect(registry.status(), "реестр решений").toBe(200)
    const { rows } = (await registry.json()) as { rows: string[][] }
    expect(
      rows.some((row) => row.join(" ").includes("Приемка контура 3")),
      "решение попало в реестр отчетности"
    ).toBe(true)
  })

  await staff.close()
})
