import { execFileSync } from "node:child_process"

import { expect } from "@playwright/test"

import type { APIRequestContext, Browser, Page } from "@playwright/test"

/** Пароль seed-аккаунтов стенда (NFR-09: в репозитории не хранится). */
export const seedPassword = process.env["E2E_PASSWORD"] ?? "TouDemo2026Stand!"

export const accounts = {
  organizer: "organizer@tou.demo",
  secretary: "secretary@tou.demo",
  finance: "finance@tou.demo",
  participant: "participant@tou.demo",
  participant2: "participant2@tou.demo",
  participant3: "participant3@tou.demo",
  chairman: "commission1@tou.demo",
  board: "board@tou.demo",
} as const

/**
 * Голосующий состав демо-комиссии (FR-1101): председатель, заместитель и
 * пять членов. Резервные (8, 9) голоса не имеют, пока не заменили отведенного.
 */
export const commissionMembers = [
  "commission1@tou.demo",
  "commission2@tou.demo",
  "commission@tou.demo",
  "commission4@tou.demo",
  "commission5@tou.demo",
  "commission6@tou.demo",
  "commission7@tou.demo",
] as const

/**
 * Вход через форму (FR-1501) - так же, как его показывают на демо.
 *
 * Страница логина отдается SSR: до гидратации ввод и клик уходят в «мертвую»
 * разметку - значения затирает первый рендер React. Поэтому весь шаг
 * повторяется до успеха, а не ждет гидратацию по косвенным признакам.
 */
export async function loginAsUi(page: Page, email: string): Promise<void> {
  await page.goto("/auth/login")

  await expect(async () => {
    await page.locator("#login-email").fill(email)
    await page.locator("#login-password").fill(seedPassword)
    await expect(page.locator("#login-email")).toHaveValue(email)
    await page.getByTestId("login-submit").click()
    await expect(page).toHaveURL(/\/app(\/|$)/, { timeout: 3_000 })
  }).toPass({ timeout: 45_000 })
}

/** Вход по API - для подготовки состояния, которое демо не показывает. */
export async function loginApi(
  request: APIRequestContext,
  email: string
): Promise<void> {
  const response = await request.post("/api/v1/auth/login", {
    data: { email, password: seedPassword },
  })
  expect(response.status(), `вход ${email}`).toBe(200)
}

/**
 * Свежий «горячий» тендер § 9.1: три заявки поданы, прием закрыт, время
 * заседания наступило. Публикация требует ≥10 дней до вскрытия (FR-303),
 * поэтому дойти до вскрытия внутри одного прогона можно только так -
 * подготовку делает `api demo-tender` (та же логика, что у seed Прил. Б).
 */
export function provisionDemoTender(): string {
  return provision("demo-tender")
}

/**
 * Тендер с завершенными торгами (T32): площадка сценариев «полный тендер
 * до договора» и «уклонение победителя → № 2». Довести свежий тендер до
 * торгов внутри прогона мешает тот же FR-303.
 */
export function provisionSummedUpTender(): string {
  return provision("demo-summed-up")
}

/** Тендер с единственной заявкой и истекшим приемом (основание п. 81.2). */
export function provisionSingleApplicationTender(): string {
  return provision("demo-single-application")
}

/**
 * Площадка сценариев контура 3 (T44): свой объект на прогон и помеченная
 * инвестиционная категория (Q-013). Свой объект нужен потому, что сценарий
 * инвестора доходит до договора, а один объект не сдается на пересекающиеся
 * периоды (INV-DB-02).
 */
export function provisionSpecialSite(objectName: string): string {
  return provision("demo-special-site", objectName)
}

/**
 * Подготовка площадки подкомандой api. Команда переопределяется
 * `E2E_PROVISION_CMD` (подкоманда подставляется вместо `{}`); по умолчанию -
 * `podman exec` в контейнер дев-стенда.
 */
/**
 * Сдвиг часов стенда (T68, ADR-0005): единственный способ провести сквозной
 * сценарий цепочкой, не вставляя готовое состояние. FR-303 требует не менее
 * десяти календарных дней между публикацией и вскрытием, и без сдвига этот
 * разрыв обойти нечем.
 *
 * Сдвиг глобален для стенда, поэтому сценарий обязан вернуть часы -
 * `finally`, а не «в конце теста».
 */
export function shiftStandClock(interval: string): void {
  const template =
    process.env["E2E_PROVISION_CMD"] ??
    "podman exec tou-rent-dev-api-1 cargo run -q -p api -- {}"
  // Сдвиг требует явного намерения (ADR-0005): без ALLOW_TIME_SHIFT
  // подкоманда откажет, и это правильно - на проде переменной нет
  const command = template
    .replace("podman exec ", "podman exec -e ALLOW_TIME_SHIFT=1 ")
    .replace("{}", "time-shift")

  const [file, ...args] = command.split(" ")
  if (file === undefined) throw new Error("E2E_PROVISION_CMD пуст")
  args.push(interval)

  execFileSync(file, args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
    timeout: 300_000,
  })
}

export function resetStandClock(): void {
  shiftStandClock("reset")
}

function provision(subcommand: string, argument?: string): string {
  const template =
    process.env["E2E_PROVISION_CMD"] ??
    "podman exec tou-rent-dev-api-1 cargo run -q -p api -- {}"
  const command = template.replace("{}", subcommand)

  const [file, ...args] = command.split(" ")
  if (file === undefined) throw new Error("E2E_PROVISION_CMD пуст")
  // Аргумент идет отдельным элементом argv: в наименовании площадки пробелы
  if (argument !== undefined) args.push(argument)

  const output = execFileSync(file, args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
    timeout: 300_000,
  })
  const id = output.trim().split("\n").at(-1)?.trim()
  if (id === undefined || !/^[0-9a-f-]{36}$/i.test(id)) {
    throw new Error(`не удалось получить id тендера: ${JSON.stringify(output)}`)
  }
  return id
}

/** Заголовок карточки заявки в решениях комиссии - по имени заявителя. */
export function decisionForm(page: Page, applicantName: string) {
  return page.locator("form").filter({ hasText: applicantName })
}

/**
 * Голоса членов комиссии по заявке (FR-1103). В демо один член голосует
 * через кабинет, остальные - по API: сценарий проверяет правило большинства,
 * а не семь одинаковых кликов.
 */
export async function castVotes(
  browser: Browser,
  applicationId: string,
  votes: Record<string, "for" | "against">
): Promise<void> {
  for (const [email, value] of Object.entries(votes)) {
    const context = await browser.newContext()
    await loginApi(context.request, email)
    const csrf = (await context.cookies()).find((c) => c.name === "tou_csrf")
    const response = await context.request.post(
      `/api/v1/applications/${applicationId}/vote`,
      {
        data: { value },
        headers: { "x-csrf-token": csrf?.value ?? "" },
      }
    )
    // Отказ правила приходит problem+json: без его текста разбирать нечего
    expect(response.status(), `голос ${email}: ${await response.text()}`).toBe(
      200
    )
    await context.close()
  }
}
