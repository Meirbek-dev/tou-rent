import { Link, useRouterState } from "@tanstack/react-router"

import { m } from "#/paraglide/messages"
import { deLocalizeHref } from "#/paraglide/runtime"
import { AppLogo } from "@/components/app-logo"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarRail,
  useSidebar,
} from "@/components/ui/sidebar"
import { cabinetLabel, userCabinets } from "@/lib/auth"
import { REPORTS_NAV, WORKSPACE_NAV, canSeeReports, roleNav } from "@/lib/nav"
import { useQueueCounts } from "@/lib/queues"
import { ChevronsUpDownIcon, LogOutIcon } from "lucide-react"

import type { NavEntry } from "@/lib/nav"
import type { User } from "@/lib/auth"

/**
 * Боковая навигация кабинетов.
 *
 * Разделы сгруппированы по ролям, потому что роль здесь - не признак
 * пользователя, а рабочее место: один человек бывает и организатором, и
 * членом комиссии, и это разные обязанности с разными сроками. Прежняя
 * горизонтальная лента ссылок это различие стирала - семь кабинетов и шесть
 * разделов организатора лежали в одной строке с прокруткой.
 *
 * `collapsible="icon"`: реестры и таблицы кабинетов широкие, и место под них
 * должно освобождаться без потери навигации.
 */
export function AppSidebar({
  user,
  onSignOut,
}: {
  user: User
  onSignOut: () => void
}) {
  const cabinets = userCabinets(user.roles)
  const counts = useQueueCounts(user.roles)

  const workspace: NavEntry[] = canSeeReports(user.roles)
    ? [...WORKSPACE_NAV, REPORTS_NAV]
    : WORKSPACE_NAV

  return (
    <Sidebar collapsible="icon">
      {/* Знак портала - в свернутом виде прячется целиком: он горизонтальный
          и в колонку 3rem не кадрируется без потери слова */}
      <SidebarHeader className="group-data-[collapsible=icon]:hidden">
        <Link
          to="/app"
          className="flex items-center rounded-lg px-1 outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
        >
          <AppLogo />
        </Link>
      </SidebarHeader>

      <SidebarContent>
        <NavGroup
          label={m.nav_workspace()}
          entries={workspace}
          counts={counts}
        />
        {cabinets.map(({ role }) => (
          <NavGroup
            key={role}
            label={cabinetLabel(role)}
            entries={roleNav(role)}
            counts={counts}
          />
        ))}
      </SidebarContent>

      <SidebarFooter>
        <UserMenu user={user} onSignOut={onSignOut} />
      </SidebarFooter>
      <SidebarRail />
    </Sidebar>
  )
}

/**
 * Активен ли пункт. Считается здесь, а не через `activeProps`, потому что
 * подсветку рисует сам `SidebarMenuButton` по признаку `isActive`, а не
 * класс на ссылке; заодно отсюда же берется `aria-current`.
 */
function isEntryActive(entry: NavEntry, pathname: string): boolean {
  if (entry.exact) return pathname === entry.to
  return pathname === entry.to || pathname.startsWith(`${entry.to}/`)
}

function NavGroup({
  label,
  entries,
  counts,
}: {
  label: string
  entries: NavEntry[]
  counts: Record<string, number>
}) {
  const { isMobile, setOpenMobile } = useSidebar()
  // Локаль живет в пути (/kk/app/..., /en/app/...). Роутер отдает уже
  // разлокализованный адрес, но сравнение с `to` из карты навигации обязано
  // выдерживать и обратное: иначе в казахской версии не подсвечивался бы
  // ни один пункт. Повторная разлокализация ничего не меняет.
  const pathname = useRouterState({
    select: (state) => deLocalizeHref(state.location.pathname),
  })

  if (entries.length === 0) return null

  return (
    <SidebarGroup>
      <SidebarGroupLabel>{label}</SidebarGroupLabel>
      <SidebarGroupContent>
        <SidebarMenu>
          {entries.map((entry) => {
            const Icon = entry.icon
            const count = counts[entry.to]
            const active = isEntryActive(entry, pathname)

            return (
              <SidebarMenuItem key={entry.to}>
                <SidebarMenuButton
                  isActive={active}
                  tooltip={entry.label()}
                  render={
                    <Link
                      to={entry.to}
                      aria-current={active ? "page" : undefined}
                      // Мобильная навигация - шторка: без закрытия она
                      // остается поверх страницы, на которую только что ушли
                      onClick={() => {
                        if (isMobile) setOpenMobile(false)
                      }}
                    />
                  }
                >
                  <Icon aria-hidden="true" />
                  <span>{entry.label()}</span>
                </SidebarMenuButton>
                {count !== undefined && (
                  <SidebarMenuBadge className="tabular-nums">
                    {count}
                  </SidebarMenuBadge>
                )}
              </SidebarMenuItem>
            )
          })}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  )
}

function UserMenu({ user, onSignOut }: { user: User; onSignOut: () => void }) {
  const roles = userCabinets(user.roles)
    .map(({ role }) => cabinetLabel(role))
    .join(", ")

  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <SidebarMenuButton size="lg" aria-label={m.nav_account()} />
            }
          >
            <span
              aria-hidden="true"
              className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-sidebar-accent text-xs font-semibold"
            >
              {initials(user.full_name)}
            </span>
            <span className="flex min-w-0 flex-1 flex-col text-left leading-tight">
              <span className="truncate text-sm font-medium">
                {user.full_name}
              </span>
              <span className="truncate text-xs text-sidebar-foreground/70">
                {roles}
              </span>
            </span>
            <ChevronsUpDownIcon aria-hidden="true" className="ml-auto" />
          </DropdownMenuTrigger>
          <DropdownMenuContent side="top" align="start" className="min-w-56">
            {/* Не DropdownMenuLabel: это Menu.GroupLabel, он требует
                Menu.Group вокруг и без него роняет меню. Здесь подпись
                описывает не группу пунктов, а самого пользователя */}
            <div className="flex flex-col gap-0.5 px-1.5 py-1">
              <span className="truncate text-sm font-medium">
                {user.full_name}
              </span>
              <span className="truncate text-xs text-muted-foreground">
                {roles}
              </span>
            </div>
            <DropdownMenuSeparator />
            <DropdownMenuItem onClick={onSignOut}>
              <LogOutIcon aria-hidden="true" />
              {m.sign_out()}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  )
}

/** Две буквы вместо аватара: фотографий у учетных записей нет. */
function initials(fullName: string): string {
  const parts = fullName.trim().split(/\s+/).filter(Boolean)
  const first = parts[0]?.charAt(0) ?? ""
  const second = parts[1]?.charAt(0) ?? ""
  return (first + second).toUpperCase() || "?"
}
