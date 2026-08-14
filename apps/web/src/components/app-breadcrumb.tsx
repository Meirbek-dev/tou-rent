import { Fragment } from "react"
import { Link, useMatches } from "@tanstack/react-router"

import { m } from "#/paraglide/messages"
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb"

/**
 * Подписи крошек по идентификатору маршрута.
 *
 * Карта, а не `staticData` у каждого маршрута: подпись - это ключ Paraglide,
 * то есть функция, и протаскивать ее типом через все файлы маршрутов ради
 * трех уровней вложенности - работа, несоразмерная результату. Маршрут, для
 * которого подписи нет (страницы с `$id`), крошки не порождает вовсе:
 * лучше короткая цепочка, чем `«/app/organizer/tenders/$tenderId»` в строке
 * навигации. Детальные страницы получат свои подписи вместе с заголовками.
 */
const CRUMBS: Record<string, () => string> = {
  "/app": m.app_dashboard_title,
  "/app/notifications": m.notif_title,
  "/app/reports": m.reports_title,

  "/app/participant": m.cabinet_participant,
  "/app/participant/": m.nav_overview,
  "/app/participant/contracts": m.my_contracts_title,

  "/app/organizer": m.cabinet_organizer,
  "/app/organizer/": m.nav_overview,
  "/app/organizer/objects": m.org_nav_objects,
  "/app/organizer/tenders/": m.org_nav_tenders,
  "/app/organizer/tenders/new": m.tender_create_title,
  "/app/organizer/calculator": m.org_nav_calculator,
  "/app/organizer/special": m.org_nav_special,
  "/app/organizer/investment": m.org_nav_investment,
  "/app/organizer/land": m.org_nav_land,

  "/app/secretary": m.cabinet_secretary,
  "/app/secretary/": m.nav_overview,

  "/app/commission": m.cabinet_commission,
  "/app/commission/": m.nav_overview,

  "/app/finance": m.cabinet_finance,
  "/app/finance/": m.nav_overview,

  "/app/board": m.cabinet_board,
  "/app/board/": m.nav_overview,

  "/app/admin": m.cabinet_admin,
  "/app/admin/": m.nav_overview,
}

/** Где я нахожусь: цепочка «кабинет → страница» в шапке каркаса. */
export function AppBreadcrumb() {
  const matches = useMatches()

  const crumbs = matches.flatMap((match) => {
    const label = CRUMBS[match.routeId]
    if (label === undefined) return []
    return [{ id: match.id, to: match.pathname, label: label() }]
  })

  if (crumbs.length === 0) return null

  return (
    <Breadcrumb className="min-w-0">
      <BreadcrumbList className="flex-nowrap">
        {crumbs.map((crumb, index) =>
          index === crumbs.length - 1 ? (
            <BreadcrumbItem key={crumb.id} className="min-w-0">
              <BreadcrumbPage className="truncate">
                {crumb.label}
              </BreadcrumbPage>
            </BreadcrumbItem>
          ) : (
            // Предки на узком экране прячутся вместе со своими разделителями:
            // три крошки не помещаются рядом с колокольчиком и сроками, а
            // текущая страница нужнее пути к ней
            <Fragment key={crumb.id}>
              <BreadcrumbItem className="hidden min-w-0 sm:inline-flex">
                <BreadcrumbLink
                  className="truncate"
                  render={<Link to={crumb.to} />}
                >
                  {crumb.label}
                </BreadcrumbLink>
              </BreadcrumbItem>
              <BreadcrumbSeparator className="hidden sm:flex" />
            </Fragment>
          )
        )}
      </BreadcrumbList>
    </Breadcrumb>
  )
}
