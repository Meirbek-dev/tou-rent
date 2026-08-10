import { Link, createFileRoute } from "@tanstack/react-router"
import { m } from "#/paraglide/messages"
import { SiteHeader } from "@/components/site-header"
import { TenderListItem } from "@/components/tender-list-item"
import { tenderStatusLabel } from "@/components/tender-status-badge"
import { Button } from "@/components/ui/button"
import { buttonVariants } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { tendersPageQuery } from "@/lib/api"
import { PUBLIC_STATUSES, validateTendersSearch } from "@/lib/tenders-search"
import { cn } from "@/lib/utils"

// FR-1401: реестр объявлений, фильтры живут в URL (shareable, работает без JS:
// форма отправляется нативным GET, ссылки - обычные <a>).
export const Route = createFileRoute("/tenders/")({
  validateSearch: validateTendersSearch,
  loaderDeps: ({ search }) => ({ after: search.after }),
  loader: ({ context, deps }) =>
    context.queryClient.ensureQueryData(tendersPageQuery(deps.after)),
  head: () => ({ meta: [{ title: `${m.tenders_title()} - ToU Rent` }] }),
  component: TendersPage,
})

function TendersPage() {
  const page = Route.useLoaderData()
  const search = Route.useSearch()

  // Фильтрация поверх страницы API: объемы контура 1 умещаются в одну страницу (A-017)
  const q = search.q?.toLowerCase()
  const items = page.items.filter(
    (tender) =>
      (search.status === undefined || tender.status === search.status) &&
      (q === undefined || tender.title.toLowerCase().includes(q))
  )

  return (
    <>
      <SiteHeader />
      <main className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-6 py-10">
        <h1 className="font-heading text-3xl font-semibold tracking-tight">
          {m.tenders_title()}
        </h1>

        {/* Нативная GET-форма: состояние фильтров - в query-параметрах URL */}
        <form method="get" aria-label={m.tenders_filter_legend()}>
          <fieldset className="flex flex-wrap items-end gap-3 rounded-lg border p-4">
            <legend className="sr-only">{m.tenders_filter_legend()}</legend>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="filter-status">
                {m.tenders_filter_status_label()}
              </Label>
              <NativeSelect
                id="filter-status"
                name="status"
                defaultValue={search.status ?? ""}
                className="min-w-48"
              >
                <NativeSelectOption value="">
                  {m.tenders_filter_all()}
                </NativeSelectOption>
                {PUBLIC_STATUSES.map((status) => (
                  <NativeSelectOption key={status} value={status}>
                    {tenderStatusLabel(status)}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="filter-q">{m.tenders_filter_query_label()}</Label>
              <Input
                id="filter-q"
                type="search"
                name="q"
                defaultValue={search.q ?? ""}
                className="min-w-56"
              />
            </div>
            <Button type="submit">{m.tenders_filter_submit()}</Button>
            {(search.status !== undefined || search.q !== undefined) && (
              <Link
                to="/tenders"
                className={cn(buttonVariants({ variant: "ghost" }))}
              >
                {m.tenders_filter_reset()}
              </Link>
            )}
          </fieldset>
        </form>

        {items.length === 0 ? (
          <p className="py-8 text-muted-foreground">{m.tenders_empty()}</p>
        ) : (
          <ul className="flex flex-col gap-3">
            {items.map((tender) => (
              <TenderListItem key={tender.id} tender={tender} />
            ))}
          </ul>
        )}

        <nav
          aria-label={m.tenders_next_page()}
          className="flex items-center gap-3"
        >
          {search.after !== undefined && (
            <Link
              to="/tenders"
              search={{ status: search.status, q: search.q }}
              className={cn(buttonVariants({ variant: "outline" }))}
            >
              {m.tenders_first_page()}
            </Link>
          )}
          {page.next_after != null && (
            <Link
              to="/tenders"
              search={{ ...search, after: page.next_after }}
              className={cn(buttonVariants({ variant: "outline" }))}
            >
              {m.tenders_next_page()}
            </Link>
          )}
        </nav>
      </main>
    </>
  )
}
