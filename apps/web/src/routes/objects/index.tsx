import { SearchXIcon } from "lucide-react"
import { Link, createFileRoute } from "@tanstack/react-router"

import { m } from "#/paraglide/messages"
import { getLocale } from "#/paraglide/runtime"
import { EmptyState } from "@/components/empty-state"
import {
  ObjectStatusBadge,
  objectStatusLabel,
} from "@/components/object-status-badge"
import { PublicShell } from "@/components/public-shell"
import { RegistryPending } from "@/components/registry-skeleton"
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
  head: () => ({
    meta: [
      { title: `${m.objects_title()} - ToU Rent` },
      { property: "og:title", content: m.objects_title() },
      { property: "og:description", content: m.objects_subtitle() },
    ],
  }),
  component: ObjectsPage,
  pendingComponent: () => <RegistryPending title={m.objects_title()} />,
})

const KIND_LABELS: Record<ObjectKind, () => string> = {
  premises: m.object_kind_premises,
  building: m.object_kind_building,
  structure: m.object_kind_structure,
  land_plot: m.object_kind_land_plot,
}

function Fact({
  label,
  value,
  numeric = false,
  wide = false,
}: {
  label: string
  value: string
  numeric?: boolean
  wide?: boolean
}) {
  return (
    <div className={cn("flex gap-2", wide && "sm:col-span-2")}>
      <dt className="shrink-0 text-muted-foreground">{label}:</dt>
      <dd className={cn(numeric && "tabular-nums")}>{value}</dd>
    </div>
  )
}

function ObjectCard({ object }: { object: ObjectDto }) {
  const kazakh = getLocale() === "kk"
  return (
    <li className="grid grid-cols-[minmax(0,1fr)] overflow-hidden rounded-xl border bg-card shadow-xs">
      <div className="flex flex-col gap-3 p-4 sm:p-5">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <h2 className="text-base font-semibold">
            {kazakh ? object.name_kk : object.name}
          </h2>
          <ObjectStatusBadge status={object.status} />
        </div>
        <dl className="grid grid-cols-1 gap-x-6 gap-y-1 text-sm sm:grid-cols-2">
          <Fact
            label={m.object_kind_label()}
            value={KIND_LABELS[object.kind]()}
          />
          <Fact
            label={m.object_area_label()}
            value={m.object_area_value({ area: trimZeros(object.area_m2) })}
            numeric
          />
          <Fact
            label={m.object_address_label()}
            value={kazakh ? object.address_kk : object.address}
            wide
          />
          {object.floor_part != null && object.floor_part !== "" && (
            <Fact label={m.object_floor_label()} value={object.floor_part} />
          )}
        </dl>
      </div>
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
    <PublicShell>
      <div className="flex flex-col gap-6">
        <div className="flex flex-col gap-2">
          <h1 className="text-3xl font-semibold tracking-tight">
            {m.objects_title()}
          </h1>
          <p className="max-w-[68ch] text-muted-foreground">
            {m.objects_subtitle()}
          </p>
        </div>

        {/* Нативная GET-форма: состояние фильтров - в query-параметрах URL */}
        <form method="get" aria-label={m.objects_filter_legend()}>
          <fieldset className="grid grid-cols-1 items-end gap-3 rounded-xl border bg-card p-4 sm:grid-cols-2 sm:gap-4 lg:grid-cols-[minmax(0,11rem)_minmax(0,11rem)_minmax(0,16rem)_minmax(0,1fr)_auto]">
            <legend className="sr-only">{m.objects_filter_legend()}</legend>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="objects-status">
                {m.objects_filter_status_label()}
              </Label>
              <NativeSelect
                id="objects-status"
                name="status"
                defaultValue={search.status ?? ""}
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

            {/* Границы площади - одна ячейка: это один фильтр, а не два */}
            <div className="grid grid-cols-2 gap-2 sm:col-span-2 lg:col-span-1">
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
                />
              </div>
            </div>

            <div className="flex flex-col gap-1.5 sm:col-span-2 lg:col-span-1">
              <Label htmlFor="objects-q">
                {m.objects_filter_query_label()}
              </Label>
              <Input
                id="objects-q"
                type="search"
                name="q"
                defaultValue={search.q ?? ""}
              />
            </div>

            <div className="flex items-center gap-2 sm:col-span-2 lg:col-span-1">
              <Button type="submit">{m.tenders_filter_submit()}</Button>
              {filtered && (
                <Link
                  to="/objects"
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
            {page.items.length}
          </span>
        </p>

        {page.items.length === 0 ? (
          <EmptyState
            icon={SearchXIcon}
            title={m.objects_empty_title()}
            description={m.objects_empty()}
            {...(filtered && {
              action: (
                <Link
                  to="/objects"
                  className={cn(buttonVariants({ variant: "outline" }))}
                >
                  {m.tenders_filter_reset()}
                </Link>
              ),
            })}
          />
        ) : (
          <ul className="grid grid-cols-1 gap-3 md:grid-cols-2">
            {page.items.map((object) => (
              <ObjectCard key={object.id} object={object} />
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
          </div>
        </nav>
      </div>
    </PublicShell>
  )
}
