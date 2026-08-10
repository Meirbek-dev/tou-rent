import { Link, createFileRoute } from "@tanstack/react-router"
import { m } from "#/paraglide/messages"
import {
  ObjectStatusBadge,
  objectStatusLabel,
} from "@/components/object-status-badge"
import { SiteHeader } from "@/components/site-header"
import { Button, buttonVariants } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { objectsPageQuery } from "@/lib/api"
import { trimZeros } from "@/lib/format"
import {
  OBJECT_KINDS,
  OBJECT_STATUSES,
  validateObjectsSearch,
} from "@/lib/objects-search"
import { cn } from "@/lib/utils"

import type { ObjectDto, ObjectKind } from "@/lib/api"

// FR-102: публичная витрина свободных площадей и земельных участков.
// SSR без авторизации; фильтры живут в URL и отправляются нативным GET -
// страница работает без JS (NFR-04). Статус объекта вычисляемый (FR-103).
export const Route = createFileRoute("/objects/")({
  validateSearch: validateObjectsSearch,
  loaderDeps: ({ search }) => search,
  loader: ({ context, deps }) =>
    context.queryClient.ensureQueryData(objectsPageQuery(deps)),
  head: () => ({ meta: [{ title: `${m.objects_title()} - ToU Rent` }] }),
  component: ObjectsPage,
})

const KIND_LABELS: Record<ObjectKind, () => string> = {
  premises: m.object_kind_premises,
  building: m.object_kind_building,
  structure: m.object_kind_structure,
  land_plot: m.object_kind_land_plot,
}

function ObjectCard({ object }: { object: ObjectDto }) {
  return (
    <li className="flex flex-col gap-3 rounded-lg border p-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="font-medium">{object.name}</h2>
        <ObjectStatusBadge status={object.status} />
      </div>
      <dl className="grid grid-cols-1 gap-x-6 gap-y-1 text-sm sm:grid-cols-2">
        <div className="flex gap-2">
          <dt className="text-muted-foreground">{m.object_kind_label()}:</dt>
          <dd>{KIND_LABELS[object.kind]()}</dd>
        </div>
        <div className="flex gap-2">
          <dt className="text-muted-foreground">{m.object_area_label()}:</dt>
          <dd>{m.object_area_value({ area: trimZeros(object.area_m2) })}</dd>
        </div>
        <div className="flex gap-2">
          <dt className="text-muted-foreground">{m.object_address_label()}:</dt>
          <dd>{object.address}</dd>
        </div>
        {object.floor_part != null && object.floor_part !== "" && (
          <div className="flex gap-2">
            <dt className="text-muted-foreground">{m.object_floor_label()}:</dt>
            <dd>{object.floor_part}</dd>
          </div>
        )}
      </dl>
    </li>
  )
}

function ObjectsPage() {
  const page = Route.useLoaderData()
  const search = Route.useSearch()
  const filtered =
    search.status !== undefined ||
    search.kind !== undefined ||
    search.q !== undefined ||
    search.area_min !== undefined ||
    search.area_max !== undefined

  return (
    <>
      <SiteHeader />
      <main className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-6 py-10">
        <div className="flex flex-col gap-2">
          <h1 className="font-heading text-3xl font-semibold tracking-tight">
            {m.objects_title()}
          </h1>
          <p className="text-muted-foreground">{m.objects_subtitle()}</p>
        </div>

        {/* Нативная GET-форма: состояние фильтров - в query-параметрах URL */}
        <form method="get" aria-label={m.objects_filter_legend()}>
          <fieldset className="flex flex-wrap items-end gap-3 rounded-lg border p-4">
            <legend className="sr-only">{m.objects_filter_legend()}</legend>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="objects-status">
                {m.objects_filter_status_label()}
              </Label>
              <NativeSelect
                id="objects-status"
                name="status"
                defaultValue={search.status ?? ""}
                className="min-w-44"
              >
                <NativeSelectOption value="">
                  {m.objects_filter_all_statuses()}
                </NativeSelectOption>
                {OBJECT_STATUSES.map((status) => (
                  <NativeSelectOption key={status} value={status}>
                    {objectStatusLabel(status)}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
            </div>

            <div className="flex flex-col gap-1.5">
              <Label htmlFor="objects-kind">
                {m.objects_filter_kind_label()}
              </Label>
              <NativeSelect
                id="objects-kind"
                name="kind"
                defaultValue={search.kind ?? ""}
                className="min-w-44"
              >
                <NativeSelectOption value="">
                  {m.objects_filter_all_kinds()}
                </NativeSelectOption>
                {OBJECT_KINDS.map((kind) => (
                  <NativeSelectOption key={kind} value={kind}>
                    {KIND_LABELS[kind]()}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
            </div>

            <div className="flex flex-col gap-1.5">
              <Label htmlFor="objects-area-min">
                {m.objects_filter_area_min()}
              </Label>
              <Input
                id="objects-area-min"
                type="number"
                inputMode="decimal"
                min="0"
                step="0.01"
                name="area_min"
                defaultValue={search.area_min ?? ""}
                className="w-32"
              />
            </div>

            <div className="flex flex-col gap-1.5">
              <Label htmlFor="objects-area-max">
                {m.objects_filter_area_max()}
              </Label>
              <Input
                id="objects-area-max"
                type="number"
                inputMode="decimal"
                min="0"
                step="0.01"
                name="area_max"
                defaultValue={search.area_max ?? ""}
                className="w-32"
              />
            </div>

            <div className="flex flex-col gap-1.5">
              <Label htmlFor="objects-q">
                {m.objects_filter_query_label()}
              </Label>
              <Input
                id="objects-q"
                type="search"
                name="q"
                defaultValue={search.q ?? ""}
                className="min-w-56"
              />
            </div>

            <Button type="submit">{m.tenders_filter_submit()}</Button>
            {filtered && (
              <Link
                to="/objects"
                className={cn(buttonVariants({ variant: "ghost" }))}
              >
                {m.tenders_filter_reset()}
              </Link>
            )}
          </fieldset>
        </form>

        {page.items.length === 0 ? (
          <p className="py-8 text-muted-foreground">{m.objects_empty()}</p>
        ) : (
          <ul className="grid grid-cols-1 gap-3 md:grid-cols-2">
            {page.items.map((object) => (
              <ObjectCard key={object.id} object={object} />
            ))}
          </ul>
        )}

        <nav
          aria-label={m.tenders_next_page()}
          className="flex items-center gap-3"
        >
          {search.after !== undefined && (
            <Link
              to="/objects"
              search={{ ...search, after: undefined }}
              className={cn(buttonVariants({ variant: "outline" }))}
            >
              {m.tenders_first_page()}
            </Link>
          )}
          {page.next_after != null && (
            <Link
              to="/objects"
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
