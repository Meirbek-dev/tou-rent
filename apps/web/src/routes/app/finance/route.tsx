import { Outlet, createFileRoute, redirect } from "@tanstack/react-router"
import { PageShell } from "@/components/page-shell"

// Кабинет департамента финансов (М10, T21): подтверждение взносов вручную
// (FR-405) и депозитная книга (FR-1001–1004).
export const Route = createFileRoute("/app/finance")({
  beforeLoad: ({ context }) => {
    if (!context.user.roles.includes("finance")) {
      throw redirect({ to: "/app" })
    }
  },
  component: FinanceLayout,
})

function FinanceLayout() {
  return (
    <PageShell>
      <Outlet />
    </PageShell>
  )
}
