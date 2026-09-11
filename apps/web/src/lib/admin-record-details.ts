import { m } from "#/paraglide/messages"
import type { ApplicationStatus } from "./participant"

const LABELS: Record<ApplicationStatus, () => string> = {
  submitted: m.app_status_submitted,
  withdrawn: m.app_status_withdrawn,
  fee_confirmed: m.app_status_fee_confirmed,
  admitted: m.app_status_admitted,
  rejected: m.app_status_rejected,
}

/** Translate only the final status suffix; preserve object names verbatim. */
export function adminRecordDetails(
  kind: string,
  details: string | null | undefined
): string {
  if (details == null) return ""
  if (kind !== "applications") return details
  const separator = details.lastIndexOf(" · ")
  if (separator < 0) return details
  const status = details.slice(separator + 3)
  switch (status) {
    case "submitted":
    case "withdrawn":
    case "fee_confirmed":
    case "admitted":
    case "rejected":
      return `${details.slice(0, separator)} · ${LABELS[status]()}`
    default:
      return details
  }
}
