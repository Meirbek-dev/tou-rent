import { ArrowRightIcon, InboxIcon } from "lucide-react"
import { Link, createFileRoute } from "@tanstack/react-router"

import { m } from "#/paraglide/messages"
import { getLocale } from "#/paraglide/runtime"
import { EmptyState } from "@/components/empty-state"
import { PublicShell } from "@/components/public-shell"
import { TenderListItem } from "@/components/tender-list-item"
import { buttonVariants } from "@/components/ui/button"
import {
  objectsPageQuery,
  siteAnnouncementQuery,
  tendersPageQuery,
} from "@/lib/api"
import { howToSteps } from "@/lib/how-to-steps"
import { landPlotsQuery } from "@/lib/land"
import { cn } from "@/lib/utils"

// FR-1401: главная - витрина портала с актуальными объявлениями (SSR)
export const Route = createFileRoute("/")({
  loader: async ({ context }) => {
    // Цифры витрины считаются из тех же выборок, что и разделы портала:
    // отдельного счетчика в api нет, а выдумывать его ради заголовка нельзя
    const [tenders, objects, plots, announcement] = await Promise.all([
      context.queryClient.ensureQueryData(tendersPageQuery()),
      context.queryClient.ensureQueryData(objectsPageQuery({ status: "free" })),
      context.queryClient.ensureQueryData(landPlotsQuery),
      context.queryClient.ensureQueryData(siteAnnouncementQuery),
    ])
    return { tenders, objects, plots, announcement }
  },
  // Единственный маршрут без своего `head` отдавал корневой запасной заголовок
  // «ToU Rent» на всех трех локалях - вкладка и выдача поиска не говорили,
  // что это за портал
  head: () => ({ meta: [{ title: `${m.portal_title()} - ToU Rent` }] }),
  component: Home,
})

const LATEST_COUNT = 5

const containerClass = "mx-auto w-full max-w-6xl px-4 sm:px-6"

/**
 * Счетчик по загруженной странице выдачи. Курсорная пагинация не знает
 * общего числа записей, поэтому усеченная страница честно помечается «+»:
 * «12» и «12+» - разные утверждения, и второе портал имеет право сделать.
 */
function pageCount(next: string | null | undefined, shown: number): string {
  return next == null ? String(shown) : `${shown}+`
}

function Stat({ value, label }: { value: string; label: string }) {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-3xl leading-none font-semibold tabular-nums">
        {value}
      </span>
      <span className="text-sm text-muted-foreground">{label}</span>
    </div>
  )
}

function EntryTile({
  to,
  title,
  text,
  className,
}: {
  to: "/objects" | "/land-plots" | "/special-orders"
  title: string
  text: string
  className?: string
}) {
  return (
    <Link
      to={to}
      className={cn(
        "group/tile flex flex-col gap-2 rounded-xl border bg-card p-6 shadow-xs transition-[border-color,box-shadow] hover:border-border hover:shadow-sm",
        className
      )}
    >
      <span className="flex items-center gap-2 font-heading text-lg font-semibold">
        {title}
        <ArrowRightIcon
          aria-hidden="true"
          className="size-4 text-muted-foreground transition-transform group-hover/tile:translate-x-0.5"
        />
      </span>
      <span className="text-sm text-muted-foreground">{text}</span>
    </Link>
  )
}

function Home() {
  const { tenders, objects, plots, announcement } = Route.useLoaderData()
  const kazakh = getLocale() === "kk"
  const latest = tenders.items.slice(0, LATEST_COUNT)
  const accepting = tenders.items.filter(
    (tender) => tender.status === "accepting"
  ).length
  const publishedPlots = plots.filter(
    (plot) => plot.published_at != null
  ).length
  const steps = howToSteps()

  return (
    <PublicShell contained={false}>
      {/* 1. Заголовок портала и цифры реестра */}
      <section className={cn(containerClass, "py-14 lg:py-20")}>
        <div className="grid grid-cols-1 gap-10 lg:grid-cols-12 lg:gap-12">
          <div className="flex flex-col items-start gap-6 lg:col-span-7">
            <h1 className="text-[clamp(2rem,4.6vw,3.25rem)] leading-[1.08] font-semibold tracking-tight text-balance">
              {m.portal_title()}
            </h1>
            <p className="max-w-[52ch] text-lg text-muted-foreground">
              {m.portal_subtitle()}
            </p>
            <div className="flex flex-wrap gap-3">
              <Link
                to="/tenders"
                className={cn(buttonVariants({ size: "lg" }))}
              >
                {m.portal_cta_tenders()}
              </Link>
              <Link
                to="/how-to"
                className={cn(
                  buttonVariants({ variant: "outline", size: "lg" })
                )}
              >
                {m.nav_how_to()}
              </Link>
            </div>
          </div>

          <section
            aria-labelledby="home-stats"
            className="rounded-xl border bg-card p-6 shadow-xs lg:col-span-5"
          >
            <h2
              id="home-stats"
              className="mb-5 text-sm font-medium text-muted-foreground"
            >
              {m.home_stats_title()}
            </h2>
            <div className="grid grid-cols-2 gap-x-6 gap-y-6">
              <Stat value={String(accepting)} label={m.home_stat_accepting()} />
              <Stat
                value={pageCount(tenders.next_after, tenders.items.length)}
                label={m.home_stat_announcements()}
              />
              <Stat
                value={pageCount(objects.next_after, objects.items.length)}
                label={m.home_stat_free_objects()}
              />
              <Stat
                value={String(publishedPlots)}
                label={m.home_stat_land_plots()}
              />
            </div>
          </section>
        </div>
      </section>

      {announcement !== null && (
        <section
          aria-labelledby="home-site-announcement"
          className={cn(containerClass, "pb-10")}
        >
          <article className="flex flex-col gap-4 rounded-xl border bg-card p-6 shadow-xs sm:p-8">
            <h2
              id="home-site-announcement"
              className="text-xl font-semibold tracking-tight"
            >
              {kazakh ? announcement.title_kk : announcement.title}
            </h2>
            <p className="text-sm leading-7 whitespace-pre-line text-muted-foreground sm:text-base">
              {kazakh ? announcement.body_kk : announcement.body}
            </p>
          </article>
        </section>
      )}

      {/* 2. Полоса принадлежности: где публикуются юридически значимые факты */}
      <div className="border-y bg-muted py-5">
        <div
          className={cn(
            containerClass,
            "flex flex-wrap items-center justify-between gap-x-8 gap-y-2"
          )}
        >
          <p className="max-w-[76ch] text-sm">{m.home_trust_text()}</p>
          <Link
            to="/how-to"
            className="text-sm font-medium text-primary underline-offset-4 hover:underline"
          >
            {m.home_how_more()}
          </Link>
        </div>
      </div>

      {/* 3. Порядок участия - тот же список, что и на /how-to */}
      <section
        aria-labelledby="home-how"
        className={cn(containerClass, "py-14")}
      >
        <div className="mb-8 flex flex-wrap items-baseline justify-between gap-3">
          <h2 id="home-how" className="text-2xl font-semibold tracking-tight">
            {m.howto_title()}
          </h2>
          <Link
            to="/how-to"
            className="text-sm text-primary underline-offset-4 hover:underline"
          >
            {m.home_how_more()}
          </Link>
        </div>
        <ol className="grid grid-cols-1 gap-8 md:grid-cols-4 md:gap-6">
          {steps.map((step, index) => (
            <li
              key={step.title}
              className="relative flex flex-col gap-3 md:pt-0"
            >
              {/* Волосяная линия связывает шаги в последовательность */}
              <span
                aria-hidden="true"
                className="absolute top-5 left-10 hidden h-px w-full bg-border md:block"
              />
              <span
                aria-hidden="true"
                className="relative z-[1] flex size-10 shrink-0 items-center justify-center rounded-xl bg-primary/10 font-mono text-sm font-semibold text-primary ring-1 ring-primary/20"
              >
                {index + 1}
              </span>
              <h3 className="text-base font-semibold">{step.title}</h3>
              <p className="text-sm text-muted-foreground">{step.text}</p>
            </li>
          ))}
        </ol>
      </section>

      {/* 4. Актуальные объявления */}
      <section
        aria-labelledby="latest-tenders"
        className={cn(containerClass, "py-6")}
      >
        <div className="mb-4 flex flex-wrap items-baseline justify-between gap-3">
          <h2
            id="latest-tenders"
            className="text-2xl font-semibold tracking-tight"
          >
            {m.home_latest_title()}
          </h2>
        </div>
        {latest.length === 0 ? (
          <EmptyState
            icon={InboxIcon}
            title={m.home_latest_empty_title()}
            description={m.home_latest_empty()}
          />
        ) : (
          <>
            <ul className="flex flex-col gap-2">
              {latest.map((tender) => (
                <TenderListItem key={tender.id} tender={tender} />
              ))}
            </ul>
            <Link
              to="/tenders"
              className="mt-2 flex items-center justify-between gap-3 rounded-xl border border-dashed border-border px-5 py-4 text-sm font-medium transition-colors hover:bg-muted"
            >
              {m.home_all_tenders()}
              <ArrowRightIcon aria-hidden="true" className="size-4" />
            </Link>
          </>
        )}
      </section>

      {/* 5. Точки входа в остальные разделы */}
      <section
        aria-labelledby="home-entries"
        className={cn(containerClass, "py-14")}
      >
        <h2
          id="home-entries"
          className="mb-6 text-2xl font-semibold tracking-tight"
        >
          {m.home_entry_title()}
        </h2>
        <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
          <EntryTile
            to="/objects"
            title={m.nav_objects()}
            text={m.objects_subtitle()}
            className="md:col-span-2"
          />
          <EntryTile
            to="/land-plots"
            title={m.nav_land_plots()}
            text={m.home_tile_land_text()}
          />
          <EntryTile
            to="/special-orders"
            title={m.nav_special_orders()}
            text={m.home_tile_special_text()}
            className="md:col-span-3"
          />
        </div>
      </section>
    </PublicShell>
  )
}
