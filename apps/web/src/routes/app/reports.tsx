import { useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useQuery, useSuspenseQuery } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { buttonVariants } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { registriesQuery, registryCsvHref, registryQuery } from "@/lib/reports"
import { cn } from "@/lib/utils"

// Отчетность (арх. § 9): реестры решений, договоров и поступлений за период
// с выгрузкой CSV. Реестр - выборка записанных фактов, а не форма отчета:
// состав отчетности заказчик не задавал (Q-012, A-079).
export const Route = createFileRoute("/app/reports")({
  loader: ({ context }) => context.queryClient.ensureQueryData(registriesQuery),
  component: ReportsPage,
})

function ReportsPage() {
  const { data: registries } = useSuspenseQuery(registriesQuery)
  const [registry, setRegistry] = useState(registries[0]?.registry ?? "")
  const [from, setFrom] = useState("")
  const [to, setTo] = useState("")

  const { data } = useQuery({
    ...registryQuery(registry, from, to),
    enabled: registry !== "",
  })

  if (registries.length === 0) {
    return (
      <div className="mx-auto w-full max-w-6xl px-6 py-8">
        <p className="text-muted-foreground">{m.reports_forbidden()}</p>
      </div>
    )
  }

  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-6 py-8">
      <div className="flex flex-col gap-1">
        <h1 className="font-heading text-2xl font-semibold">
          {m.reports_title()}
        </h1>
        <p className="text-sm text-muted-foreground">{m.reports_hint()}</p>
      </div>

      <form
        className="flex flex-wrap items-end gap-3 rounded-lg border p-4"
        onSubmit={(event) => event.preventDefault()}
      >
        <div className="flex min-w-56 flex-col gap-1.5">
          <Label htmlFor="registry">{m.reports_registry_label()}</Label>
          <NativeSelect
            id="registry"
            value={registry}
            onChange={(event) => setRegistry(event.target.value)}
          >
            {registries.map((item) => (
              <NativeSelectOption key={item.registry} value={item.registry}>
                {item.title_ru}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </div>
        <div className="flex w-44 flex-col gap-1.5">
          <Label htmlFor="from">{m.reports_from_label()}</Label>
          <Input
            id="from"
            type="date"
            value={from}
            onChange={(event) => setFrom(event.target.value)}
          />
        </div>
        <div className="flex w-44 flex-col gap-1.5">
          <Label htmlFor="to">{m.reports_to_label()}</Label>
          <Input
            id="to"
            type="date"
            value={to}
            onChange={(event) => setTo(event.target.value)}
          />
        </div>
        <a
          href={registryCsvHref(registry, from, to)}
          className={cn(buttonVariants({ variant: "outline" }))}
          data-testid="registry-csv"
        >
          {m.reports_export()}
        </a>
      </form>

      {data === undefined ? null : data.rows.length === 0 ? (
        <p className="text-muted-foreground">{m.reports_empty()}</p>
      ) : (
        <div className="overflow-x-auto rounded-lg border">
          <table className="w-full text-sm" data-testid="registry-table">
            <thead className="bg-muted/50">
              <tr>
                {data.columns.map((column) => (
                  <th key={column} className="px-3 py-2 text-left font-medium">
                    {column}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {data.rows.map((row, index) => (
                // Строка реестра - это факт БД; ключом служит ее место
                // в выборке: сервер отдает уже отформатированные значения
                // eslint-disable-next-line react/no-array-index-key
                <tr key={index} className="border-t">
                  {row.map((value, cell) => (
                    <td key={cell} className="px-3 py-2 align-top">
                      {value}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}
