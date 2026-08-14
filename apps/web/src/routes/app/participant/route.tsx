import { Outlet, createFileRoute, redirect } from "@tanstack/react-router"
import { PageShell } from "@/components/page-shell"

// Кабинет участника (Т8): доступ - только роль participant (INV-POL-01).
export const Route = createFileRoute("/app/participant")({
  beforeLoad: ({ context }) => {
    if (!context.user.roles.includes("participant")) {
      throw redirect({ to: "/app" })
    }
  },
  component: ParticipantLayout,
})

// Имя кабинета теперь называет группа боковой навигации, а `h1` принадлежит
// странице: макет отвечает только за полосу содержимого.
function ParticipantLayout() {
  return (
    <PageShell>
      <Outlet />
    </PageShell>
  )
}
