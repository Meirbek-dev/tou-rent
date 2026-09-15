import type { CommissionDocument } from "./commission-documents"

/** Presentation only; the server independently authorizes every list and PDF. */
export function documentsForApplication(
  documents: CommissionDocument[],
  applicationId: string | undefined,
  includeCommon: boolean
) {
  return documents.filter(
    (d) =>
      d.application_id === (applicationId ?? null) ||
      (includeCommon && d.application_id === null)
  )
}
