import { useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useQuery, useSuspenseQuery } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { EmptyState } from "@/components/empty-state"
import { PageHeader } from "@/components/page-header"
import { PageShell } from "@/components/page-shell"
import { QueryBoundary } from "@/components/query-boundary"
import { buttonVariants } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { Skeleton } from "@/components/ui/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { registriesQuery, registryCsvHref, registryQuery } from "@/lib/reports"
import { serverLabel } from "@/lib/server-label"
import { cn } from "@/lib/utils"
import { LockIcon, TableIcon } from "lucide-react"

// Отчетность (арх. § 9): реестры решений, договоров и поступлений за период
// с выгрузкой CSV. Реестр - выборка записанных фактов, а не форма отчета:
// состав отчетности заказчик не задавал (Q-012, A-079).
export const Route = createFileRoute("/app/reports")({
  loader: ({ context }) => context.queryClient.ensureQueryData(registriesQuery),
  head: () => ({ meta: [{ title: `${m.reports_title()} - ToU Rent` }] }),
  component: ReportsPage,
})

function ReportsPage() {
  const { data: registries } = useSuspenseQuery(registriesQuery)
  const [registry, setRegistry] = useState(registries[0]?.registry ?? "")
  const [from, setFrom] = useState("")
  const [to, setTo] = useState("")

  const query = useQuery({
    ...registryQuery(registry, from, to),
    enabled: registry !== "",
  })

  // Пустой перечень реестров - это отказ доступа, а не «нет данных»:
  // право на реестр дает не отчетность сама по себе, а область, срезом
  // которой она является (INV-POL-01)
  if (registries.length === 0) {
    return (
      <PageShell>
        <PageHeader title={m.reports_title()} />
        <EmptyState
          icon={LockIcon}
          title={m.reports_forbidden_title()}
          description={m.reports_forbidden()}
        />
      </PageShell>
    )
  }

  return (
    <PageShell>
      <PageHeader title={m.reports_title()} description={m.reports_hint()} />

      <form
        className="flex flex-wrap items-end gap-3 rounded-lg border p-4"
        onSubmit={(event) => event.preventDefault()}
      >
        <div className="flex w-full flex-col gap-1.5 sm:w-56">
          <Label htmlFor="registry">{m.reports_registry_label()}</Label>
          <NativeSelect
            id="registry"
            className="w-full"
            value={registry}
            onChange={(event) => setRegistry(event.target.value)}
          >
            {registries.map((item) => (
              <NativeSelectOption key={item.registry} value={item.registry}>
                {serverLabel(item, "title")}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </div>
        <div className="flex w-full flex-col gap-1.5 sm:w-44">
          <Label htmlFor="from">{m.reports_from_label()}</Label>
          <Input
            id="from"
            type="date"
            value={from}
            onChange={(event) => setFrom(event.target.value)}
          />
        </div>
        <div className="flex w-full flex-col gap-1.5 sm:w-44">
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

      <QueryBoundary
        query={query}
        skeleton={<Skeleton className="h-64 w-full rounded-xl" />}
        empty={{
          when: (data) => data.rows.length === 0,
          icon: TableIcon,
          title: m.reports_empty_title(),
          description: m.reports_empty(),
        }}
      >
        {(data) => (
          <div className="flex flex-col gap-3">
            {/* Экран реестра - первая страница выборки, выгрузка - весь период.
                Бухгалтерия считает по строкам, поэтому «показано не все» стоит
                над таблицей, а не сноской под ней */}
            {data.truncated && (
              <p
                role="status"
                className="rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-sm text-amber-700 dark:text-amber-400"
                data-testid="registry-truncated"
              >
                {m.reports_truncated({ count: data.rows.length })}
              </p>
            )}

            <div className="rounded-lg border">
              <Table
                data-testid="registry-table"
                aria-label={serverLabel(data, "title")}
              >
                <TableHeader>
                  <TableRow>
                    {data.columns.map((column) => (
                      <TableHead key={column}>{column}</TableHead>
                    ))}
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {data.rows.map((row) => (
                    // Строка реестра - факт БД, но идентификатора в ответе нет:
                    // сервер отдает уже отформатированные значения. Ключом
                    // служит само содержимое - оно и отличает факт от факта,
                    // а место в выборке меняется при любой смене периода
                    <TableRow key={row.join("")}>
                      {row.map((value, cell) => (
                        <TableCell
                          key={data.columns[cell] ?? String(cell)}
                          className="align-top whitespace-normal"
                        >
                          {value}
                        </TableCell>
                      ))}
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          </div>
        )}
      </QueryBoundary>
    </PageShell>
  )
}
