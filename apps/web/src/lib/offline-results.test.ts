import { describe, expect, it } from "vite-plus/test"
import { completeOfflineDecisions } from "./offline-decisions"
import type { OfflineDecision, OfflineState } from "./offline-results"

const state: OfflineState = {
  recorded: false,
  eligible: true,
  protocol_id: null,
  recorded_at: null,
  recorded_by: null,
  lots: [
    {
      lot_id: "lot",
      seq: 1,
      application_id: "app",
      applicant: "Test",
      price: "100",
      ground: null,
      resolution: null,
      note: "",
      application_status: "submitted",
    },
  ],
}
const decision: OfflineDecision = {
  lot_id: "lot",
  application_id: "app",
  resolution: "single_source",
  note: "Signed decision",
}

describe("offline decisions", () => {
  it("requires explicit decisions for all lots", () =>
    expect(completeOfflineDecisions(state, {})).toBe(false))
  it("accepts a complete transcription", () =>
    expect(completeOfflineDecisions(state, { lot: decision })).toBe(true))
  it("rejects notes without a resolution", () =>
    expect(
      completeOfflineDecisions(state, {
        lot: { ...decision, resolution: "" as OfflineDecision["resolution"] },
      })
    ).toBe(false))
  it("rejects empty notes", () =>
    expect(
      completeOfflineDecisions(state, { lot: { ...decision, note: " " } })
    ).toBe(false))
  it("rejects stale application selections", () =>
    expect(
      completeOfflineDecisions(state, {
        lot: { ...decision, application_id: "other" },
      })
    ).toBe(false))
  it("rejects ineligible tenders", () =>
    expect(
      completeOfflineDecisions({ ...state, eligible: false }, { lot: decision })
    ).toBe(false))
})
