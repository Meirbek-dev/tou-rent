import { describe, expect, it } from "vite-plus/test"

import { deadlinePassed, intakeClosed } from "./intake"

/**
 * Срок приема заявок (FR-401, FR-501).
 *
 * Пока этой пары не было, «прием закрыт» знал только бейдж объявления:
 * мастер подачи оставался открытым после срока и отказ `application_deadline_passed`
 * приходил уже по заполненной форме, а реестры кабинетов писали «Прием заявок».
 */

const DEADLINE = "2026-09-02T16:52:00Z"
const BEFORE = Date.parse("2026-09-02T16:51:00Z")
const AFTER = Date.parse("2026-09-02T16:53:00Z")

describe("срок приема заявок", () => {
  it("до срока не истек, после - истек", () => {
    expect(deadlinePassed(DEADLINE, BEFORE)).toBe(false)
    expect(deadlinePassed(DEADLINE, AFTER)).toBe(true)
  })

  it("ровно в момент срока прием уже закрыт", () => {
    expect(deadlinePassed(DEADLINE, Date.parse(DEADLINE))).toBe(true)
  })

  it("до монтирования срок не считается: SSR и первый кадр совпадают", () => {
    expect(deadlinePassed(DEADLINE, null)).toBe(false)
    expect(intakeClosed("accepting", DEADLINE, null)).toBe(false)
  })

  it("срока нет или он не разбирается - прием не закрыт", () => {
    expect(deadlinePassed(null, AFTER)).toBe(false)
    expect(deadlinePassed(undefined, AFTER)).toBe(false)
    expect(deadlinePassed("", AFTER)).toBe(false)
    expect(deadlinePassed("не дата", AFTER)).toBe(false)
  })

  it("закрытый прием - это только статус «прием заявок» с вышедшим сроком", () => {
    expect(intakeClosed("accepting", DEADLINE, AFTER)).toBe(true)
    expect(intakeClosed("accepting", DEADLINE, BEFORE)).toBe(false)
    // Вскрытие уже прошло - статус говорит сам за себя
    expect(intakeClosed("qualification", DEADLINE, AFTER)).toBe(false)
    expect(intakeClosed("announced", DEADLINE, AFTER)).toBe(false)
    expect(intakeClosed("trading", DEADLINE, AFTER)).toBe(false)
  })
})
