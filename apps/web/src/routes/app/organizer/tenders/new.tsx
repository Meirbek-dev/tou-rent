import { useState } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { BuildingIcon } from "lucide-react"
import { Link } from "@tanstack/react-router"
import { m } from "#/paraglide/messages"
import { EmptyState } from "@/components/empty-state"
import { Button, buttonVariants } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { api } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import {
  defaultRateOptions,
  objectsQuery,
  organizerTendersQuery,
  rateOptionsQuery,
} from "@/lib/organizer"
import { RateOptionsFields } from "@/components/rate-fields"

import type { RateOptions } from "@/lib/organizer"

// FR-301: тендер с лотами; снимок ставки считает сервер по опциям Прил. 4.
export const Route = createFileRoute("/app/organizer/tenders/new")({
  loader: async ({ context }) => {
    await Promise.all([
      context.queryClient.ensureQueryData(objectsQuery),
      context.queryClient.ensureQueryData(rateOptionsQuery),
    ])
  },
  head: () => ({ meta: [{ title: `${m.tender_create_title()} - ToU Rent` }] }),
  component: NewTenderPage,
})

type LotDraft = {
  object_id: string
  purpose: string
  purpose_kk: string
  lease_months: string
  viewing_terms: string
  rate_options: RateOptions
  /** FR-205: `monthly` - ставка за месяц, `hourly` - почасовая аренда (п. 97) */
  rate_unit: "monthly" | "hourly"
  /** Объем разыгрываемых часов почасового лота */
  hours_total: string
}

function emptyLot(objectId: string): LotDraft {
  return {
    object_id: objectId,
    purpose: "",
    purpose_kk: "",
    lease_months: "12",
    viewing_terms: "",
    rate_options: defaultRateOptions(),
    rate_unit: "monthly",
    hours_total: "8",
  }
}

function NewTenderPage() {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { data: objectsPage } = useSuspenseQuery(objectsQuery)
  const firstObjectId = objectsPage.items[0]?.id ?? ""

  const [title, setTitle] = useState("")
  const [lots, setLots] = useState<LotDraft[]>([emptyLot(firstObjectId)])

  const patchLot = (index: number, patch: Partial<LotDraft>) => {
    setLots((current) =>
      current.map((lot, i) => (i === index ? { ...lot, ...patch } : lot))
    )
  }

  const create = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST("/api/v1/tenders", {
        body: {
          title,
          lots: lots.map((lot) => ({
            object_id: lot.object_id,
            purpose: lot.purpose,
            purpose_kk: lot.purpose_kk,
            lease_months: Number(lot.lease_months),
            viewing_terms: lot.viewing_terms === "" ? null : lot.viewing_terms,
            rate_options: lot.rate_options,
            rate_unit: lot.rate_unit,
            hours_total:
              lot.rate_unit === "hourly" ? Number(lot.hours_total) : null,
          })),
        },
      })
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("failed to create tender")
      }
      return data
    },
    onSuccess: async (tender) => {
      await queryClient.invalidateQueries({
        queryKey: organizerTendersQuery.queryKey,
      })
      await navigate({
        to: "/app/organizer/tenders/$tenderId",
        params: { tenderId: tender.id },
      })
    },
  })

  // Тупик «объектов нет» был абзацем без выхода: лот без объекта не создать,
  // а куда идти заводить объект - страница не говорила
  if (objectsPage.items.length === 0) {
    return (
      <EmptyState
        icon={BuildingIcon}
        titleAs="h1"
        title={m.objects_empty_title()}
        description={m.tender_new_no_objects()}
        action={
          <Link to="/app/organizer/objects" className={buttonVariants()}>
            {m.object_create_title()}
          </Link>
        }
      />
    )
  }

  return (
    <form
      className="flex flex-col gap-6"
      onSubmit={(event) => {
        event.preventDefault()
        create.mutate()
      }}
    >
      <h1 className="font-heading text-2xl font-semibold">
        {m.tender_create_title()}
      </h1>

      <div className="flex flex-col gap-1.5">
        <Label htmlFor="tender-title">{m.tender_title_label()}</Label>
        <Input
          id="tender-title"
          required
          value={title}
          onChange={(event) => setTitle(event.target.value)}
        />
      </div>

      {lots.map((lot, index) => (
        <fieldset
          key={index}
          className="flex flex-col gap-4 rounded-lg border p-4"
        >
          <legend className="px-1 font-medium">
            {m.tender_lot_legend({ n: index + 1 })}
          </legend>

          <div className="flex flex-wrap gap-3">
            <div className="flex min-w-64 flex-1 flex-col gap-1.5">
              <Label htmlFor={`lot-${index}-object`}>
                {m.tender_lot_object()}
              </Label>
              <NativeSelect
                id={`lot-${index}-object`}
                value={lot.object_id}
                onChange={(event) =>
                  patchLot(index, { object_id: event.target.value })
                }
              >
                {objectsPage.items.map((object) => (
                  <NativeSelectOption key={object.id} value={object.id}>
                    {object.name} -{" "}
                    {m.object_area_value({ area: object.area_m2 })}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
            </div>
            <div className="flex min-w-64 flex-1 flex-col gap-1.5">
              <Label htmlFor={`lot-${index}-purpose`}>
                {m.lot_purpose_ru()}
              </Label>
              <Input
                id={`lot-${index}-purpose`}
                required
                value={lot.purpose}
                onChange={(event) =>
                  patchLot(index, { purpose: event.target.value })
                }
              />
            </div>
            <div className="flex min-w-64 flex-1 flex-col gap-1.5">
              <Label htmlFor={`lot-${index}-purpose-kk`}>
                {m.lot_purpose_kk()}
              </Label>
              <Input
                id={`lot-${index}-purpose-kk`}
                required
                value={lot.purpose_kk}
                onChange={(event) =>
                  patchLot(index, { purpose_kk: event.target.value })
                }
              />
            </div>
            <div className="flex w-36 flex-col gap-1.5">
              <Label htmlFor={`lot-${index}-months`}>
                {m.lot_lease_months()}
              </Label>
              <Input
                id={`lot-${index}-months`}
                required
                type="number"
                min="1"
                max="240"
                value={lot.lease_months}
                onChange={(event) =>
                  patchLot(index, { lease_months: event.target.value })
                }
              />
            </div>
            {/* FR-205 (п. 97): почасовой лот торгуется ставкой за час */}
            <div className="flex w-44 flex-col gap-1.5">
              <Label htmlFor={`lot-${index}-unit`}>{m.lot_rate_unit()}</Label>
              <NativeSelect
                id={`lot-${index}-unit`}
                value={lot.rate_unit}
                onChange={(event) =>
                  patchLot(index, {
                    rate_unit: event.target.value as "monthly" | "hourly",
                  })
                }
              >
                <NativeSelectOption value="monthly">
                  {m.lot_rate_unit_monthly()}
                </NativeSelectOption>
                <NativeSelectOption value="hourly">
                  {m.lot_rate_unit_hourly()}
                </NativeSelectOption>
              </NativeSelect>
            </div>
            {lot.rate_unit === "hourly" && (
              <div className="flex w-36 flex-col gap-1.5">
                <Label htmlFor={`lot-${index}-hours`}>{m.lot_hours()}</Label>
                <Input
                  id={`lot-${index}-hours`}
                  required
                  type="number"
                  min="1"
                  max="10000"
                  value={lot.hours_total}
                  onChange={(event) =>
                    patchLot(index, { hours_total: event.target.value })
                  }
                />
              </div>
            )}
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor={`lot-${index}-viewing`}>
              {m.lot_viewing_terms()}
            </Label>
            <Input
              id={`lot-${index}-viewing`}
              value={lot.viewing_terms}
              onChange={(event) =>
                patchLot(index, { viewing_terms: event.target.value })
              }
            />
          </div>

          <details>
            <summary className="cursor-pointer text-sm font-medium">
              {m.tender_lot_coefficients()}
            </summary>
            <div className="pt-3">
              <RateOptionsFields
                value={lot.rate_options}
                onChange={(next) => patchLot(index, { rate_options: next })}
                idPrefix={`lot-${index}`}
              />
            </div>
          </details>

          {lots.length > 1 && (
            <div>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={() =>
                  setLots((current) => current.filter((_, i) => i !== index))
                }
              >
                {m.tender_lot_remove()}
              </Button>
            </div>
          )}
        </fieldset>
      ))}

      <div className="flex flex-wrap gap-3">
        <Button
          type="button"
          variant="outline"
          onClick={() =>
            setLots((current) => [...current, emptyLot(firstObjectId)])
          }
        >
          {m.tender_lot_add()}
        </Button>
        <Button
          type="submit"
          data-testid="create-tender"
          disabled={create.isPending}
        >
          {m.tender_create_submit()}
        </Button>
      </div>
      {create.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(create.error)}
        </p>
      )}
    </form>
  )
}
