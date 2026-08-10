import { useSuspenseQuery } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { formatTenge, trimZeros } from "@/lib/format"
import { COEFFICIENTS, rateOptionsQuery } from "@/lib/organizer"

import type { RateCalculation, RateOptions } from "@/lib/organizer"

// Поля и расшифровка расчета Прил. 4 (FR-201). Живут отдельно от маршрута
// калькулятора, потому что их переиспользуют формы лота и инвестиционного
// договора: маршрут не импортируется из другого маршрута (гейт G7).

/** Селекты опций по каждому множителю Прил. 4 (переиспользуется формой лота). */
export function RateOptionsFields({
  value,
  onChange,
  idPrefix,
}: {
  value: RateOptions
  onChange: (next: RateOptions) => void
  idPrefix: string
}) {
  const { data: catalog } = useSuspenseQuery(rateOptionsQuery)

  return (
    <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4">
      {COEFFICIENTS.map(([field, label]) => {
        const options = catalog.options.filter(
          (option) => option.coefficient === field
        )
        return (
          <div key={field} className="flex flex-col gap-1.5">
            <Label htmlFor={`${idPrefix}-${field}`}>{label}</Label>
            <NativeSelect
              id={`${idPrefix}-${field}`}
              value={value[field]}
              onChange={(event) =>
                onChange({ ...value, [field]: event.target.value })
              }
            >
              {options.map((option) => (
                <NativeSelectOption
                  key={option.option_code}
                  value={option.option_code}
                >
                  {option.option_code} ({option.value})
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </div>
        )
      })}
    </div>
  )
}

/** Таблица-расшифровка результата (FR-201: объяснимость). */
export function RateBreakdown({ calc }: { calc: RateCalculation }) {
  return (
    <div className="flex flex-col gap-4">
      <div className="rounded-lg border">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead scope="col">{m.calc_factor()}</TableHead>
              <TableHead scope="col">{m.calc_option()}</TableHead>
              <TableHead scope="col" className="text-right">
                {m.calc_value()}
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {COEFFICIENTS.map(([field, label]) => (
              <TableRow key={field}>
                <TableCell className="font-medium">{label}</TableCell>
                <TableCell className="text-muted-foreground">
                  {calc.factors[field].option_code}
                </TableCell>
                <TableCell className="text-right tabular-nums">
                  {calc.factors[field].value}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>

      <dl className="grid grid-cols-1 gap-3 rounded-lg border p-4 sm:grid-cols-2">
        <ResultRow label={m.calc_mrp()} value={formatTenge(calc.mrp)} />
        <ResultRow
          label={m.calc_base_rate()}
          value={formatTenge(calc.base_rate_rbs)}
        />
        <ResultRow
          label={m.calc_multiplier()}
          value={trimZeros(calc.multiplier)}
        />
        <ResultRow label={m.calc_annual()} value={formatTenge(calc.annual)} />
        <ResultRow
          label={m.calc_monthly()}
          value={formatTenge(calc.monthly)}
          emphasize
        />
        <ResultRow
          label={m.calc_fee()}
          value={formatTenge(calc.guarantee_fee)}
          emphasize
        />
      </dl>
      {!calc.vat_included && (
        <p className="text-sm text-muted-foreground">{m.calc_vat_note()}</p>
      )}
    </div>
  )
}

/** Строка результата расчета: используется и калькулятором, и почасовой формой. */
export function ResultRow({
  label,
  value,
  emphasize = false,
}: {
  label: string
  value: string
  emphasize?: boolean
}) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-sm text-muted-foreground">{label}</dt>
      <dd
        className={
          emphasize ? "font-heading text-lg font-semibold" : "font-medium"
        }
        suppressHydrationWarning
      >
        {value}
      </dd>
    </div>
  )
}
