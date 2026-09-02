import { useRef, useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { FileSignatureIcon } from "lucide-react"

import { m } from "#/paraglide/messages"
import { actKindLabel, stageLabel } from "@/components/contract-panel"
import { DepositPanel } from "@/components/deposit-panel"
import { PageHeader } from "@/components/page-header"
import { Panel } from "@/components/panel"
import { QueryBoundary } from "@/components/query-boundary"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Skeleton } from "@/components/ui/skeleton"
import { problemMessage } from "@/lib/auth"
import {
  contractActsQuery,
  contractChecklistQuery,
  myContractsQuery,
  uploadContractScan,
} from "@/lib/contracts"
import { formatDate, formatDateTime, formatTenge } from "@/lib/format"
import { serverLabel } from "@/lib/server-label"
import { notifyError, notifySuccess } from "@/lib/toast"
import { cn } from "@/lib/utils"

import type { ContractStage } from "@/components/contract-panel"
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
  head: () => ({ meta: [{ title: `${m.my_contracts_title()} - ToU Rent` }] }),
  component: MyContractsPage,
})

/**
 * Порядок шагов конвейера п. 110–115.
 *
 * Союз и подписи шагов - общие с панелью организатора (`contract-panel`):
 * два разных русских названия одного и того же шага были бы расхождением
 * между кабинетами. Здесь добавляется только порядок, и добавляется
 * исчерпывающим `Record`: новый шаг домена ломает typecheck тут, а не
 * выпадает из индикатора молча.
 */
const STAGE_SEQUENCE: Record<ContractStage, number> = {
  drafted: 0,
  handed_to_tenant: 1,
  tenant_signed: 2,
  documents_received: 3,
  checklist_completed: 4,
  landlord_signed: 5,
  copy_sent: 6,
  registered: 7,
}

const STAGES = (Object.keys(STAGE_SEQUENCE) as ContractStage[]).toSorted(
  (left, right) => STAGE_SEQUENCE[left] - STAGE_SEQUENCE[right]
)

function isStage(value: string | null | undefined): value is ContractStage {
  return value != null && value in STAGE_SEQUENCE
}

function MyContractsPage() {
  const contracts = useQuery(myContractsQuery)

  return (
    <div className="flex flex-col gap-6">
      <PageHeader title={m.my_contracts_title()} />

      <QueryBoundary
        query={contracts}
        empty={{
          when: (page) => page.items.length === 0,
          icon: FileSignatureIcon,
          title: m.my_contracts_empty(),
          description: m.participant_contracts_empty_hint(),
        }}
      >
        {(page) => (
          <>
            {page.truncated && (
              <p
                role="status"
                className="rounded-lg bg-amber-500/10 px-3 py-2 text-sm text-amber-700 ring-1 ring-amber-500/30 dark:text-amber-400"
                data-testid="contracts-truncated"
              >
                {m.list_truncated({ count: page.items.length })}
              </p>
            )}
            <ul className="flex flex-col gap-6">
              {page.items.map((contract) => (
                <li key={contract.id}>
                  <ContractCard contract={contract} />
                </li>
              ))}
            </ul>
          </>
        )}
      </QueryBoundary>
    </div>
  )
}

function ContractCard({ contract }: { contract: ContractDto }) {
  const queryClient = useQueryClient()
  const checklist = useQuery(contractChecklistQuery(contract.id))
  const acts = useQuery(contractActsQuery(contract.id))
  const fileInput = useRef<HTMLInputElement>(null)
  // Отказ остается на экране рядом с формой: тост исчезает, а причина
  // отказа нужна тому, кто будет выбирать файл заново
  const [scanError, setScanError] = useState<string | undefined>(undefined)

  const upload = useMutation({
    mutationFn: (file: File) => uploadContractScan(contract.id, file),
    onSuccess: async () => {
      setScanError(undefined)
      if (fileInput.current !== null) fileInput.current.value = ""
      notifySuccess(m.contract_scan_uploaded())
      await queryClient.invalidateQueries({
        queryKey: myContractsQuery.queryKey,
      })
    },
    onError: (error: unknown) => {
      const message = problemMessage(error)
      setScanError(message)
      notifyError(message)
    },
  })

  return (
    <Panel
      title={m.contract_card_title({ id: contract.id.slice(0, 8) })}
      description={`${contract.object_name} · ${formatTenge(contract.monthly_rate)}`}
      contentClassName="flex flex-col gap-5"
    >
      <ContractStages stage={contract.stage} />

      <dl className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <div className="flex flex-col gap-0.5">
          <dt className="text-sm text-muted-foreground">
            {m.contract_next_stage()}
          </dt>
          <dd className="font-medium">
            {isStage(contract.next_stage)
              ? stageLabel(contract.next_stage)
              : "-"}
          </dd>
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
      <section aria-label={m.contract_checklist_title()}>
        <h3 className="mb-2 font-medium">{m.contract_checklist_title()}</h3>
        <QueryBoundary
          query={checklist}
          skeleton={<RowsSkeleton />}
          empty={{
            when: (items) => items.length === 0,
            title: m.contract_checklist_empty(),
          }}
        >
          {(items) => (
            <ul className="flex flex-col gap-1 text-sm">
              {items.map((item) => (
                <li
                  key={item.item_code}
                  className="flex flex-wrap justify-between gap-2"
                >
                  <span>{serverLabel(item)}</span>
                  <span
                    className={
                      item.checked
                        ? "text-emerald-700 dark:text-emerald-400"
                        : "text-destructive"
                    }
                  >
                    {item.checked
                      ? m.contract_checklist_done()
                      : m.contract_checklist_pending()}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </QueryBoundary>
      </section>

      {/* Подписанный экземпляр возвращает наниматель (п. 111) */}
      <section aria-label={m.contract_scan_label()}>
        <h3 className="mb-2 font-medium">{m.contract_scan_label()}</h3>
        {contract.has_scan ? (
          <p className="text-sm text-muted-foreground">
            {m.contract_scan_uploaded()}
          </p>
        ) : (
          <form
            className="flex flex-wrap items-end gap-2"
            onSubmit={(event) => {
              event.preventDefault()
              const file = fileInput.current?.files?.[0]
              // Пустая форма - не отказ сервера: сообщение свое и локализованное
              if (file === undefined) {
                setScanError(m.file_not_selected())
                return
              }
              setScanError(undefined)
              upload.mutate(file)
            }}
          >
            <div className="flex flex-col gap-1.5">
              <Label htmlFor={`scan-${contract.id}`}>
                {m.contract_scan_label()}
              </Label>
              <Input
                id={`scan-${contract.id}`}
                type="file"
                ref={fileInput}
                aria-invalid={scanError !== undefined}
              />
            </div>
            <Button type="submit" disabled={upload.isPending}>
              {m.contract_scan_cta()}
            </Button>
          </form>
        )}
        {scanError !== undefined && (
          <p role="alert" className="mt-2 text-sm text-destructive">
            {scanError}
          </p>
        )}
      </section>

      {/* Депозит по договору (FR-1003): наниматель видит, финблок движет */}
      <DepositPanel contractId={contract.id} canManage={false} />

      <section aria-label={m.contract_acts_title()}>
        <h3 className="mb-2 font-medium">{m.contract_acts_title()}</h3>
        <QueryBoundary
          query={acts}
          skeleton={<RowsSkeleton />}
          empty={{
            when: (items) => items.length === 0,
            title: m.contract_acts_empty(),
          }}
        >
          {(items) => (
            <ul className="flex flex-col gap-1 text-sm">
              {items.map((act) => (
                <li
                  key={act.id}
                  className="flex flex-wrap items-center justify-between gap-2"
                >
                  <span>{actKindLabel(act.kind)}</span>
                  <span
                    className="text-muted-foreground"
                    suppressHydrationWarning
                  >
                    {formatDate(act.act_date)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </QueryBoundary>
      </section>
    </Panel>
  )
}

/**
 * Индикатор конвейера п. 110–115: цепочка шагов с отметкой текущего.
 *
 * Голое `handed_to_tenant` в поле «Шаг конвейера» ничего не говорило
 * нанимателю ни о том, что уже сделано, ни о том, сколько осталось.
 * Порядок шагов - это и есть содержание п. 110–115, поэтому он показан
 * списком, а не одной строкой.
 */
function ContractStages({ stage }: { stage: string | null | undefined }) {
  const current = isStage(stage) ? stage : null
  const reached = current == null ? -1 : STAGE_SEQUENCE[current]

  return (
    <section aria-label={m.contract_stage_label()}>
      <div className="mb-2">
        <h3 className="font-medium">{m.contract_stage_label()}</h3>
      </div>
      <ol className="flex flex-col gap-1.5 sm:flex-row sm:flex-wrap sm:items-center">
        {STAGES.map((item, index) => {
          const done = STAGE_SEQUENCE[item] < reached
          const active = item === current

          return (
            <li
              key={item}
              aria-current={active ? "step" : undefined}
              className={cn(
                "flex items-center gap-1.5 rounded-lg border px-2 py-1 text-sm",
                done &&
                  "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400",
                active &&
                  "border-primary/25 bg-primary/10 font-medium text-primary",
                !done &&
                  !active &&
                  "border-border bg-muted/50 text-muted-foreground"
              )}
            >
              <span className="tabular-nums opacity-70">{index + 1}</span>
              {stageLabel(item)}
            </li>
          )
        })}
      </ol>
    </section>
  )
}

function RowsSkeleton() {
  return (
    <div className="flex flex-col gap-1.5" aria-hidden="true">
      <Skeleton className="h-5 w-full rounded-md" />
      <Skeleton className="h-5 w-4/5 rounded-md" />
    </div>
  )
}
