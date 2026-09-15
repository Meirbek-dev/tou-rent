import { describe, expect, it } from "vite-plus/test"
import { documentsForApplication } from "./commission-document-placement"
import type { CommissionDocument } from "./commission-documents"

const document = (
  id: string,
  application: string | null
): CommissionDocument => ({
  id,
  application_id: application,
  tender_id: "tender",
  title: "Протокол",
  number: "1",
  document_date: "2026-09-11",
  filename: "document.pdf",
  size_bytes: 10,
  uploaded_by: "secretary",
  uploaded_at: "2026-09-14T00:00:00Z",
  shared_at: null,
})
describe("document placement", () => {
  const documents = [
    document("common", null),
    document("own", "a"),
    document("other", "b"),
  ]
  it("shows only common documents in the secretary protocol tab", () => {
    expect(
      documentsForApplication(documents, undefined, false).map((d) => d.id)
    ).toEqual(["common"])
  })
  it("shows only the selected application in the secretary dialog", () => {
    expect(
      documentsForApplication(documents, "a", false).map((d) => d.id)
    ).toEqual(["own"])
  })
  it("shows common and own documents in the application", () => {
    expect(
      documentsForApplication(documents, "a", true).map((d) => d.id)
    ).toEqual(["common", "own"])
  })
})
