import { m } from "#/paraglide/messages"
import { formatTenge } from "./format"

import type { AuctionDto, CircleParticipantDto } from "@/lib/auctions"

export function auctionParticipantLabel(
  participant: Pick<
    CircleParticipantDto,
    "application_id" | "status" | "initial_price"
  >,
  auction: Pick<
    AuctionDto,
    "status" | "winner_application_id" | "runner_up_application_id"
  >
): string {
  if (auction.status === "finished") {
    if (participant.application_id === auction.winner_application_id)
      return m.auction_state_winner()
    if (participant.application_id === auction.runner_up_application_id)
      return m.auction_state_runner_up()
    if (participant.status === "active") return m.auction_state_finished()
  }
  switch (participant.status) {
    case "passed":
      return m.auction_state_passed()
    case "absent":
      return m.auction_state_absent({
        amount: formatTenge(participant.initial_price),
      })
    default:
      return m.auction_state_active()
  }
}
