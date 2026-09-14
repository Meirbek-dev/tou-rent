import { m } from "#/paraglide/messages"
import { documentKindLabel } from "./application-documents"
import type { ApplicationDto, ApplicationFile } from "./participant"
import type { LotDto } from "./api"

export type ArchiveEntry = { path: string; url?: string; size: number }
// Browser memory guard, not a restriction on the stored application package.
export const ARCHIVE_MAX_BYTES = 256 * 1024 * 1024

function fitName(value: string): string {
  const encoder = new TextEncoder()
  const extension = /\.[a-z0-9]{1,10}$/iu.exec(value)?.[0] ?? ""
  let stem = extension ? value.slice(0, -extension.length) : value
  // Leave room for collision suffixes and filesystem limits (UTF-8 bytes).
  while (encoder.encode(stem + extension).length > 220) {
    stem = Array.from(stem).slice(0, -1).join("")
  }
  return stem + extension
}

export function safeArchiveName(value: string): string {
  const cleaned = Array.from(value.normalize("NFC"), (char) =>
    char.charCodeAt(0) < 32 || '<>:"/\\|?*'.includes(char) ? "_" : char
  )
    .join("")
    .trim()
    .replace(/[. ]+$/u, "")
  return cleaned === "" ||
    /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/iu.test(cleaned)
    ? fitName(`_${cleaned}`)
    : fitName(cleaned)
}

function uniqueName(value: string, used: Set<string>, file: boolean): string {
  const name = safeArchiveName(value)
  const dot = file ? name.lastIndexOf(".") : -1
  const stem = dot > 0 ? name.slice(0, dot) : name
  const ext = dot > 0 ? name.slice(dot) : ""
  let candidate = name
  let index = 2
  while (used.has(candidate.toLocaleLowerCase("en"))) {
    candidate = `${stem} (${index++})${ext}`
  }
  used.add(candidate.toLocaleLowerCase("en"))
  return candidate
}

export function applicationDocumentLabel(
  file: Pick<ApplicationFile, "document_kind" | "filename">
): string {
  return `${documentKindLabel(file.document_kind)} — ${file.filename}`
}

export function applicationArchiveEntries(
  applications: readonly ApplicationDto[],
  lots: readonly Pick<LotDto, "id" | "seq">[],
  folders: boolean
): ArchiveEntry[] {
  const usedFolders = new Set<string>()
  return applications.flatMap((application) => {
    const name: unknown = application.applicant_details["name"]
    const lot = lots.find((item) => item.id === application.lot_id)
    const folder = uniqueName(
      m.application_archive_folder({
        lot: String(lot?.seq ?? application.lot_id),
        name:
          typeof name === "string" && name.trim() !== ""
            ? name
            : application.id,
      }),
      usedFolders,
      false
    )
    const usedFiles = new Set<string>()
    const prefix = folders ? `${folder}/` : ""
    return [
      ...(folders ? [{ path: prefix, size: 0 }] : []),
      ...application.files.map((file) => ({
        path:
          prefix + uniqueName(applicationDocumentLabel(file), usedFiles, true),
        url: `/api/v1/applications/${encodeURIComponent(application.id)}/files/${encodeURIComponent(file.id)}`,
        size: file.size_bytes,
      })),
    ]
  })
}

/** Reuses the protected file endpoint; no bypass of opening, session or role checks. */
export async function createApplicationArchive(
  entries: readonly ArchiveEntry[],
  onProgress: (done: number, total: number) => void,
  signal: AbortSignal,
  fetchFile: typeof fetch = fetch
): Promise<Blob> {
  const total = entries.filter((entry) => entry.url !== undefined).length
  if (total === 0) throw new Error(m.application_archive_empty())
  if (entries.reduce((sum, entry) => sum + entry.size, 0) > ARCHIVE_MAX_BYTES) {
    throw new Error(m.application_archive_large())
  }
  const { Zip, ZipPassThrough } = await import("fflate")
  const chunks: BlobPart[] = []
  let failure: Error | undefined
  const archive = new Zip((error, data) => {
    if (error) failure = error
    else chunks.push(new Uint8Array(data))
  })
  let bytes = 0
  let done = 0
  onProgress(done, total)
  try {
    for (const entry of entries) {
      signal.throwIfAborted()
      const part = new ZipPassThrough(entry.path)
      archive.add(part)
      if (entry.url !== undefined) {
        const response = await fetchFile(entry.url, {
          credentials: "same-origin",
          cache: "no-store",
          redirect: "error",
          signal: AbortSignal.any([signal, AbortSignal.timeout(120_000)]),
        }).catch(() => {
          signal.throwIfAborted()
          throw new Error(
            m.application_archive_file_failed({ file: entry.path })
          )
        })
        if (
          !response.ok ||
          !response.body ||
          !/^(application\/pdf|application\/octet-stream)(;|$)/iu.test(
            response.headers.get("content-type") ?? ""
          )
        ) {
          await response.body?.cancel()
          throw new Error(
            m.application_archive_file_failed({ file: entry.path })
          )
        }
        const reader = response.body.getReader()
        try {
          while (true) {
            signal.throwIfAborted()
            const next = await reader.read()
            if (next.done) break
            bytes += next.value.byteLength
            if (bytes > ARCHIVE_MAX_BYTES)
              throw new Error(m.application_archive_large())
            part.push(next.value)
            if (failure) throw failure
          }
        } finally {
          await reader.cancel()
          reader.releaseLock()
        }
        onProgress(++done, total)
      }
      part.push(new Uint8Array(), true)
      if (failure) throw failure
    }
    archive.end()
    if (failure) throw failure
    return new Blob(chunks, { type: "application/zip" })
  } finally {
    archive.terminate()
  }
}
