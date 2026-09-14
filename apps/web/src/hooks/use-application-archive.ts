import { useEffect, useRef, useState } from "react"
import { m } from "#/paraglide/messages"
import {
  applicationArchiveEntries,
  createApplicationArchive,
  safeArchiveName,
} from "../lib/application-archive"
import type { ApplicationDto } from "../lib/participant"
import type { LotDto } from "../lib/api"

export function useApplicationArchive(
  tenderId: string,
  applications: ApplicationDto[],
  lots: LotDto[],
  sealed: boolean
) {
  const active = useRef<AbortController | null>(null)
  const [busy, setBusy] = useState(false)
  const [currentId, setCurrentId] = useState<string | undefined>()
  const [status, setStatus] = useState("")
  const [error, setError] = useState("")
  useEffect(() => () => active.current?.abort(), [])

  async function download(applicationId?: string) {
    if (sealed || active.current !== null) return
    const controller = new AbortController()
    active.current = controller
    setBusy(true)
    setCurrentId(applicationId)
    setError("")
    setStatus(m.application_archive_preparing())
    try {
      const selected =
        applicationId === undefined
          ? applications
          : applications.filter((a) => a.id === applicationId)
      const entries = applicationArchiveEntries(
        selected,
        lots,
        applicationId === undefined
      )
      const blob = await createApplicationArchive(
        entries,
        (done, total) => {
          setStatus(m.application_archive_progress({ done, total }))
        },
        controller.signal
      )
      controller.signal.throwIfAborted()
      const url = URL.createObjectURL(blob)
      const link = document.createElement("a")
      link.href = url
      link.download =
        safeArchiveName(
          m.application_archive_filename({ id: applicationId ?? tenderId })
        ) + ".zip"
      document.body.append(link)
      try {
        link.click()
      } finally {
        link.remove()
        window.setTimeout(() => URL.revokeObjectURL(url), 60_000)
      }
      setStatus(m.application_archive_ready())
    } catch (cause) {
      if (!controller.signal.aborted) {
        setError(
          cause instanceof Error
            ? cause.message
            : m.application_archive_failed()
        )
        setStatus("")
      }
    } finally {
      active.current = null
      if (!controller.signal.aborted) setBusy(false)
    }
  }
  return { download, busy, currentId, status, error }
}
