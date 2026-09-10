import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it, vi } from "vite-plus/test"
import { CommissionApplicationMaterials } from "./commission-application-materials"
import type { ApplicationDto } from "@/lib/participant"

vi.mock("./ui/separator", () => ({ Separator: () => null }))

const application = {
  id: "application-1",
  submitted_at: "2026-09-10T10:00:00Z",
  qualification: "Qualification details",
  files: ["first", "second"].map((id) => ({
    id,
    filename: `${id}.pdf`,
    document_kind: "application_form" as const,
    content_type: "application/pdf",
    size_bytes: 100,
    uploaded_at: "2026-09-10T10:00:00Z",
  })),
} satisfies Pick<
  ApplicationDto,
  "id" | "submitted_at" | "qualification" | "files"
>

describe("commission application materials", () => {
  it("does not render sealed qualification, filenames or download links", () => {
    const html = renderToStaticMarkup(
      <CommissionApplicationMaterials
        application={application}
        lot={undefined}
        opened={false}
      />
    )
    expect(html).not.toContain("Qualification details")
    expect(html).not.toContain("first.pdf")
    expect(html).not.toContain("/files/")
  })

  it("renders every file of the same type after opening", () => {
    const html = renderToStaticMarkup(
      <CommissionApplicationMaterials
        application={application}
        lot={undefined}
        opened
      />
    )
    expect(html).toContain("Qualification details")
    expect(html).toContain("/api/v1/applications/application-1/files/first")
    expect(html).toContain("/api/v1/applications/application-1/files/second")
  })

  it("supports missing qualification and an empty file list", () => {
    const html = renderToStaticMarkup(
      <CommissionApplicationMaterials
        application={{ ...application, qualification: null, files: [] }}
        lot={undefined}
        opened
      />
    )
    expect(html).not.toContain("/files/")
    expect(html).not.toContain("Qualification details")
    expect(html).toContain("application-1")
  })
})
