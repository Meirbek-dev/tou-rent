import { describe, expect, it } from "vite-plus/test"
import { m } from "#/paraglide/messages"
import { auctionParticipantLabel } from "./auction-participant-label"

const participant = {
  application_id: "a",
  status: "active",
  initial_price: "100",
}
const auction = {
  status: "finished",
  winner_application_id: "a",
  runner_up_application_id: "b",
}

describe("auctionParticipantLabel", () => {
  it("shows the winner after completion", () => {
    expect(auctionParticipantLabel(participant, auction)).toBe(
      m.auction_state_winner()
    )
  })
  it("shows second place even after passing", () => {
    expect(
      auctionParticipantLabel(
        { ...participant, application_id: "b", status: "passed" },
        auction
      )
    ).toBe(m.auction_state_runner_up())
  })
  it("does not show active trading when no winner was determined", () => {
    expect(
      auctionParticipantLabel(participant, {
        ...auction,
        winner_application_id: null,
        runner_up_application_id: null,
      })
    ).toBe(m.auction_state_finished())
  })
  it("preserves live statuses while running", () => {
    expect(
      auctionParticipantLabel(participant, { ...auction, status: "running" })
    ).toBe(m.auction_state_active())
    expect(
      auctionParticipantLabel(
        { ...participant, status: "passed" },
        { ...auction, status: "running" }
      )
    ).toBe(m.auction_state_passed())
  })
  it("preserves passing and absence for other participants", () => {
    expect(
      auctionParticipantLabel(
        { ...participant, application_id: "c", status: "passed" },
        auction
      )
    ).toBe(m.auction_state_passed())
    expect(
      auctionParticipantLabel(
        { ...participant, application_id: "c", status: "absent" },
        auction
      )
    ).not.toBe(m.auction_state_active())
  })
})
