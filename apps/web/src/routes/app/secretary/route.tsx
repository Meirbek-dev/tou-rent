import { Outlet, createFileRoute, redirect } from "@tanstack/react-router"
import { PageShell } from "@/components/page-shell"

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
    <PageShell>
      <Outlet />
    </PageShell>
  )
}
