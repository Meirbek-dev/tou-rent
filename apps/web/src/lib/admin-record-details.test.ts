import { describe, expect, it } from "vite-plus/test"
import { m } from "#/paraglide/messages"
import { adminRecordDetails } from "./admin-record-details"

describe("adminRecordDetails", () => {
  it("preserves lot and object and translates submitted status", () => {
    expect(adminRecordDetails("applications", "№9 — Boxing · submitted")).toBe(
      `№9 — Boxing · ${m.app_status_submitted()}`
    )
  })
  it("translates withdrawn applications without changing delimiters in names", () => {
    expect(
      adminRecordDetails("applications", "№2 — Hall · A · withdrawn")
    ).toBe(`№2 — Hall · A · ${m.app_status_withdrawn()}`)
  })
  it("leaves other record kinds and unknown statuses alone", () => {
    expect(adminRecordDetails("objects", "Hall · submitted")).toBe(
      "Hall · submitted"
    )
    expect(adminRecordDetails("applications", "Hall · unknown")).toBe(
      "Hall · unknown"
    )
  })
  it("handles missing details and old responses", () => {
    expect(adminRecordDetails("applications", null)).toBe("")
    expect(adminRecordDetails("applications", "Tender · submitted")).toBe(
      `Tender · ${m.app_status_submitted()}`
    )
  })
})
