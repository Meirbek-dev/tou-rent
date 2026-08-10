import { expect, test } from "@playwright/test"

import {
  accounts,
  loginAsUi,
  resetStandClock,
  shiftStandClock,
} from "./helpers"

/**
 * Сквозной путь одной цепочкой состояний (T68, ADR-0005).
 *
 * Остальные приемочные сценарии стартуют с середины: площадку готовит
 * `api demo-*`, вставляя уже сложившееся состояние. Причина законная -
 * FR-303 требует не менее десяти календарных дней между публикацией
 * и вскрытием, а прогон длится минуты. Следствие тоже реальное: стыки
 * между шагами не проверялись ни разу (ТЗ v2 § 2.4), а именно в них
 * живут ошибки, которых не видит ни один шаг по отдельности.
 *
 * Здесь тендер проходит путь сам: создан, опубликован, принял заявку,
 * пережил дедлайн и дошел до вскрытия. Разрыв в десять дней закрывается
 * сдвигом часов стенда, а не вставкой строк в БД - правила при этом
 * работают ровно те же, что на проде.
 *
 * Сдвиг глобален для стенда, поэтому часы возвращаются в `finally`:
 * упавший тест не имеет права оставить стенд в другом времени.
 */

const almatyInput = (offsetDays: number): string => {
  const at = new Date(Date.now() + offsetDays * 24 * 60 * 60 * 1000)
  return at.toISOString().slice(0, 16)
}

test("сквозной путь без вставки состояния: публикация → заявка → дедлайн → вскрытие", async ({
  page,
  browser,
}) => {
  const stamp = Date.now()
  const objectName = `T68 помещение ${stamp}`
  const tenderTitle = `T68 тендер ${stamp}`
  let tenderId = ""

  try {
    await loginAsUi(page, accounts.organizer)

    await test.step("объект и тендер с лотом (FR-101, FR-301)", async () => {
      await page.goto("/app/organizer")
      await page.locator("#obj-name").fill(objectName)
      await page.locator("#obj-address").fill("г. Павлодар, ул. Ломова, 68")
      await page.locator("#obj-area").fill("42")
      await page.getByTestId("create-object").click()
      await expect(page.getByText(objectName).first()).toBeVisible()

      await page.goto("/app/organizer/tenders/new")
      await page.locator("#tender-title").fill(tenderTitle)
      await page
        .locator("#lot-0-object")
        .selectOption({ label: `${objectName} - 42.00 м²` })
      await page.locator("#lot-0-purpose").fill("T68 назначение")
      await page.getByTestId("create-tender").click()
      await expect(
        page.getByRole("heading", { name: tenderTitle })
      ).toBeVisible()
      tenderId = new URL(page.url()).pathname.split("/").at(-1) ?? ""
      expect(tenderId, "id тендера из адреса карточки").toBeTruthy()
    })

    await test.step("публикация и открытие приема (FR-303, п. 36)", async () => {
      // Дедлайн через 11 дней, вскрытие через 12: окно FR-303 набирается
      await page.locator("#dates-deadline").fill(almatyInput(11))
      await page.locator("#dates-opening").fill(almatyInput(12))
      await page.getByTestId("save-dates").click()
      await page.getByTestId("publish-tender").click()
      await expect(page.getByTestId("publish-tender")).toBeHidden()

      const openAcceptance = page.getByRole("button", {
        name: "Открыть прием заявок",
      })
      await openAcceptance.click()
      await expect(openAcceptance).toBeHidden()
    })

    await test.step("заявка участника до дедлайна (FR-401, INV-037)", async () => {
      const participant = await browser.newContext()
      const participantPage = await participant.newPage()
      await loginAsUi(participantPage, accounts.participant)

      await participantPage.goto(`/app/participant/apply/${tenderId}`)
      // Форма отдается SSR: до гидратации ввод затирает первый рендер React,
      // поэтому шаг повторяется до успеха - как и вход (см. helpers)
      await expect(async () => {
        await participantPage.locator("#apply-name").fill("T68 участник")
        await expect(participantPage.locator("#apply-name")).toHaveValue(
          "T68 участник"
        )
      }).toPass({ timeout: 30_000 })

      await participantPage.locator("#apply-idnum").fill("990101300123")
      await participantPage
        .locator("#apply-address")
        .fill("г. Павлодар, ул. Ломова, 68")
      await participantPage.locator("#apply-phone").fill("+7 701 000 00 68")
      await participantPage.locator("#apply-price").fill("55000")

      const submitted = participantPage.waitForResponse(
        (response) =>
          response.url().includes("/applications") &&
          response.request().method() === "POST"
      )
      await participantPage
        .getByRole("button", { name: "Подать заявку" })
        .click()
      const response = await submitted
      expect(response.status(), "заявка принята (FR-401)").toBe(201)
      await participant.close()
    })

    await test.step("стенд переживает дедлайн и день вскрытия (T68)", async () => {
      // Тринадцать дней: дедлайн (11) и время заседания (12) уже позади
      shiftStandClock("13 days")

      const closed = await page.request.get(`/api/v1/tenders/${tenderId}`)
      expect(closed.status(), "карточка тендера").toBe(200)
    })

    await test.step("кворум, вскрытие и открытые цены (FR-1102, FR-403)", async () => {
      const secretary = await browser.newContext()
      const secretaryPage = await secretary.newPage()
      await loginAsUi(secretaryPage, accounts.secretary)

      await secretaryPage.goto(`/app/secretary/tenders/${tenderId}`)

      // Вскрытие возможно только на открытом заседании при кворуме
      // (FR-1102, п. 12) - это часть того же пути, а не подготовка
      const form = secretaryPage.getByTestId("attendance-form")
      const checkboxes = form.locator('input[type="checkbox"]')

      // Форма отдается SSR: до гидратации отметки не доходят до React.
      // Признак гидратации - реакция самой формы на выбор председательствующего
      await expect(async () => {
        await form.locator("#chairing").selectOption({ index: 1 })
        await expect(checkboxes.first()).toBeChecked()
      }).toPass({ timeout: 30_000 })

      const total = await checkboxes.count()
      for (let index = 0; index < total; index += 1) {
        await checkboxes.nth(index).check()
      }
      const saved = secretaryPage.waitForResponse(
        (response) =>
          response.url().includes("/meeting/attendance") &&
          response.request().method() === "POST"
      )
      await secretaryPage.getByTestId("save-attendance").click()
      await saved
      await secretaryPage.getByTestId("open-meeting").click()
      await expect(secretaryPage.getByTestId("meeting-quorum")).toContainText(
        "из"
      )

      await secretaryPage.getByTestId("open-tender").click()
      await expect(secretaryPage.getByTestId("open-tender")).toBeHidden()

      // Цена, скрытая до вскрытия (INV-040), становится видна комиссии
      await expect(secretaryPage.getByText("55 000").first()).toBeVisible()
      await secretary.close()
    })
  } finally {
    resetStandClock()
  }
})
