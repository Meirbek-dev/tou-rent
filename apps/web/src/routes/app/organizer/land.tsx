import { createFileRoute } from "@tanstack/react-router"
import { LandOrganizerPanel } from "@/components/land-panels"
import {
  landApplicationsQuery,
  landPlotsQuery,
  landRefdataQuery,
} from "@/lib/land"

// FR-1801 (п. 104, 107): характеристики участков и их публикация,
// а по удовлетворенной заявке - договор с особыми условиями (INV-105).
export const Route = createFileRoute("/app/organizer/land")({
  loader: async ({ context }) => {
    await Promise.all([
      context.queryClient.ensureQueryData(landPlotsQuery),
      context.queryClient.ensureQueryData(landRefdataQuery),
      context.queryClient.ensureQueryData(landApplicationsQuery),
    ])
  },
  component: LandOrganizerPanel,
})
