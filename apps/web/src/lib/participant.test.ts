import { QueryClient } from "@tanstack/react-query"
import { describe, expect, it, vi } from "vite-plus/test"

const { get } = vi.hoisted(() => ({
  get: vi.fn<() => Promise<{ data: ApplicationDto[] }>>(),
}))
vi.mock("@/lib/api", () => ({ api: { GET: get } }))
vi.mock("@/lib/server-label", () => ({ serverLabel: () => "" }))

import { loadOwnApplication, myApplicationsQuery } from "./participant"
import type { ApplicationDto } from "./participant"

function application(
  id: string,
  status: ApplicationDto["status"]
): ApplicationDto {
  return {
    id,
    status,
    tender_id: "tender",
    lot_id: "lot",
    participant_id: "participant",
    applicant_kind: "individual",
    applicant_details: {},
    files: [],
    qualification: null,
    submitted_at: "2026-09-08T10:00:00Z",
    withdrawn_at: status === "withdrawn" ? "2026-09-08T11:00:00Z" : null,
    rejection_reason: null,
    package_complete: false,
    price_amount: null,
  }
}

describe("opening an application after submission", () => {
  it("loads the new application even when a fresh cached list only contains the withdrawn one", async () => {
    const client = new QueryClient({
      defaultOptions: { queries: { staleTime: Infinity, retry: false } },
    })
    const withdrawn = application("old", "withdrawn")
    const submitted = application("new", "submitted")
    client.setQueryData(myApplicationsQuery.queryKey, [withdrawn])
    get.mockResolvedValueOnce({ data: [submitted, withdrawn] })

    expect(await loadOwnApplication(client, submitted.id)).toEqual(submitted)
    expect(client.getQueryData(myApplicationsQuery.queryKey)).toEqual([
      submitted,
      withdrawn,
    ])
    client.clear()
  })

  it("does not report a missing application until the server confirms it", async () => {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    })
    client.setQueryData(myApplicationsQuery.queryKey, [
      application("removed", "submitted"),
    ])
    get.mockResolvedValueOnce({ data: [] })

    expect(await loadOwnApplication(client, "removed")).toBeUndefined()
    client.clear()
  })

  it("propagates a request failure instead of treating it as a missing application", async () => {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    })
    client.setQueryData(myApplicationsQuery.queryKey, [])
    const error = new Error("connection failed")
    get.mockRejectedValueOnce(error)

    await expect(loadOwnApplication(client, "new")).rejects.toBe(error)
    client.clear()
  })
})
