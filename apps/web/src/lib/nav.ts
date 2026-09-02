import { m } from "#/paraglide/messages"
import {
  BellIcon,
  BuildingIcon,
  CalculatorIcon,
  ClipboardListIcon,
  FileSignatureIcon,
  GavelIcon,
  LandmarkIcon,
  LayoutDashboardIcon,
  MapIcon,
  ScrollTextIcon,
  TableIcon,
  TrendingUpIcon,
} from "lucide-react"

import type { LucideIcon } from "lucide-react"

/**
 * Карта навигации кабинетов - один список на весь интерфейс.
 *
 * До этого разделы были рассыпаны по трем местам: роли-кабинеты в шапке
 * `routes/app/route.tsx`, шесть разделов организатора - в его макете, а
 * отчетность - отдельной ссылкой с собственным условием по ролям. Боковая
 * навигация, командная палитра и крошки обязаны показывать одно и то же
 * дерево, поэтому дерево здесь одно.
 *
 * `to` - строка: у `Link` из TanStack Router строковый адрес допустим,
 * а объявлять здесь литеральный союз всех 33 маршрутов значило бы держать
 * его в синхроне вручную.
 */
export type NavEntry = {
  to: string
  /** Ключ Paraglide, а не готовая строка: подпись меняется вместе с локалью */
  label: () => string
  icon: LucideIcon
  /** Индекс кабинета совпадает по префиксу со всеми его страницами */
  exact?: boolean
}

/**
 * Роли, которым доступен хотя бы один реестр отчетности (арх. § 9).
 *
 * `admin` здесь был, а прав на реестры у него нет: `GET /api/v1/reports` под
 * администратором отдает пустой список (организатор - 2 реестра, финансы - 2,
 * правление - 1). Пункт меню вел прямиком на «доступ закрыт» - меню обещало
 * то, чего сервер не дает.
 */
export const REPORT_ROLES = ["organizer", "finance", "board"]

/** Общая для всех ролей группа: обзор, уведомления, отчетность. */
export const WORKSPACE_NAV: NavEntry[] = [
  {
    to: "/app",
    label: m.app_dashboard_title,
    icon: LayoutDashboardIcon,
    exact: true,
  },
  { to: "/app/notifications", label: m.notif_title, icon: BellIcon },
]

/** Отчетность видна не всем - отсюда отдельным пунктом (арх. § 9). */
export const REPORTS_NAV: NavEntry = {
  to: "/app/reports",
  label: m.reports_title,
  icon: TableIcon,
}

/**
 * Разделы кабинета по роли. Порядок - порядок работы: сперва то, с чего
 * начинают день, потом справочное.
 */
export const ROLE_NAV: Record<string, NavEntry[]> = {
  organizer: [
    // Обзор и реестр объектов разъехались: до сих пор индексом кабинета была
    // таблица объектов, и «что сегодня горит» узнавалось обходом разделов
    {
      to: "/app/organizer",
      label: m.nav_overview,
      icon: ClipboardListIcon,
      exact: true,
    },
    {
      to: "/app/organizer/objects",
      label: m.org_nav_objects,
      icon: BuildingIcon,
    },
    { to: "/app/organizer/tenders", label: m.org_nav_tenders, icon: GavelIcon },
    {
      to: "/app/organizer/special",
      label: m.org_nav_special,
      icon: ScrollTextIcon,
    },
    {
      to: "/app/organizer/investment",
      label: m.org_nav_investment,
      icon: TrendingUpIcon,
    },
    { to: "/app/organizer/land", label: m.org_nav_land, icon: MapIcon },
    {
      to: "/app/organizer/calculator",
      label: m.org_nav_calculator,
      icon: CalculatorIcon,
    },
  ],
  participant: [
    {
      to: "/app/participant",
      label: m.nav_overview,
      icon: ClipboardListIcon,
      exact: true,
    },
    {
      to: "/app/participant/contracts",
      label: m.my_contracts_title,
      icon: FileSignatureIcon,
    },
  ],
  // Кабинеты об одной странице: группа остается - она называет роль,
  // в которой пользователь сейчас работает, и это единственное место,
  // где несколько ролей одного человека видны сразу
  secretary: [
    {
      to: "/app/secretary",
      label: m.nav_overview,
      icon: ClipboardListIcon,
      exact: true,
    },
  ],
  commission: [
    {
      to: "/app/commission",
      label: m.nav_overview,
      icon: ClipboardListIcon,
      exact: true,
    },
  ],
  finance: [
    {
      to: "/app/finance",
      label: m.nav_overview,
      icon: LandmarkIcon,
      exact: true,
    },
  ],
  board: [
    {
      to: "/app/board",
      label: m.nav_overview,
      icon: ClipboardListIcon,
      exact: true,
    },
  ],
  admin: [
    {
      to: "/app/admin",
      label: m.nav_overview,
      icon: ClipboardListIcon,
      exact: true,
    },
  ],
}

export function roleNav(role: string): NavEntry[] {
  return ROLE_NAV[role] ?? []
}

/** Видна ли пользователю отчетность (арх. § 9). */
export function canSeeReports(roles: readonly string[]): boolean {
  return roles.some((role) => REPORT_ROLES.includes(role))
}
