import { m } from "#/paraglide/messages"
import { RateOptionsFields } from "@/components/rate-fields"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { defaultRateOptions } from "@/lib/organizer"

import type { ObjectDto, RateOptions } from "@/lib/organizer"
import type { components } from "@tou/api-client"

/**
 * Черновик лота в форме организатора. Поля держатся строками: `<input>`
 * возвращает строку, и приводить ее к числу на каждый ввод значило бы
 * ломать промежуточные состояния вроде пустого поля.
 */
export type LotDraft = {
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

export function emptyLot(objectId: string): LotDraft {
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

/** Черновик в тело запроса: снимок ставки по опциям Прил. 4 считает сервер. */
export function lotDraftToRequest(
  lot: LotDraft
): components["schemas"]["CreateLotRequest"] {
  return {
    object_id: lot.object_id,
    purpose: lot.purpose,
    purpose_kk: lot.purpose_kk,
    lease_months: Number(lot.lease_months),
    viewing_terms: lot.viewing_terms === "" ? null : lot.viewing_terms,
    rate_options: lot.rate_options,
    rate_unit: lot.rate_unit,
    hours_total: lot.rate_unit === "hourly" ? Number(lot.hours_total) : null,
  }
}

/**
 * Поля одного лота - общие для создания тендера (FR-301) и для новой
 * редакции документации, которая добавляет лоты к объявленному тендеру
 * (FR-304, п. 27). Обе формы правят один и тот же черновик, поэтому и
 * набор полей у них один: разойдись они - разошлись бы и проверки.
 */
export function LotDraftFields({
  lot,
  n,
  idPrefix,
  objects,
  onChange,
  onRemove,
}: {
  lot: LotDraft
  /** Номер лота в объявлении - показывается в заголовке блока */
  n: number
  idPrefix: string
  objects: ObjectDto[]
  onChange: (patch: Partial<LotDraft>) => void
  /** Не задан - лот убрать нельзя (единственный лот создаваемого тендера) */
  onRemove?: (() => void) | undefined
}) {
  return (
    <fieldset className="flex flex-col gap-4 rounded-lg border p-4">
      <legend className="px-1 font-medium">{m.tender_lot_legend({ n })}</legend>

      <div className="flex flex-wrap gap-3">
        <div className="flex min-w-64 flex-1 flex-col gap-1.5">
          <Label htmlFor={`${idPrefix}-object`}>{m.tender_lot_object()}</Label>
          <NativeSelect
            id={`${idPrefix}-object`}
            value={lot.object_id}
            onChange={(event) => onChange({ object_id: event.target.value })}
          >
            {objects.map((object) => (
              <NativeSelectOption key={object.id} value={object.id}>
                {object.name} - {m.object_area_value({ area: object.area_m2 })}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </div>
        <div className="flex min-w-64 flex-1 flex-col gap-1.5">
          <Label htmlFor={`${idPrefix}-purpose`}>{m.lot_purpose_ru()}</Label>
          <Input
            id={`${idPrefix}-purpose`}
            required
            value={lot.purpose}
            onChange={(event) => onChange({ purpose: event.target.value })}
          />
        </div>
        <div className="flex min-w-64 flex-1 flex-col gap-1.5">
          <Label htmlFor={`${idPrefix}-purpose-kk`}>{m.lot_purpose_kk()}</Label>
          <Input
            id={`${idPrefix}-purpose-kk`}
            required
            value={lot.purpose_kk}
            onChange={(event) => onChange({ purpose_kk: event.target.value })}
          />
        </div>
        <div className="flex w-36 flex-col gap-1.5">
          <Label htmlFor={`${idPrefix}-months`}>{m.lot_lease_months()}</Label>
          <Input
            id={`${idPrefix}-months`}
            required
            type="number"
            min="1"
            max="240"
            value={lot.lease_months}
            onChange={(event) => onChange({ lease_months: event.target.value })}
          />
        </div>
        {/* FR-205 (п. 97): почасовой лот торгуется ставкой за час */}
        <div className="flex w-44 flex-col gap-1.5">
          <Label htmlFor={`${idPrefix}-unit`}>{m.lot_rate_unit()}</Label>
          <NativeSelect
            id={`${idPrefix}-unit`}
            value={lot.rate_unit}
            onChange={(event) =>
              onChange({
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
            <Label htmlFor={`${idPrefix}-hours`}>{m.lot_hours()}</Label>
            <Input
              id={`${idPrefix}-hours`}
              required
              type="number"
              min="1"
              max="10000"
              value={lot.hours_total}
              onChange={(event) =>
                onChange({ hours_total: event.target.value })
              }
            />
          </div>
        )}
      </div>

      <div className="flex flex-col gap-1.5">
        <Label htmlFor={`${idPrefix}-viewing`}>{m.lot_viewing_terms()}</Label>
        <Input
          id={`${idPrefix}-viewing`}
          value={lot.viewing_terms}
          onChange={(event) => onChange({ viewing_terms: event.target.value })}
        />
      </div>

      <details>
        <summary className="cursor-pointer text-sm font-medium">
          {m.tender_lot_coefficients()}
        </summary>
        <div className="pt-3">
          <RateOptionsFields
            value={lot.rate_options}
            onChange={(next) => onChange({ rate_options: next })}
            idPrefix={idPrefix}
          />
        </div>
      </details>

      {onRemove !== undefined && (
        <div>
          <Button type="button" variant="ghost" size="sm" onClick={onRemove}>
            {m.tender_lot_remove()}
          </Button>
        </div>
      )}
    </fieldset>
  )
}
