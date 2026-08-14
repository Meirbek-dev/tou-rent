import { Outlet, createFileRoute, redirect } from "@tanstack/react-router"
import { PageShell } from "@/components/page-shell"

// Кабинет организатора (Т7): доступ - только роль organizer (INV-POL-01).
export const Route = createFileRoute("/app/organizer")({
  beforeLoad: ({ context }) => {
    if (!context.user.roles.includes("organizer")) {
      throw redirect({ to: "/app" })
    }
  },
  component: OrganizerLayout,
})

// Шесть разделов кабинета переехали в боковую навигацию (`lib/nav.ts`):
// вторая полоса ссылок под шапкой повторяла бы ее один в один.
function OrganizerLayout() {
  return (
    <PageShell>
      <Outlet />
    </PageShell>
  )
}
