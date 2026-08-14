import { MenuIcon } from "lucide-react"
import { Link } from "@tanstack/react-router"

import { m } from "#/paraglide/messages"
import { AppLogo } from "@/components/app-logo"
import { ThemeToggle } from "@/components/theme-toggle"
import { buttonVariants } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import LocaleSwitcher from "@/components/locale-switcher"

/** Разделы портала: один список на широкую шапку и на раскрывающееся меню. */
const SECTIONS = [
  { to: "/tenders", label: m.nav_tenders },
  { to: "/objects", label: m.nav_objects },
  // FR-1801 (п. 104): характеристики земельных участков
  { to: "/land-plots", label: m.nav_land_plots },
  // FR-1403 (п. 90, 92, 97): публикации особого порядка
  { to: "/special-orders", label: m.nav_special_orders },
  { to: "/how-to", label: m.nav_how_to },
] as const

/** Подчеркивание активного раздела: полоса у нижней кромки шапки. */
const ACTIVE_PROPS = {
  className:
    "text-foreground font-semibold after:absolute after:inset-x-3 after:bottom-0 after:h-0.5 after:rounded-full after:bg-primary",
  "aria-current": "page",
} as const

export function SiteHeader() {
  return (
    <header className="sticky top-0 z-30 border-b bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/80">
      <div className="relative mx-auto flex h-16 w-full max-w-6xl items-center gap-2 px-4 sm:px-6">
        <Link to="/" className="rounded-md">
          <AppLogo />
        </Link>

        {/* Пять русских подписей в одну строку помещаются начиная с xl;
            ниже - раскрывающееся меню на <details>, работает без JS */}
        <nav
          aria-label={m.nav_primary()}
          className="ml-4 hidden items-center xl:flex"
        >
          {SECTIONS.map((section) => (
            <Link
              key={section.to}
              to={section.to}
              className="relative inline-flex h-16 items-center px-3 text-sm font-medium text-muted-foreground -outline-offset-2 transition-colors hover:text-foreground"
              activeProps={ACTIVE_PROPS}
            >
              {section.label()}
            </Link>
          ))}
        </nav>

        <div className="ml-auto flex shrink-0 items-center gap-1">
          <ThemeToggle />
          <LocaleSwitcher />
          <Link
            to="/auth/login"
            className={cn(buttonVariants({ variant: "outline", size: "sm" }))}
          >
            {m.sign_in()}
          </Link>

          <details className="group xl:hidden">
            <summary
              aria-label={m.nav_menu()}
              className={cn(
                buttonVariants({ variant: "ghost", size: "icon" }),
                // Главный орган навигации на телефоне: 44 px пальцем
                // (SC 2.5.8). Правило `pointer: coarse` в globals.css
                // иконочные размеры не трогает, поэтому размер задан здесь
                "size-11 list-none [&::-webkit-details-marker]:hidden"
              )}
            >
              <MenuIcon aria-hidden="true" />
            </summary>
            <nav
              aria-label={m.nav_primary()}
              className="absolute inset-x-0 top-16 z-30 border-b bg-background p-2 shadow-md"
            >
              <ul className="mx-auto flex w-full max-w-6xl flex-col px-2 sm:px-4">
                {SECTIONS.map((section) => (
                  <li key={section.to}>
                    <Link
                      to={section.to}
                      className="relative flex min-h-11 items-center rounded-lg px-3 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                      activeProps={{
                        className: "text-foreground font-semibold bg-muted",
                        "aria-current": "page",
                      }}
                    >
                      {section.label()}
                    </Link>
                  </li>
                ))}
              </ul>
            </nav>
          </details>
        </div>
      </div>
    </header>
  )
}
