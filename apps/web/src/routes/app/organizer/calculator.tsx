import { useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useMutation } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import {
  RateBreakdown,
  RateOptionsFields,
  ResultRow,
} from "@/components/rate-fields"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { api } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import { formatTenge, trimZeros } from "@/lib/format"
import { defaultRateOptions, rateOptionsQuery } from "@/lib/organizer"

import type { RateOptions } from "@/lib/organizer"

// FR-201: форма → RateCalculation с расшифровкой; тот же серверный код
// замораживает снимок в лоте (FR-202) - калькулятор не может разойтись с лотом.
export const Route = createFileRoute("/app/organizer/calculator")({
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(rateOptionsQuery),
  component: CalculatorPage,
})

function CalculatorPage() {
  const [area, setArea] = useState("42")
  const [options, setOptions] = useState<RateOptions>(defaultRateOptions())

  const preview = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST("/api/v1/rates/preview", {
        body: { area_m2: area, options },
      })
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("preview failed")
      }
      return data
    },
  })

  // FR-205 (п. 97): почасовая ставка считается от 2 МРП/час и не зависит
  // от площади - отдельный расчет, а не строка в годовом
  const hourly = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST("/api/v1/rates/preview-hourly", {
        body: { options },
      })
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("hourly preview failed")
      }
      return data
    },
  })

  return (
    <div className="flex flex-col gap-6">
      <form
        className="flex flex-col gap-4 rounded-lg border p-4"
        onSubmit={(event) => {
          event.preventDefault()
          preview.mutate()
        }}
      >
        <div className="flex w-40 flex-col gap-1.5">
          <Label htmlFor="calc-area">{m.object_area_label()}</Label>
          <Input
            id="calc-area"
            required
            type="number"
            min="0.01"
            step="0.01"
            value={area}
            onChange={(event) => setArea(event.target.value)}
          />
        </div>
        <RateOptionsFields
          value={options}
          onChange={setOptions}
          idPrefix="calc"
        />
        <div className="flex flex-wrap gap-3">
          <Button
            type="submit"
            data-testid="calc-submit"
            disabled={preview.isPending}
          >
            {m.calc_submit()}
          </Button>
          <Button
            type="button"
            variant="outline"
            data-testid="calc-hourly"
            disabled={hourly.isPending}
            onClick={() => hourly.mutate()}
          >
            {m.calc_submit_hourly()}
          </Button>
        </div>
        {(preview.isError || hourly.isError) && (
          <p role="alert" className="text-sm text-destructive">
            {problemMessage(preview.error ?? hourly.error)}
          </p>
        )}
      </form>

      {preview.data !== undefined && <RateBreakdown calc={preview.data} />}

      {hourly.data !== undefined && (
        <section
          aria-labelledby="hourly-rate"
          className="flex flex-col gap-3 rounded-lg border p-4"
          data-testid="hourly-breakdown"
        >
          <h3 id="hourly-rate" className="font-heading text-lg font-semibold">
            {m.calc_hourly_title()}
          </h3>
          <dl className="grid grid-cols-1 gap-3 sm:grid-cols-3">
            <ResultRow
              label={m.calc_hourly_floor()}
              value={formatTenge(hourly.data.floor)}
            />
            <ResultRow
              label={m.calc_multiplier()}
              value={trimZeros(hourly.data.multiplier)}
            />
            <ResultRow
              label={m.calc_hourly_rate()}
              value={formatTenge(hourly.data.hourly)}
              emphasize
            />
          </dl>
          <p className="text-sm text-muted-foreground">
            {hourly.data.floor_applied
              ? m.calc_hourly_floor_applied()
              : m.calc_hourly_hint()}
          </p>
        </section>
      )}
    </div>
  )
}
