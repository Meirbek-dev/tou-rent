import { describe, expect, it, vi } from "vite-plus/test"
import { unzipSync } from "fflate"
import { m } from "#/paraglide/messages"
import {
  applicationArchiveEntries,
  applicationDocumentLabel,
  ARCHIVE_MAX_BYTES,
  createApplicationArchive,
  safeArchiveName,
} from "./application-archive"
import type { ApplicationDto } from "./participant"

const file = {
  id: "file-1",
  filename: "Справка.pdf",
  document_kind: "tax_clearance" as const,
  content_type: "application/pdf",
  size_bytes: 3,
  uploaded_at: "2026-09-14T00:00:00Z",
}
function application(id = "app-1", name = "Иванов И.И."): ApplicationDto {
  return {
    id,
    lot_id: "lot-2",
    tender_id: "tender",
    participant_id: "user",
    applicant_kind: "individual",
    applicant_details: JSON.parse(JSON.stringify({ name })),
    files: [file],
    status: "submitted",
    submitted_at: file.uploaded_at,
    package_complete: false,
  }
}
const lots = [{ id: "lot-2", seq: 2 }]

describe("application archives", () => {
  it("uses the site's document label, with flat files for one application", () => {
    const [entry] = applicationArchiveEntries([application()], lots, false)
    expect(entry?.path).toBe(applicationDocumentLabel(file))
    expect(entry?.url).toBe("/api/v1/applications/app-1/files/file-1")
  })
  it("groups every application by lot and applicant, including empty applications", () => {
    const empty = { ...application("app-2", "Петров"), files: [] }
    const entries = applicationArchiveEntries(
      [application(), empty],
      lots,
      true
    )
    expect(entries.map((e) => e.path)).toEqual([
      "Лот № 2 — Иванов И.И/",
      `Лот № 2 — Иванов И.И/${applicationDocumentLabel(file)}`,
      "Лот № 2 — Петров/",
    ])
  })
  it("does not overwrite duplicate names, even case-insensitive or literal numbered names", () => {
    const app = application()
    app.files = [
      file,
      { ...file, id: "two" },
      { ...file, id: "three", filename: "Справка (2).pdf" },
    ]
    const entries = applicationArchiveEntries(
      [app, { ...app, id: "second" }],
      lots,
      true
    )
    expect(new Set(entries.map((e) => e.path.toLowerCase())).size).toBe(8)
    expect(entries[2]?.path).toContain("Справка (2).pdf")
    expect(entries[4]?.path).toContain("(2)/")
  })
  it("sanitizes paths, reserved device names, control characters and trailing dots", () => {
    expect(safeArchiveName("../a\\b:foo?\u0000.pdf")).toBe(".._a_b_foo__.pdf")
    expect(safeArchiveName("CON.pdf")).toBe("_CON.pdf")
    expect(safeArchiveName("..")).toBe("_")
    expect(safeArchiveName("Файл. ")).toBe("Файл")
  })
  it("falls back to full application ID if applicant name is missing", () => {
    const app = { ...application(), applicant_details: {} }
    expect(applicationArchiveEntries([app], lots, true)[0]?.path).toContain(
      "app-1"
    )
  })
  it("keeps long Cyrillic names extractable and preserves PDF extension", () => {
    const result = safeArchiveName("Документ".repeat(100) + ".pdf")
    expect(new TextEncoder().encode(result).length).toBeLessThanOrEqual(220)
    expect(result.endsWith(".pdf")).toBe(true)
  })
  it("creates a readable UTF-8 ZIP with all original bytes, using protected URLs", async () => {
    const fetchFile = vi.fn<typeof fetch>().mockImplementation(
      async () =>
        new Response(new Uint8Array([37, 80, 68]), {
          headers: { "content-type": "application/pdf" },
        })
    )
    const progress = vi.fn<(done: number, total: number) => void>()
    const entries = applicationArchiveEntries([application()], lots, true)
    const blob = await createApplicationArchive(
      entries,
      progress,
      new AbortController().signal,
      fetchFile
    )
    const unpacked = unzipSync(new Uint8Array(await blob.arrayBuffer()))
    expect(Object.keys(unpacked)).toEqual(entries.map((entry) => entry.path))
    expect(unpacked[entries[1]!.path]).toEqual(new Uint8Array([37, 80, 68]))
    expect(fetchFile).toHaveBeenCalledWith(
      entries[1]?.url,
      expect.objectContaining({
        credentials: "same-origin",
        cache: "no-store",
        redirect: "error",
      })
    )
    expect(progress).toHaveBeenLastCalledWith(1, 1)
  })
  it.each([401, 403, 404, 500])(
    "refuses to create a partial ZIP on HTTP %i",
    async (status) => {
      const fetchFile = vi
        .fn<typeof fetch>()
        .mockResolvedValueOnce(
          new Response("pdf", {
            headers: { "content-type": "application/pdf" },
          })
        )
        .mockResolvedValueOnce(new Response("denied", { status }))
      const app = application()
      app.files.push({ ...file, id: "two" })
      await expect(
        createApplicationArchive(
          applicationArchiveEntries([app], lots, false),
          vi.fn(),
          new AbortController().signal,
          fetchFile
        )
      ).rejects.toThrow("Архив не сохранён")
    }
  )
  it("rejects an HTML login page instead of packing it as a PDF", async () => {
    await expect(
      createApplicationArchive(
        applicationArchiveEntries([application()], lots, false),
        vi.fn(),
        new AbortController().signal,
        vi
          .fn<typeof fetch>()
          .mockResolvedValue(
            new Response("login", { headers: { "content-type": "text/html" } })
          )
      )
    ).rejects.toThrow("Архив не сохранён")
  })
  it("rejects empty and oversized packages before fetching", async () => {
    const fetchFile = vi.fn<typeof fetch>()
    await expect(
      createApplicationArchive(
        [],
        vi.fn(),
        new AbortController().signal,
        fetchFile
      )
    ).rejects.toThrow(m.application_archive_empty())
    await expect(
      createApplicationArchive(
        [{ path: "big.pdf", url: "/file", size: ARCHIVE_MAX_BYTES + 1 }],
        vi.fn(),
        new AbortController().signal,
        fetchFile
      )
    ).rejects.toThrow(m.application_archive_large())
    expect(fetchFile).not.toHaveBeenCalled()
  })
  it("stops before fetching when cancelled", async () => {
    const controller = new AbortController()
    controller.abort()
    const fetchFile = vi.fn<typeof fetch>()
    await expect(
      createApplicationArchive(
        applicationArchiveEntries([application()], lots, false),
        vi.fn(),
        controller.signal,
        fetchFile
      )
    ).rejects.toThrow(/abort/iu)
    expect(fetchFile).not.toHaveBeenCalled()
  })
})
