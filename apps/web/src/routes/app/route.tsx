import {
  Outlet,
  createFileRoute,
  redirect,
  useNavigate,
} from "@tanstack/react-router"
import { useQueryClient } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { AppBreadcrumb } from "@/components/app-breadcrumb"
import { AppSidebar } from "@/components/app-sidebar"
import { CommandPalette } from "@/components/command-palette"
import { DeadlineChip } from "@/components/deadline-chip"
import LocaleSwitcher from "@/components/locale-switcher"
import { NotificationBell } from "@/components/notification-bell"
import { ThemeToggle } from "@/components/theme-toggle"
import {
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from "@/components/ui/sidebar"
import { Toaster } from "@/components/ui/toast"
import { api } from "@/lib/api"
import { meQuery } from "@/lib/auth"

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
    <SidebarProvider>
      <AppSidebar user={user} onSignOut={() => void logout()} />
      {/* min-w-0: без него широкая таблица кабинета растягивает колонку
          и уводит всю страницу в горизонтальную прокрутку */}
      <SidebarInset className="min-w-0">
        {/* Шапка вставки: где я нахожусь - слева, что требует внимания -
            справа. Ближайший срок Правил (FR-1702) стоит рядом с
            уведомлениями намеренно: до сих пор сроки были видны только внизу
            дашборда, то есть ровно на одном экране из тридцати трех */}
        <header className="sticky top-0 z-10 flex h-14 shrink-0 items-center gap-2 border-b bg-background/95 px-3 supports-backdrop-filter:bg-background/80 supports-backdrop-filter:backdrop-blur sm:px-4">
          <SidebarTrigger aria-label={m.nav_menu()} />
          <div className="min-w-0 flex-1">
            <AppBreadcrumb />
          </div>
          <div className="flex shrink-0 items-center gap-0.5 sm:gap-1">
            <CommandPalette roles={user.roles} />
            <DeadlineChip />
            <NotificationBell />
            <ThemeToggle />
            <LocaleSwitcher />
          </div>
        </header>
        {/* Ориентир <main> дает сам SidebarInset; внутренний блок - только
            якорь для skip-ссылки (#main), вторым <main> ему быть нельзя */}
        <div id="main" className="flex-1">
          <Outlet />
        </div>
      </SidebarInset>
      {/* Один менеджер тостов на все кабинеты; отправляют в него через
          `lib/toast.ts` */}
      <Toaster />
    </SidebarProvider>
  )
}
