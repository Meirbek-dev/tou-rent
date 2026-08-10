import { expect, test } from "@playwright/test"

import {
  accounts,
  castVotes,
  commissionMembers,
  decisionForm,
  loginAsUi,
  provisionDemoTender,
} from "./helpers"

/**
 * Демо-сценарий § 9.1 (гейт G11). Разбит по границе, которую задают сами
 * Правила: публикация требует ≥10 календарных дней до вскрытия (FR-303),
 * поэтому первая половина идет на свежесозданном тендере, а вторая - на
 * подготовленном (`api demo-tender`, та же логика, что у seed Прил. Б).
 */

const almatyInput = (offsetDays: number): string => {
  const at = new Date(Date.now() + offsetDays * 24 * 60 * 60 * 1000)
  return at.toISOString().slice(0, 16)
}

test("организатор: объект → расчет ставки → тендер → публикация → портал", async ({
  page,
}) => {
  const stamp = Date.now()
  const objectName = `E2E помещение ${stamp}`
  const tenderTitle = `E2E тендер ${stamp}`

  await loginAsUi(page, accounts.organizer)

  await test.step("объект имущества (FR-101)", async () => {
    await page.goto("/app/organizer")
    await page.locator("#obj-name").fill(objectName)
    await page.locator("#obj-address").fill("г. Павлодар, ул. Ломова, 64")
    await page.locator("#obj-area").fill("42")
    await page.getByTestId("create-object").click()
    await expect(page.getByText(objectName).first()).toBeVisible()
  })

  await test.step("калькулятор ставки (FR-201, Прил. 4)", async () => {
    await page.goto("/app/organizer/calculator")
    await page.locator("#calc-area").fill("42")
    await page.getByTestId("calc-submit").click()
    // Расшифровка приходит с сервера: ждем строку месячной ставки (FR-201)
    await expect(page.getByText("Ставка в месяц")).toBeVisible()
  })

  await test.step("тендер с лотом (FR-301)", async () => {
    await page.goto("/app/organizer/tenders/new")
    await page.locator("#tender-title").fill(tenderTitle)
    // Подпись опции - «имя - площадь м²»
    await page
      .locator("#lot-0-object")
      .selectOption({ label: `${objectName} - 42.00 м²` })
    await page.locator("#lot-0-purpose").fill("E2E назначение")
    await page.getByTestId("create-tender").click()
    await expect(page.getByRole("heading", { name: tenderTitle })).toBeVisible()
  })

  await test.step("даты и публикация (FR-303: ≥10 дней до вскрытия)", async () => {
    await page.locator("#dates-deadline").fill(almatyInput(12))
    await page.locator("#dates-opening").fill(almatyInput(13))
    await page.getByTestId("save-dates").click()
    await page.getByTestId("publish-tender").click()
    await expect(page.getByTestId("publish-tender")).toBeHidden()
  })

  await test.step("объявление на публичном портале (FR-1401) и PDF", async () => {
    await page.goto("/tenders")
    const card = page.getByText(tenderTitle).first()
    await expect(card).toBeVisible()
    await card.click()

    const tenderId = new URL(page.url()).pathname.split("/").at(-1)
    expect(tenderId, "id тендера из адреса карточки").toBeTruthy()

    const pdf = await page.request.get(
      `/api/v1/tenders/${tenderId}/announcement.pdf`
    )
    expect(pdf.status(), "PDF объявления").toBe(200)
    expect(pdf.headers()["content-type"]).toContain("application/pdf")
  })
})

test("комиссия и торги: вскрытие → допуск → уведомления → торги на двух экранах → протоколы", async ({
  page,
  browser,
}) => {
  const tenderId = provisionDemoTender()

  await loginAsUi(page, accounts.secretary)
  await page.goto(`/app/secretary/tenders/${tenderId}`)

  await test.step("явка и кворум: заседание открывается (FR-1102, п. 12)", async () => {
    const form = page.getByTestId("attendance-form")
    const checkboxes = form.locator('input[type="checkbox"]')

    // Форма отдается SSR: до гидратации отметки не доходят до React.
    // Признак гидратации - реакция самой формы: выбор председательствующего
    // отмечает его присутствующим (это делает React, а не разметка)
    await expect(async () => {
      await form.locator("#chairing").selectOption({ index: 1 })
      await expect(checkboxes.first()).toBeChecked()
    }).toPass({ timeout: 30_000 })

    // Присутствуют все члены комиссии
    const total = await checkboxes.count()
    for (let index = 0; index < total; index += 1) {
      await checkboxes.nth(index).check()
    }
    await expect(checkboxes.last()).toBeChecked()
    // Явка сохраняется отдельным запросом - заседание открываем после него
    const saved = page.waitForResponse(
      (response) =>
        response.url().includes("/meeting/attendance") &&
        response.request().method() === "POST"
    )
    await page.getByTestId("save-attendance").click()
    await saved
    await page.getByTestId("open-meeting").click()
    await expect(page.getByTestId("meeting-quorum")).toContainText("из")
  })

  await test.step("вскрытие открывает цены (FR-403, INV-040)", async () => {
    await expect(page.getByTestId("open-tender")).toBeVisible()
    await page.getByTestId("open-tender").click()
    await expect(page.getByTestId("open-tender")).toBeHidden()
    // Три первоначальных предложения демо-участников
    await expect(
      page.getByText("55 000", { exact: false }).first()
    ).toBeVisible()
    await expect(
      page.getByText("48 000", { exact: false }).first()
    ).toBeVisible()
  })

  await test.step("комиссия голосует лично (FR-1103), решение - большинством", async () => {
    const applications = await page.request.get(
      `/api/v1/tenders/${tenderId}/applications`
    )
    expect(applications.status()).toBe(200)
    const items = (await applications.json()) as {
      id: string
      applicant_details: { name?: string }
    }[]

    for (const application of items) {
      const name = application.applicant_details.name ?? ""
      // Третьему участнику комиссия отказывает большинством голосов
      const against = name.includes("Участник 3")
      const votes = Object.fromEntries(
        commissionMembers.map((email) => [
          email,
          against ? ("against" as const) : ("for" as const),
        ])
      )
      await castVotes(browser, application.id, votes)
    }

    await page.reload()
    for (const applicant of ["ТОО «Демо-Участник»", "ТОО «Демо-Участник 2»"]) {
      const form = decisionForm(page, applicant).first()
      await expect(form).toContainText("допустить")
      await form.getByTestId("decide-submit").click()
      // Решенная заявка уходит из блока решений комиссии
      await expect(form).toBeHidden()
    }

    const rejected = decisionForm(page, "ТОО «Демо-Участник 3»").first()
    await expect(rejected).toContainText("отклонить")
    // Основание - из закрытого перечня п. 52 (INV-052)
    await rejected.locator("select").first().selectOption({ index: 0 })
    await rejected.getByTestId("decide-submit").click()
    await expect(rejected).toBeHidden()
  })

  await test.step("протокол допуска PDF (FR-503, п. 55)", async () => {
    await page.getByTestId("generate-admission-protocol").click()
    await expect(page.getByTestId("admission-protocol-pdf")).toBeVisible()

    const pdf = await page.request.get(
      `/api/v1/tenders/${tenderId}/admission-protocol.pdf`
    )
    expect(pdf.status(), "PDF протокола допуска").toBe(200)
  })

  await test.step("уведомление допущенных (FR-504) доходит до колокольчика (FR-1301)", async () => {
    await page.getByTestId("notify-admitted").click()

    const participant = await browser.newContext()
    const participantPage = await participant.newPage()
    await loginAsUi(participantPage, accounts.participant)
    await expect(participantPage.getByTestId("unread-badge")).toBeVisible()
    await participant.close()
  })

  const auctionUrl =
    await test.step("открытие комнаты торгов (FR-601)", async () => {
      await page.reload()
      await page.getByTestId("open-auction").first().click()
      const link = page.getByRole("link", { name: /комнат/i }).first()
      await link.click()
      await expect(page).toHaveURL(/\/app\/auctions\//)
      await page.getByTestId("start-auction").click()
      await expect(page.getByTestId("auction-status")).toContainText(
        "Идут торги"
      )
      return page.url()
    })

  await test.step("два экрана торгуются в реальном времени (FR-603, INV-063)", async () => {
    const first = await browser.newContext()
    const second = await browser.newContext()
    const firstPage = await first.newPage()
    const secondPage = await second.newPage()

    await loginAsUi(firstPage, accounts.participant)
    await loginAsUi(secondPage, accounts.participant2)
    await firstPage.goto(auctionUrl)
    await secondPage.goto(auctionUrl)

    // Первая ставка - минимально допустимая (старт + шаг 5 %)
    const minNext = await firstPage.locator("#bid-amount").inputValue()
    await firstPage.getByTestId("place-bid").click()
    await expect(firstPage.getByTestId("bid-feed")).toContainText(
      minNext.slice(0, 2)
    )
    // Лента второго участника обновляется без перезагрузки
    await expect(secondPage.getByTestId("bid-feed")).not.toBeEmpty()

    const secondBid = await secondPage.locator("#bid-amount").inputValue()
    await secondPage.getByTestId("place-bid").click()
    await expect(secondPage.getByTestId("bid-feed")).toContainText(
      secondBid.slice(0, 2)
    )

    await first.close()
    await second.close()
  })

  await test.step("завершение торгов и протокол итогов (FR-606, FR-701)", async () => {
    await page.getByTestId("finish-auction").click()
    await expect(page.getByTestId("auction-winner")).toBeVisible()

    await page.goto(`/app/secretary/tenders/${tenderId}`)
    await page.getByTestId("generate-results-protocol").click()
    await expect(page.getByTestId("results-protocol-pdf")).toBeVisible()

    const pdf = await page.request.get(
      `/api/v1/tenders/${tenderId}/results-protocol.pdf`
    )
    expect(pdf.status(), "PDF протокола итогов").toBe(200)
  })
})
