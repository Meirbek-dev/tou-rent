import { expect, test } from "@playwright/test"

import {
  accounts,
  loginAsUi,
  provisionSingleApplicationTender,
  provisionSummedUpTender,
} from "./helpers"

/**
 * Приемка контура 2 (ТЗ § 10в, T32): три сквозных сценария Правил после
 * итогов торгов - «полный тендер до договора», «несостоявшийся → повтор»
 * и «победитель уклонился → № 2».
 *
 * Площадки готовятся подкомандами `api demo-summed-up` и
 * `api demo-single-application`: довести свежий тендер до торгов внутри
 * прогона мешает FR-303 (≥10 календарных дней до вскрытия), как и в § 9.1.
 * Подготовка идет сборкой api в контейнере стенда, поэтому тайм-аут выше
 * обычного.
 */
test.describe.configure({ timeout: 300_000 })

/** Шаг конвейера договора (FR-902, п. 110–115) кнопкой карточки. */
async function advanceContract(
  page: import("@playwright/test").Page,
  times: number
): Promise<void> {
  for (let step = 0; step < times; step += 1) {
    const button = page.getByTestId("advance-contract").first()
    await expect(button).toBeVisible()
    const before = await page.getByTestId("contract-stage").first().innerText()
    await button.click()
    await expect(page.getByTestId("contract-stage").first()).not.toHaveText(
      before
    )
  }
}

test("полный тендер до договора: конвейер п. 110–115, регистрация и акт передачи", async ({
  page,
}) => {
  const tenderId = provisionSummedUpTender()

  await loginAsUi(page, accounts.organizer)
  await page.goto(`/app/organizer/tenders/${tenderId}?tab=contracts`)

  await test.step("договор составляется из итогов торгов (FR-901, п. 108)", async () => {
    await page.getByTestId("draft-contract").first().click()
    const card = page.getByTestId("contract-card").first()
    await expect(card).toBeVisible()
    // Существенные условия - снимок торгов: ставка победителя
    await expect(card).toContainText("60 500")
    await expect(page.getByTestId("contract-stage").first()).toContainText(
      "договор составлен"
    )
  })

  await test.step("шаги конвейера идут по порядку (FR-902, п. 110–112)", async () => {
    // Передача экземпляра → возврат подписанного → документы для сверки
    await advanceContract(page, 3)
    await expect(page.getByTestId("contract-stage").first()).toContainText(
      "документы для сверки"
    )
  })

  await test.step("INV-115: без сверки наймодатель не подписывает (п. 113, 115)", async () => {
    const checklist = page.getByTestId("checklist").first()
    await expect(checklist).toBeVisible()

    // Отметка «сверка завершена» - еще не подпись: следующий шаг конвейера
    await advanceContract(page, 1)
    await expect(page.getByTestId("contract-stage").first()).toContainText(
      "сверка документов завершена"
    )

    // А вот подпись наймодателя без отмеченного перечня не проходит.
    // Проверяется текст интерфейса, а не имя инварианта: причина отказа
    // приходит машинным полем `rule`, а строка берется из каталога переводов
    // (W-09) - «INV-115» пользователю больше не показывается.
    await page.getByTestId("advance-contract").first().click()
    await expect(page.getByRole("alert").first()).toContainText(
      "Сверка документов не завершена"
    )

    const items = checklist.locator('input[type="checkbox"]')
    const total = await items.count()
    for (let index = 0; index < total; index += 1) {
      // Отметка возвращается с сервера (п. 113): клик, затем ожидание факта
      await items.nth(index).click()
      await expect(items.nth(index)).toBeChecked()
    }
  })

  await test.step("подписание и направление экземпляра (п. 114–115)", async () => {
    await advanceContract(page, 2)
    await expect(page.getByTestId("contract-stage").first()).toContainText(
      "экземпляр направлен"
    )
  })

  await test.step("регистрация в журнале дает дату заключения (FR-905, п. 126)", async () => {
    const regNumber = `Д-E2E-${Date.now()}`
    await page.getByLabel(/номер в журнале/i).fill(regNumber)
    await page.getByTestId("register-contract").click()
    await expect(page.getByTestId("contract-card").first()).toContainText(
      regNumber
    )
  })

  await test.step("акт приема-передачи начинает начисление платы (FR-904, п. 122)", async () => {
    const acts = page.getByTestId("create-act").first()
    await expect(acts).toBeVisible()
    await page
      .getByLabel(/дата акта/i)
      .first()
      .fill(new Date().toISOString().slice(0, 10))
    await acts.click()
    await expect(page.getByTestId("acts").first()).toContainText(
      "акт приема-передачи"
    )
  })

  await test.step("досье собралось само (FR-1602, п. 16)", async () => {
    const dossier = await page.request.get(
      `/api/v1/tenders/${tenderId}/dossier`
    )
    expect(dossier.status(), "состав досье").toBe(200)
    const items = (await dossier.json()) as { kind: string }[]
    expect(items.map((item) => item.kind)).toContain("contract")
    expect(items.map((item) => item.kind)).toContain("act")

    const archive = await page.request.get(
      `/api/v1/tenders/${tenderId}/dossier.zip`
    )
    expect(archive.status(), "выгрузка досье архивом").toBe(200)
    expect(archive.headers()["content-type"]).toContain("application/zip")
  })
})

test("несостоявшийся → повтор: основание п. 81.2, протокол и повторный тендер", async ({
  page,
  browser,
}) => {
  const tenderId = provisionSingleApplicationTender()

  await test.step("секретарь видит наступившее основание (FR-801, п. 81.2)", async () => {
    await loginAsUi(page, accounts.secretary)
    await page.goto(`/app/secretary/tenders/${tenderId}?tab=risk`)

    const panel = page.getByTestId("failure-panel")
    await expect(panel).toBeVisible()
    await expect(page.getByTestId("failure-ground")).toContainText(
      "единственная заявка"
    )
  })

  await test.step("признание несостоявшимся и протокол (FR-802, п. 82)", async () => {
    await page.getByTestId("declare-failed").click()
    await expect(page.getByTestId("failed-protocol")).toBeVisible()

    const generated = page.waitForResponse((response) =>
      response.url().includes("/failed-protocol")
    )
    await page.getByTestId("failed-protocol").click()
    expect((await generated).status(), "протокол о несостоявшемся").toBe(201)

    const pdf = await page.request.get(
      `/api/v1/tenders/${tenderId}/failed-protocol.pdf`
    )
    expect(pdf.status(), "PDF протокола о несостоявшемся").toBe(200)
  })

  await test.step("организатор объявляет повторный тендер (п. 82)", async () => {
    const organizer = await browser.newContext()
    const organizerPage = await organizer.newPage()
    await loginAsUi(organizerPage, accounts.organizer)
    await organizerPage.goto(`/app/organizer/tenders/${tenderId}?tab=risk`)

    const created = organizerPage.waitForResponse(
      (response) =>
        response.url().includes(`/tenders/${tenderId}/repeat`) &&
        response.request().method() === "POST"
    )
    await organizerPage.getByTestId("repeat-tender").click()
    const response = await created
    expect(response.status(), "повторный тендер объявлен").toBe(201)
    await expect(organizerPage.getByTestId("failure-panel")).toContainText(
      /повторн/i
    )

    // Повторный тендер - отдельный черновик со ссылкой на несостоявшийся
    const { tender_id: repeatId } = (await response.json()) as {
      tender_id: string
    }
    const repeat = await organizerPage.request.get(
      `/api/v1/tenders/${repeatId}`
    )
    expect(repeat.status(), "карточка повторного тендера").toBe(200)
    const draft = (await repeat.json()) as {
      status: string
      repeat_of: string | null
      lots: unknown[]
    }
    expect(draft.repeat_of, "ссылка на несостоявшийся тендер").toBe(tenderId)
    expect(draft.status, "повтор начинается черновиком").toBe("draft")
    expect(draft.lots.length, "лоты перенесены в повтор").toBeGreaterThan(0)

    await organizer.close()
  })
})

test("победитель уклонился → № 2: удержание взноса, протокол и договор со вторым местом", async ({
  page,
  browser,
}) => {
  const tenderId = provisionSummedUpTender()

  await loginAsUi(page, accounts.organizer)
  await page.goto(`/app/organizer/tenders/${tenderId}?tab=contracts`)

  await test.step("договор победителя передан на подписание (п. 110)", async () => {
    await page.getByTestId("draft-contract").first().click()
    await expect(page.getByTestId("contract-card").first()).toBeVisible()
    await advanceContract(page, 1)
    await expect(page.getByTestId("contract-stage").first()).toContainText(
      "экземпляр передан"
    )
  })

  await test.step("уклонение фиксируется с основанием п. 116 (FR-903)", async () => {
    const select = page.getByLabel(/основание уклонения/i).first()
    await select.selectOption({ index: 1 })
    await page.getByTestId("declare-evasion").click()

    // Договор прекращен, взнос удержан - карточка говорит об этом прямо
    await expect(page.getByTestId("contract-card").first()).toContainText(
      /взнос удержан/i
    )
    await expect(page.getByTestId("evasion-panel")).toBeVisible()
  })

  await test.step("взнос уклонившегося удержан проводкой книги (п. 116)", async () => {
    const finance = await browser.newContext()
    const financePage = await finance.newPage()
    await loginAsUi(financePage, accounts.finance)

    const accountsResponse = await financePage.request.get(
      "/api/v1/ledger/accounts?kind=participant_fee"
    )
    expect(accountsResponse.status()).toBe(200)
    const rows = (await accountsResponse.json()) as {
      tender_id: string | null
      balance: string
    }[]
    const held = rows.filter((row) => row.tender_id === tenderId)
    expect(held.length, "счета взносов тендера").toBeGreaterThan(0)
    expect(
      held.some((row) => Number(row.balance) === 0),
      "у уклонившегося остатка не осталось"
    ).toBe(true)

    await finance.close()
  })

  await test.step("протокол о победителе № 2 и уведомление (п. 117–118)", async () => {
    const secretary = await browser.newContext()
    const secretaryPage = await secretary.newPage()
    await loginAsUi(secretaryPage, accounts.secretary)
    await secretaryPage.goto(`/app/secretary/tenders/${tenderId}?tab=risk`)

    const generated = secretaryPage.waitForResponse((response) =>
      response.url().includes("/winner2-protocol")
    )
    await secretaryPage.getByTestId("winner2-protocol").click()
    expect((await generated).status(), "протокол о победителе № 2").toBe(201)

    const pdf = await secretaryPage.request.get(
      `/api/v1/tenders/${tenderId}/winner2-protocol.pdf`
    )
    expect(pdf.status(), "PDF протокола о победителе № 2").toBe(200)

    await secretary.close()
  })

  await test.step("договор составляется с участником № 2 на его ставку (п. 117)", async () => {
    await page.reload()
    await page.getByTestId("draft-contract").first().click()

    const cards = page.getByTestId("contract-card")
    await expect(cards).toHaveCount(2)
    // Второй договор - с участником № 2 и на его ставку из торгов
    // (порядок карточек по одному лоту не определен, поэтому смотрим секцию)
    const second = cards.filter({ hasText: "участник № 2" })
    await expect(second).toHaveCount(1)
    await expect(second).toContainText("57 750")
  })
})
