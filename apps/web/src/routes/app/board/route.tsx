import { Outlet, createFileRoute, redirect } from "@tanstack/react-router"
import { m } from "#/paraglide/messages"

// Кабинет Правления (T34): доступ - только роль board (INV-POL-01).
export const Route = createFileRoute("/app/board")({
  beforeLoad: ({ context }) => {
    if (!context.user.roles.includes("board")) {
      throw redirect({ to: "/app" })
    }
  },
  component: BoardLayout,
})

function BoardLayout() {
  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-6 py-8">
      <header>
        <h1 className="font-heading text-2xl font-semibold">
          {m.cabinet_board()}
        </h1>
      </header>
      <Outlet />
    </div>
  )
}
