import { Outlet, createFileRoute, redirect } from "@tanstack/react-router"
import { PageShell } from "@/components/page-shell"

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
    <PageShell>
      <Outlet />
    </PageShell>
  )
}
