import {
  Link,
  Outlet,
  createFileRoute,
  redirect,
  useNavigate,
} from "@tanstack/react-router"
import { useQueryClient } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { AppLogo } from "@/components/app-logo"
import LocaleSwitcher from "@/components/locale-switcher"
import { NotificationBell } from "@/components/notification-bell"
import { ThemeToggle } from "@/components/theme-toggle"
import { Button } from "@/components/ui/button"
import { api } from "@/lib/api"
import { CABINET_PATHS, cabinetLabel, meQuery } from "@/lib/auth"

// Каркас кабинетов (ТЗ § 8). Кабинеты - client-only (ssr: false):
// NFR-04 требует работы без JS только от публичного портала, а клиентский
// рендер избавляет SSR-загрузчики от проброса сессионных cookie.
export const Route = createFileRoute("/app")({
  ssr: false,
  beforeLoad: async ({ context }) => {
    const user = await context.queryClient.ensureQueryData(meQuery)
    if (user === null) {
      throw redirect({ to: "/auth/login" }) // FR-1501
    }
    return { user }
  },
  component: AppLayout,
})

/** Роли, которым доступен хотя бы один реестр отчетности (арх. § 9). */
const REPORT_ROLES = ["organizer", "finance", "board", "admin"]

function AppLayout() {
  const { user } = Route.useRouteContext()
  const navigate = useNavigate()
  const queryClient = useQueryClient()

  const logout = async () => {
    // Сессию, открытую внешним провайдером, завершает он же (FR-1502):
    // иначе «выйти» на общем компьютере ничего не значит - следующий вход
    // пройдет молча по живой сессии провайдера
    if (user.external_session) {
      window.location.href = "/api/v1/auth/oidc/logout"
      return
    }
    await api.POST("/api/v1/auth/logout")
    queryClient.setQueryData(meQuery.queryKey, null)
    await navigate({ to: "/" })
  }

  return (
    <div className="flex min-h-svh flex-col">
      <header className="border-b">
        <div className="mx-auto flex w-full max-w-6xl flex-wrap items-center justify-between gap-3 px-4 py-3 sm:px-6 lg:flex-nowrap lg:gap-4">
          <div className="flex min-w-0 flex-1 items-center gap-1">
            <Link to="/app">
              <AppLogo />
            </Link>
            <nav className="ml-4 flex min-w-0 items-center gap-1 overflow-x-auto">
              {user.roles
                .filter((role) => role in CABINET_PATHS)
                .map((role) => (
                  <Link
                    key={role}
                    to={CABINET_PATHS[role]}
                    className="rounded-md px-3 py-1.5 text-sm hover:bg-muted"
                  >
                    {cabinetLabel(role)}
                  </Link>
                ))}
              {/* Отчетность (арх. § 9): реестры ведут организатор,
                  финансы, Правление и админ */}
              {user.roles.some((role) => REPORT_ROLES.includes(role)) && (
                <Link
                  to="/app/reports"
                  className="rounded-md px-3 py-1.5 text-sm hover:bg-muted"
                >
                  {m.reports_title()}
                </Link>
              )}
            </nav>
          </div>
          <div className="flex shrink-0 items-center gap-2 sm:gap-3">
            <span className="hidden max-w-40 truncate text-sm text-muted-foreground md:inline">
              {user.full_name}
            </span>
            <NotificationBell />
            <ThemeToggle />
            <LocaleSwitcher />
            <Button variant="outline" size="sm" onClick={logout}>
              {m.sign_out()}
            </Button>
          </div>
        </div>
      </header>
      <Outlet />
    </div>
  )
}
