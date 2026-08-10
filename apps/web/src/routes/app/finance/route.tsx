import { Outlet, createFileRoute, redirect } from "@tanstack/react-router"
import { m } from "#/paraglide/messages"

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
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-6 py-8">
      <h1 className="font-heading text-2xl font-semibold">
        {m.cabinet_finance()}
      </h1>
      <Outlet />
    </div>
  )
}
