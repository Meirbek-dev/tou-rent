import { useRef, useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { m } from "#/paraglide/messages"
import { DepositPanel } from "@/components/deposit-panel"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { problemMessage } from "@/lib/auth"
import {
  contractActsQuery,
  contractChecklistQuery,
  myContractsQuery,
  uploadContractScan,
} from "@/lib/contracts"
import { formatDateTime, formatTenge } from "@/lib/format"

import type { ContractDto } from "@/lib/contracts"

/**
 * Кабинет нанимателя (FR-902, FR-1003).
 *
 * Конвейер п. 110–115 наполовину принадлежит нанимателю: он возвращает
 * подписанный договор (п. 111) и представляет документы для сверки
 * (п. 112), и система считает по этим шагам сроки. До сих пор экрана
 * у этой стороны не было вовсе - все отмечал за нее организатор, а
 * эндпоинт `/contracts/my` существовал, но никем не вызывался.
 */
export const Route = createFileRoute("/app/participant/contracts")({
  loader: async ({ context }) => {
    await context.queryClient.ensureQueryData(myContractsQuery)
  },
  component: MyContractsPage,
})

function MyContractsPage() {
  const { data: contracts } = useQuery(myContractsQuery)

  return (
    <div className="flex flex-col gap-6">
      <h2 className="font-heading text-lg font-semibold">
        {m.my_contracts_title()}
      </h2>

      {contracts == null || contracts.length === 0 ? (
        <p className="text-muted-foreground">{m.my_contracts_empty()}</p>
      ) : (
        <ul className="flex flex-col gap-6">
          {contracts.map((contract) => (
            <li key={contract.id}>
              <ContractCard contract={contract} />
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

function ContractCard({ contract }: { contract: ContractDto }) {
  const queryClient = useQueryClient()
  const { data: checklist } = useQuery(contractChecklistQuery(contract.id))
  const { data: acts } = useQuery(contractActsQuery(contract.id))
  const fileInput = useRef<HTMLInputElement>(null)
  const [scanError, setScanError] = useState<unknown>(null)

  const upload = useMutation({
    mutationFn: async () => {
      const file = fileInput.current?.files?.[0]
      if (file === undefined) throw new Error("файл не выбран")
      return uploadContractScan(contract.id, file)
    },
    onSuccess: async () => {
      setScanError(null)
      if (fileInput.current !== null) fileInput.current.value = ""
      await queryClient.invalidateQueries({
        queryKey: myContractsQuery.queryKey,
      })
    },
    onError: (error) => setScanError(error),
  })

  return (
    <article className="flex flex-col gap-4 rounded-lg border p-4">
      <header className="flex flex-wrap items-baseline justify-between gap-2">
        <h3 className="font-heading text-base font-semibold">
          {m.contract_card_title({ id: contract.id.slice(0, 8) })}
        </h3>
        <span className="text-sm text-muted-foreground">
          {contract.object_name} · {formatTenge(contract.monthly_rate)}
        </span>
      </header>

      <dl className="grid gap-3 sm:grid-cols-3">
        <div className="flex flex-col gap-0.5">
          <dt className="text-sm text-muted-foreground">
            {m.contract_stage_label()}
          </dt>
          <dd className="font-medium">
            {contract.stage ?? "-"}
            {contract.stage_rule_ref != null && (
              <span className="ml-2 text-sm font-normal text-muted-foreground">
                {contract.stage_rule_ref}
              </span>
            )}
          </dd>
        </div>
        <div className="flex flex-col gap-0.5">
          <dt className="text-sm text-muted-foreground">
            {m.contract_next_stage()}
          </dt>
          <dd className="font-medium">{contract.next_stage ?? "-"}</dd>
        </div>
        <div className="flex flex-col gap-0.5">
          <dt className="text-sm text-muted-foreground">
            {m.contract_reg_number()}
          </dt>
          <dd className="font-medium" suppressHydrationWarning>
            {contract.reg_number ?? "-"}
            {contract.registered_at != null && (
              <span className="ml-2 text-sm font-normal text-muted-foreground">
                {formatDateTime(contract.registered_at)}
              </span>
            )}
          </dd>
        </div>
      </dl>

      {/* Перечень сверки п. 113: наниматель видит, чего от него ждут */}
      <section className="flex flex-col gap-2">
        <h4 className="font-medium">{m.contract_checklist_title()}</h4>
        <ul className="flex flex-col gap-1 text-sm">
          {(checklist ?? []).map((item) => (
            <li
              key={item.item_code}
              className="flex flex-wrap justify-between gap-2"
            >
              <span>{item.label_ru}</span>
              <span
                className={
                  item.checked ? "text-muted-foreground" : "text-destructive"
                }
              >
                {item.checked
                  ? m.contract_checklist_done()
                  : m.contract_checklist_pending()}
              </span>
            </li>
          ))}
        </ul>
      </section>

      {/* Подписанный экземпляр возвращает наниматель (п. 111) */}
      <section className="flex flex-col gap-2">
        <h4 className="font-medium">{m.contract_scan_label()}</h4>
        {contract.has_scan ? (
          <p className="text-sm text-muted-foreground">
            {m.contract_scan_uploaded()}
          </p>
        ) : (
          <div className="flex flex-wrap items-end gap-2">
            <div className="flex flex-col gap-1.5">
              <Label htmlFor={`scan-${contract.id}`}>
                {m.contract_scan_label()}
              </Label>
              <Input id={`scan-${contract.id}`} type="file" ref={fileInput} />
            </div>
            <Button onClick={() => upload.mutate()} disabled={upload.isPending}>
              {m.contract_scan_cta()}
            </Button>
          </div>
        )}
        {scanError != null && (
          <p role="alert" className="text-sm text-destructive">
            {problemMessage(scanError)}
          </p>
        )}
      </section>

      {/* Депозит по договору (FR-1003): наниматель видит, финблок движет */}
      <DepositPanel contractId={contract.id} canManage={false} />

      <section className="flex flex-col gap-2">
        <h4 className="font-medium">{m.contract_acts_title()}</h4>
        {acts == null || acts.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {m.contract_acts_empty()}
          </p>
        ) : (
          <ul className="flex flex-col gap-1 text-sm">
            {acts.map((act) => (
              <li key={act.id} className="flex flex-wrap justify-between gap-2">
                <span>{act.kind}</span>
                <span className="text-muted-foreground">{act.act_date}</span>
              </li>
            ))}
          </ul>
        )}
      </section>
    </article>
  )
}
