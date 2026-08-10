import { Link } from "@tanstack/react-router"
import { m } from "#/paraglide/messages"
import { AppLogo } from "@/components/app-logo"
import { ThemeToggle } from "@/components/theme-toggle"
import { buttonVariants } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import LocaleSwitcher from "@/components/locale-switcher"

export function SiteHeader() {
  return (
    <header className="border-b">
      <div className="mx-auto flex w-full max-w-6xl flex-wrap items-center justify-between gap-x-4 gap-y-2 px-4 py-3 sm:px-6 lg:flex-nowrap">
        <Link to="/">
          <AppLogo />
        </Link>
        <nav className="order-3 flex w-full items-center gap-1 overflow-x-auto lg:order-none lg:w-auto lg:overflow-visible">
          <Link
            to="/tenders"
            className={cn(buttonVariants({ variant: "ghost" }))}
          >
            {m.nav_tenders()}
          </Link>
          <Link
            to="/objects"
            className={cn(buttonVariants({ variant: "ghost" }))}
          >
            {m.nav_objects()}
          </Link>
          {/* FR-1801 (п. 104): характеристики земельных участков */}
          <Link
            to="/land-plots"
            className={cn(buttonVariants({ variant: "ghost" }))}
          >
            {m.nav_land_plots()}
          </Link>
          {/* FR-1403 (п. 90, 92, 97): публикации особого порядка */}
          <Link
            to="/special-orders"
            className={cn(buttonVariants({ variant: "ghost" }))}
          >
            {m.nav_special_orders()}
          </Link>
          <Link
            to="/how-to"
            className={cn(buttonVariants({ variant: "ghost" }))}
          >
            {m.nav_how_to()}
          </Link>
        </nav>
        <div className="flex shrink-0 items-center gap-2">
          <ThemeToggle />
          <LocaleSwitcher />
          <Link
            to="/auth/login"
            className={cn(buttonVariants({ variant: "outline" }))}
          >
            {m.sign_in()}
          </Link>
        </div>
      </div>
    </header>
  )
}
