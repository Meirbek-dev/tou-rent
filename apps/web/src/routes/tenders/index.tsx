import { SearchXIcon } from "lucide-react"
import { Link, createFileRoute } from "@tanstack/react-router"

import { m } from "#/paraglide/messages"
import { EmptyState } from "@/components/empty-state"
import { PublicShell } from "@/components/public-shell"
import { RegistryPending } from "@/components/registry-skeleton"
import { TenderListItem } from "@/components/tender-list-item"
import { tenderStatusLabel } from "@/components/tender-status-badge"
import { Button, buttonVariants } from "@/components/ui/button"
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
  pendingComponent: () => <RegistryPending title={m.tenders_title()} />,
})

function TendersPage() {
  const page = Route.useLoaderData()
  const search = Route.useSearch()
  const filtered = search.status !== undefined || search.q !== undefined

  // Фильтрация поверх страницы API: объемы контура 1 умещаются в одну страницу (A-017)
  const q = search.q?.toLowerCase()
  const items = page.items.filter(
    (tender) =>
      (search.status === undefined || tender.status === search.status) &&
      (q === undefined || tender.title.toLowerCase().includes(q))
  )

  return (
    <PublicShell>
      <div className="flex flex-col gap-6">
        <h1 className="text-3xl font-semibold tracking-tight">
          {m.tenders_title()}
        </h1>

        {/* Нативная GET-форма: состояние фильтров - в query-параметрах URL */}
        <form method="get" aria-label={m.tenders_filter_legend()}>
          <fieldset className="grid grid-cols-1 items-end gap-3 rounded-xl border bg-card p-4 sm:grid-cols-[minmax(0,14rem)_minmax(0,1fr)_auto] sm:gap-4">
            <legend className="sr-only">{m.tenders_filter_legend()}</legend>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="filter-status">
                {m.tenders_filter_status_label()}
              </Label>
              <NativeSelect
                id="filter-status"
                name="status"
                defaultValue={search.status ?? ""}
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
              />
            </div>
            <div className="flex items-center gap-2">
              <Button type="submit">{m.tenders_filter_submit()}</Button>
              {filtered && (
                <Link
                  to="/tenders"
                  className={cn(buttonVariants({ variant: "ghost" }))}
                >
                  {m.tenders_filter_reset()}
                </Link>
              )}
            </div>
          </fieldset>
        </form>

        <p className="text-sm text-muted-foreground">
          {m.registry_found()}:{" "}
          <span className="font-medium text-foreground tabular-nums">
            {items.length}
          </span>
        </p>

        {items.length === 0 ? (
          <EmptyState
            icon={SearchXIcon}
            title={m.tenders_empty_title()}
            description={m.tenders_empty()}
            {...(filtered && {
              action: (
                <Link
                  to="/tenders"
                  className={cn(buttonVariants({ variant: "outline" }))}
                >
                  {m.tenders_filter_reset()}
                </Link>
              ),
            })}
          />
        ) : (
          <ul className="flex flex-col gap-2">
            {items.map((tender) => (
              <TenderListItem
                key={tender.id}
                tender={tender}
                headingLevel={2}
              />
            ))}
          </ul>
        )}

        <nav
          aria-label={m.tenders_next_page()}
          className="flex flex-wrap items-center gap-3 border-t pt-6"
        >
          <p className="text-sm text-muted-foreground">
            {m.pagination_on_page()}:{" "}
            <span className="font-medium text-foreground tabular-nums">
              {page.items.length}
            </span>
          </p>
          <div className="ml-auto flex items-center gap-3">
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
          </div>
        </nav>
      </div>
    </PublicShell>
  )
}
