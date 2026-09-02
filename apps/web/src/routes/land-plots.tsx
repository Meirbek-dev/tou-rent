import { LandPlotIcon } from "lucide-react"
import { createFileRoute } from "@tanstack/react-router"

import { m } from "#/paraglide/messages"
import { EmptyState } from "@/components/empty-state"
import { PublicShell } from "@/components/public-shell"
import { RegistryPending } from "@/components/registry-skeleton"
import { Badge } from "@/components/ui/badge"
import { formatTenge, trimZeros } from "@/lib/format"
import { landPlotsQuery } from "@/lib/land"
import { cn } from "@/lib/utils"

import type { LandPlot } from "@/lib/land"

// FR-1801 (п. 104): характеристики земельных участков - под общежития
// и иное. Страница публичная и работает без JS (SSR, FR-1401): по
// опубликованному участку инвестор подает заявку из своего кабинета (п. 105).
export const Route = createFileRoute("/land-plots")({
  loader: ({ context }) => context.queryClient.ensureQueryData(landPlotsQuery),
  head: () => ({
    meta: [
      { title: `${m.land_plots_title()} - ToU Rent` },
      { property: "og:title", content: m.land_plots_title() },
      { property: "og:description", content: m.land_plots_hint() },
    ],
  }),
  component: LandPlotsPage,
  pendingComponent: () => <RegistryPending title={m.land_plots_title()} />,
})

function LandPlotsPage() {
  const plots = Route.useLoaderData()
  const published = plots.filter((plot) => plot.published_at != null)

  return (
    <PublicShell>
      <div className="flex flex-col gap-6">
        <div className="flex flex-col gap-2">
          <h1 className="text-3xl font-semibold tracking-tight">
            {m.land_plots_title()}
          </h1>
          <p className="max-w-[68ch] text-muted-foreground">
            {m.land_plots_hint()}
          </p>
        </div>

        {published.length === 0 ? (
          <EmptyState
            icon={LandPlotIcon}
            title={m.land_plots_empty_title()}
            description={m.land_plots_empty()}
          />
        ) : (
          <>
            <p className="text-sm text-muted-foreground">
              {m.registry_found()}:{" "}
              <span className="font-medium text-foreground tabular-nums">
                {published.length}
              </span>
            </p>
            <ul className="flex flex-col gap-3">
              {published.map((plot) => (
                <li key={plot.object_id}>
                  <PlotCard plot={plot} />
                </li>
              ))}
            </ul>
          </>
        )}
      </div>
    </PublicShell>
  )
}

function Fact({
  label,
  value,
  numeric = false,
  wide = false,
  intl = false,
}: {
  label: string
  value: string
  numeric?: boolean
  wide?: boolean
  /** Intl-вывод может отличаться между SSR и браузерами с урезанным ICU */
  intl?: boolean
}) {
  return (
    <div className={cn("flex gap-2", wide && "sm:col-span-2")}>
      <dt className="shrink-0 text-muted-foreground">{label}</dt>
      <dd
        className={cn(numeric && "tabular-nums")}
        suppressHydrationWarning={intl}
      >
        {value}
      </dd>
    </div>
  )
}

function PlotCard({ plot }: { plot: LandPlot }) {
  return (
    <article className="flex flex-col gap-3 rounded-xl border bg-card p-5 shadow-xs">
      <div className="flex flex-wrap items-center gap-3">
        <Badge variant="outline">{plot.designation_label}</Badge>
        <span className="text-sm text-muted-foreground">{plot.address}</span>
      </div>
      <h2 className="text-base font-semibold">{plot.name}</h2>
      <dl className="grid gap-x-6 gap-y-1 text-sm sm:grid-cols-2">
        <Fact
          label={m.land_plot_cadastral()}
          value={plot.cadastral_number}
          numeric
        />
        {/* Площадь оформляется так же, как в витрине объектов: «{area} м²» */}
        <Fact
          label={m.object_area_label()}
          value={m.object_area_value({ area: trimZeros(plot.area_m2) })}
          numeric
        />
        <Fact
          label={m.land_plot_permitted_use()}
          value={plot.permitted_use}
          wide
        />
        {plot.min_investment != null && (
          <Fact
            label={m.land_plot_min_investment()}
            value={formatTenge(plot.min_investment)}
            numeric
            wide
            intl
          />
        )}
      </dl>
      <p className="border-t pt-3 text-sm text-muted-foreground">
        {m.land_plot_apply_hint()}
      </p>
    </article>
  )
}
