import { createFileRoute } from "@tanstack/react-router"
import { m } from "#/paraglide/messages"
import { SiteHeader } from "@/components/site-header"
import { formatTenge } from "@/lib/format"
import { landPlotsQuery } from "@/lib/land"

import type { LandPlot } from "@/lib/land"

// FR-1801 (п. 104): характеристики земельных участков - под общежития
// и иное. Страница публичная и работает без JS (SSR, FR-1401): по
// опубликованному участку инвестор подает заявку из своего кабинета (п. 105).
export const Route = createFileRoute("/land-plots")({
  loader: ({ context }) => context.queryClient.ensureQueryData(landPlotsQuery),
  head: () => ({ meta: [{ title: `${m.land_plots_title()} - ToU Rent` }] }),
  component: LandPlotsPage,
})

function LandPlotsPage() {
  const plots = Route.useLoaderData()
  const published = plots.filter((plot) => plot.published_at != null)

  return (
    <>
      <SiteHeader />
      <main className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-6 py-10">
        <h1 className="font-heading text-3xl font-semibold tracking-tight">
          {m.land_plots_title()}
        </h1>
        <p className="text-muted-foreground">{m.land_plots_hint()}</p>

        {published.length === 0 ? (
          <p className="py-8 text-muted-foreground">{m.land_plots_empty()}</p>
        ) : (
          <ul className="flex flex-col gap-3">
            {published.map((plot) => (
              <li key={plot.object_id}>
                <PlotCard plot={plot} />
              </li>
            ))}
          </ul>
        )}
      </main>
    </>
  )
}

function PlotCard({ plot }: { plot: LandPlot }) {
  return (
    <article className="flex flex-col gap-2 rounded-lg border p-4">
      <div className="flex flex-wrap items-center gap-3">
        <span className="rounded-md border px-2 py-0.5 text-sm">
          {plot.designation_label}
        </span>
        <span className="text-sm text-muted-foreground">{plot.address}</span>
      </div>
      <h2 className="font-medium">{plot.name}</h2>
      <dl className="grid gap-x-6 gap-y-1 text-sm sm:grid-cols-2">
        <div className="flex gap-2">
          <dt className="text-muted-foreground">{m.land_plot_cadastral()}</dt>
          <dd>{plot.cadastral_number}</dd>
        </div>
        <div className="flex gap-2">
          <dt className="text-muted-foreground">{m.object_area_label()}</dt>
          <dd>{plot.area_m2}</dd>
        </div>
        <div className="flex gap-2 sm:col-span-2">
          <dt className="text-muted-foreground">
            {m.land_plot_permitted_use()}
          </dt>
          <dd>{plot.permitted_use}</dd>
        </div>
        {plot.min_investment != null && (
          <div className="flex gap-2 sm:col-span-2">
            <dt className="text-muted-foreground">
              {m.land_plot_min_investment()}
            </dt>
            <dd>{formatTenge(plot.min_investment)}</dd>
          </div>
        )}
      </dl>
      <p className="text-sm text-muted-foreground">
        {m.land_plot_apply_hint()}
      </p>
    </article>
  )
}
