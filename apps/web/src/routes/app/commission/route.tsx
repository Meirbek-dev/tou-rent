import { Outlet, createFileRoute, redirect } from "@tanstack/react-router"
import { m } from "#/paraglide/messages"

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
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-6 py-8">
      <h1 className="font-heading text-2xl font-semibold">
        {m.cabinet_commission()}
      </h1>
      <Outlet />
    </div>
  )
}
