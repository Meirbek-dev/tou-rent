import { Outlet, createFileRoute, redirect } from "@tanstack/react-router"
import { m } from "#/paraglide/messages"

// Кабинет секретаря комиссии (Т8: журнал; Т9: вскрытие и допуск).
export const Route = createFileRoute("/app/secretary")({
  beforeLoad: ({ context }) => {
    if (!context.user.roles.includes("secretary")) {
      throw redirect({ to: "/app" })
    }
  },
  component: SecretaryLayout,
})

function SecretaryLayout() {
  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-6 py-8">
      <h1 className="font-heading text-2xl font-semibold">
        {m.cabinet_secretary()}
      </h1>
      <Outlet />
    </div>
  )
}
