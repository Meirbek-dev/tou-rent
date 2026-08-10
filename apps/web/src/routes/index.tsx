import { Link, createFileRoute } from "@tanstack/react-router"
import { m } from "#/paraglide/messages"
import { SiteHeader } from "@/components/site-header"
import { TenderListItem } from "@/components/tender-list-item"
import { buttonVariants } from "@/components/ui/button"
import { tendersPageQuery } from "@/lib/api"
import { cn } from "@/lib/utils"

// FR-1401: главная - витрина портала с актуальными объявлениями (SSR)
export const Route = createFileRoute("/")({
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(tendersPageQuery()),
  component: Home,
})

const LATEST_COUNT = 4

function Home() {
  const page = Route.useLoaderData()
  const latest = page.items.slice(0, LATEST_COUNT)

  return (
    <>
      <SiteHeader />
      <main className="mx-auto flex w-full max-w-6xl flex-col gap-12 px-6 py-16">
        <section className="flex flex-col items-start gap-6">
          <h1 className="max-w-3xl font-heading text-4xl font-semibold tracking-tight text-balance">
            {m.portal_title()}
          </h1>
          <p className="max-w-2xl text-lg text-muted-foreground">
            {m.portal_subtitle()}
          </p>
          <div className="flex flex-wrap gap-3">
            <Link to="/tenders" className={cn(buttonVariants({ size: "lg" }))}>
              {m.portal_cta_tenders()}
            </Link>
            <Link
              to="/how-to"
              className={cn(buttonVariants({ variant: "outline", size: "lg" }))}
            >
              {m.nav_how_to()}
            </Link>
          </div>
        </section>

        <section
          aria-labelledby="latest-tenders"
          className="flex flex-col gap-4"
        >
          <div className="flex flex-wrap items-baseline justify-between gap-3">
            <h2
              id="latest-tenders"
              className="font-heading text-2xl font-semibold"
            >
              {m.home_latest_title()}
            </h2>
            <Link
              to="/tenders"
              className="text-sm underline-offset-4 hover:underline"
            >
              {m.home_all_tenders()}
            </Link>
          </div>
          {latest.length === 0 ? (
            <p className="text-muted-foreground">{m.home_latest_empty()}</p>
          ) : (
            <ul className="grid grid-cols-1 gap-3 md:grid-cols-2">
              {latest.map((tender) => (
                <TenderListItem key={tender.id} tender={tender} />
              ))}
            </ul>
          )}
        </section>
      </main>
    </>
  )
}
