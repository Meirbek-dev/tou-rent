import type { OfflineDecision, OfflineState } from "./offline-results"

/** No implicit admission: every lot needs an explicit decision and protocol excerpt. */
export function completeOfflineDecisions(
  state: OfflineState,
  choices: Record<string, OfflineDecision>
): boolean {
  return (
    state.eligible &&
    state.lots.length > 0 &&
    state.lots.every((lot) => {
      const decision = choices[lot.lot_id]
      return (
        decision !== undefined &&
        ["no_applications", "rejected", "single_source"].includes(
          decision.resolution
        ) &&
        decision.application_id === lot.application_id &&
        decision.note.trim().length > 0 &&
        decision.note.trim().length <= 2000 &&
        (lot.application_id == null
          ? decision.resolution === "no_applications"
          : decision.resolution !== "no_applications" &&
            (decision.resolution !== "single_source" ||
              (lot.price != null && lot.application_status !== "rejected")) &&
            (decision.resolution !== "rejected" ||
              lot.application_status !== "admitted"))
      )
    })
  )
}
