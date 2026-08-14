import { Outlet, createFileRoute, redirect } from "@tanstack/react-router"
import { PageShell } from "@/components/page-shell"

// Кабинет члена тендерной комиссии (М11, T19): декларация конфликта
// интересов (FR-1104) и личное голосование по заявкам (FR-1103).
export const Route = createFileRoute("/app/commission")({
  beforeLoad: ({ context }) => {
    if (!context.user.roles.includes("commission")) {
      throw redirect({ to: "/app" })
    }
  },
  component: CommissionLayout,
})

function CommissionLayout() {
  return (
    <PageShell>
      <Outlet />
    </PageShell>
  )
}
