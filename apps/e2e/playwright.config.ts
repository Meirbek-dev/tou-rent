import { defineConfig, devices } from "@playwright/test"

/**
 * E2E демо-сценария § 9.1 (T14, гейт G11). Прогон идет против поднятого
 * стенда: `vp run stack:up` (api :8080) + `vp run dev` (web :3000).
 * Адрес переопределяется E2E_BASE_URL - вторая dev-сессия слушает :3105.
 */
export default defineConfig({
  testDir: "./tests",
  // Сценарий последовательный: секретарь, участники и торги делят один тендер
  fullyParallel: false,
  workers: 1,
  timeout: 120_000,
  expect: { timeout: 15_000 },
  retries: process.env["CI"] === undefined ? 0 : 1,
  reporter:
    process.env["CI"] === undefined
      ? [["list"]]
      : [["list"], ["junit", { outputFile: "results.xml" }]],
  use: {
    baseURL: process.env["E2E_BASE_URL"] ?? "http://localhost:3000",
    locale: "ru-RU",
    timezoneId: "Asia/Almaty",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
})
