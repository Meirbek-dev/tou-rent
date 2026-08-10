import { Link, Outlet, createFileRoute, redirect } from "@tanstack/react-router"
import { m } from "#/paraglide/messages"

// Кабинет организатора (Т7): доступ - только роль organizer (INV-POL-01).
export const Route = createFileRoute("/app/organizer")({
  beforeLoad: ({ context }) => {
    if (!context.user.roles.includes("organizer")) {
      throw redirect({ to: "/app" })
    }
  },
  component: OrganizerLayout,
})

function OrganizerLayout() {
  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-6 py-8">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="font-heading text-2xl font-semibold">
          {m.cabinet_organizer()}
        </h1>
        <nav className="flex items-center gap-1">
          <Link
            to="/app/organizer"
            activeOptions={{ exact: true }}
            activeProps={{ className: "bg-muted" }}
            className="rounded-md px-3 py-1.5 text-sm hover:bg-muted"
          >
            {m.org_nav_objects()}
          </Link>
          <Link
            to="/app/organizer/calculator"
            activeProps={{ className: "bg-muted" }}
            className="rounded-md px-3 py-1.5 text-sm hover:bg-muted"
          >
            {m.org_nav_calculator()}
          </Link>
          <Link
            to="/app/organizer/tenders"
            activeProps={{ className: "bg-muted" }}
            className="rounded-md px-3 py-1.5 text-sm hover:bg-muted"
          >
            {m.org_nav_tenders()}
          </Link>
          <Link
            to="/app/organizer/special"
            activeProps={{ className: "bg-muted" }}
            className="rounded-md px-3 py-1.5 text-sm hover:bg-muted"
          >
            {m.org_nav_special()}
          </Link>
          <Link
            to="/app/organizer/investment"
            activeProps={{ className: "bg-muted" }}
            className="rounded-md px-3 py-1.5 text-sm hover:bg-muted"
          >
            {m.org_nav_investment()}
          </Link>
          {/* FR-1801 (п. 104–107): земельные участки */}
          <Link
            to="/app/organizer/land"
            activeProps={{ className: "bg-muted" }}
            className="rounded-md px-3 py-1.5 text-sm hover:bg-muted"
          >
            {m.org_nav_land()}
          </Link>
        </nav>
      </header>
      <Outlet />
    </div>
  )
}
