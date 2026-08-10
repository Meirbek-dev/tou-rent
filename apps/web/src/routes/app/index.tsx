import { Link, createFileRoute, redirect } from "@tanstack/react-router"
import { m } from "#/paraglide/messages"
import { MyDeadlines } from "@/components/my-deadlines"
import { CABINET_PATHS, cabinetLabel } from "@/lib/auth"

// Единственная роль - сразу в свой кабинет; несколько - выбор (ТЗ § 8).
export const Route = createFileRoute("/app/")({
  beforeLoad: ({ context }) => {
    const cabinets = context.user.roles.filter((role) => role in CABINET_PATHS)
    const only = cabinets.length === 1 ? cabinets[0] : undefined
    if (only !== undefined) {
      throw redirect({ to: CABINET_PATHS[only] })
    }
  },
  component: AppHome,
})

function AppHome() {
  const { user } = Route.useRouteContext()
  const cabinets = user.roles.filter((role) => role in CABINET_PATHS)

  return (
    <main className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-6 py-10">
      <h1 className="font-heading text-2xl font-semibold">
        {m.app_dashboard_title()}
      </h1>
      <ul className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {cabinets.map((role) => (
          <li key={role}>
            <Link
              to={CABINET_PATHS[role]}
              className="block rounded-lg border p-4 font-medium transition-colors hover:bg-muted/50"
            >
              {cabinetLabel(role)}
            </Link>
          </li>
        ))}
      </ul>
      <MyDeadlines />
    </main>
  )
}
