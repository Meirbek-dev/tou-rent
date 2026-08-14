import { createFileRoute } from "@tanstack/react-router"
import { m } from "#/paraglide/messages"
import { LandOrganizerPanel } from "@/components/land-panels"
import { PageHeader } from "@/components/page-header"
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
  head: () => ({ meta: [{ title: `${m.org_nav_land()} - ToU Rent` }] }),
  component: LandPage,
})

// Раздел кабинета - страница, а не голая панель: у нее должен быть свой `h1`
// (прежний жил в макете кабинета и назывался «Кабинет организатора»).
function LandPage() {
  return (
    <div className="flex flex-col gap-6">
      <PageHeader title={m.org_nav_land()} />
      <LandOrganizerPanel />
    </div>
  )
}
