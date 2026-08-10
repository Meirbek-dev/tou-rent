import { Outlet, createFileRoute, redirect } from "@tanstack/react-router"
import { m } from "#/paraglide/messages"

// Кабинет участника (Т8): доступ - только роль participant (INV-POL-01).
export const Route = createFileRoute("/app/participant")({
  beforeLoad: ({ context }) => {
    if (!context.user.roles.includes("participant")) {
      throw redirect({ to: "/app" })
    }
  },
  component: ParticipantLayout,
})

function ParticipantLayout() {
  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-6 py-8">
      <h1 className="font-heading text-2xl font-semibold">
        {m.cabinet_participant()}
      </h1>
      <Outlet />
    </div>
  )
}
