import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { ContractAmendmentsPanel } from "@/components/contract-amendments-panel"
import { DepositPanel } from "@/components/deposit-panel"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { problemMessage } from "@/lib/auth"
import {
  advanceContract,
  checkChecklistItem,
  contractActsQuery,
  contractChecklistQuery,
  createAct,
  draftContract,
  registerContract,
  tenderContractsQuery,
} from "@/lib/contracts"
import {
  declareEvasion,
  evasionGroundsQuery,
  tenderEvasionsQuery,
} from "@/lib/evasion"
import { formatDateTime, formatTenge } from "@/lib/format"

import type { ContractDto } from "@/lib/contracts"

/**
 * Договорный конвейер (FR-901–902, FR-905, INV-115): существенные условия
 * только показываются, шаги п. 110–115 идут по порядку, подпись наймодателя
 * недоступна, пока не отмечен весь перечень сверки.
 */
export function ContractPanel({
  tenderId,
  lots = [],
  canDraft = false,
  canManageDeposit = false,
}: {
  tenderId: string
  /** Лоты тендера: по каждому с итогами торгов составляется договор (п. 108) */
  lots?: { id: string; seq: number }[]
  canDraft?: boolean
  /** Проводки депозита оформляет только финблок (FR-1001, FR-1003) */
  canManageDeposit?: boolean
}) {
  const queryClient = useQueryClient()
  const { data: contracts } = useQuery(tenderContractsQuery(tenderId))

  const draft = useMutation({
    mutationFn: (lotId: string) => draftContract(lotId),
    onSuccess: () =>
      queryClient.invalidateQueries({
        queryKey: tenderContractsQuery(tenderId).queryKey,
      }),
  })

  if (contracts === undefined) return null

  // Договор составляется по лоту, у которого нет действующего: прекращенный
  // уклонением места не занимает (FR-903, п. 117)
  const pending = canDraft
    ? lots.filter(
        (lot) =>
          !contracts.some(
            (contract) => contract.lot_id === lot.id && !contract.evaded
          )
      )
    : []

  if (contracts.length === 0 && pending.length === 0) return null

  return (
    <section aria-labelledby="contracts" className="flex flex-col gap-4">
      <h3 id="contracts" className="font-heading text-lg font-semibold">
        {m.contracts_title()}
      </h3>

      {pending.length > 0 && (
        <div className="flex flex-wrap items-center gap-3">
          {pending.map((lot) => (
            <Button
              key={lot.id}
              variant="outline"
              size="sm"
              data-testid="draft-contract"
              disabled={draft.isPending}
              onClick={() => draft.mutate(lot.id)}
            >
              {m.contract_draft({ seq: lot.seq })}
            </Button>
          ))}
        </div>
      )}
      {draft.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(draft.error)}
        </p>
      )}

      {contracts.map((contract) => (
        <ContractCard
          key={contract.id}
          contract={contract}
          tenderId={tenderId}
          canManageDeposit={canManageDeposit}
        />
      ))}
    </section>
  )
}

function ContractCard({
  contract,
  tenderId,
  canManageDeposit,
}: {
  contract: ContractDto
  tenderId: string
  /** Проводки депозита оформляет финблок (FR-1001) */
  canManageDeposit: boolean
}) {
  const queryClient = useQueryClient()
  const [regNumber, setRegNumber] = useState("")
  const { data: checklist } = useQuery(contractChecklistQuery(contract.id))

  const refresh = async () => {
    await Promise.all([
      queryClient.invalidateQueries({
        queryKey: tenderContractsQuery(tenderId).queryKey,
      }),
      queryClient.invalidateQueries({
        queryKey: contractChecklistQuery(contract.id).queryKey,
      }),
    ])
  }

  const advance = useMutation({
    mutationFn: (stage: string) => advanceContract(contract.id, stage),
    onSuccess: refresh,
  })
  const check = useMutation({
    mutationFn: ({ code, checked }: { code: string; checked: boolean }) =>
      checkChecklistItem(contract.id, code, checked),
    onSuccess: refresh,
  })
  const register = useMutation({
    mutationFn: () => registerContract(contract.id, regNumber),
    onSuccess: refresh,
  })

  const error = advance.error ?? check.error ?? register.error

  return (
    <article
      className="flex flex-col gap-3 rounded-lg border p-4"
      data-testid="contract-card"
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-col">
          <span className="font-medium">{contract.object_name}</span>
          <span className="text-sm text-muted-foreground">
            {contract.tenant_name}
            {contract.lot_seq != null &&
              ` - ${m.lot_seq()} ${contract.lot_seq}`}
            {contract.place === "runner_up" &&
              ` - ${m.contract_place_second()}`}
          </span>
        </div>
        <div className="flex flex-col items-end">
          <span className="font-medium">
            {formatTenge(contract.monthly_rate)}
          </span>
          <span className="text-xs text-muted-foreground">
            {m.contract_terms_frozen()}
          </span>
        </div>
      </div>

      <dl className="grid grid-cols-1 gap-x-6 gap-y-1 text-sm sm:grid-cols-2">
        <div className="flex gap-2">
          <dt className="text-muted-foreground">{m.contract_stage_label()}:</dt>
          <dd data-testid="contract-stage">
            {stageLabel(contract.stage)}
            {contract.stage_rule_ref != null && ` (${contract.stage_rule_ref})`}
          </dd>
        </div>
        <div className="flex gap-2">
          <dt className="text-muted-foreground">{m.contract_reg_label()}:</dt>
          <dd suppressHydrationWarning>
            {contract.reg_number ??
              (contract.registered_at != null
                ? formatDateTime(contract.registered_at)
                : m.contract_not_registered())}
          </dd>
        </div>
      </dl>

      {checklist !== undefined && checklist.length > 0 && (
        <ul className="flex flex-col gap-1 text-sm" data-testid="checklist">
          {checklist.map((item) => (
            <li key={item.item_code} className="flex items-center gap-2">
              <input
                id={`check-${contract.id}-${item.item_code}`}
                type="checkbox"
                checked={item.checked}
                disabled={check.isPending}
                onChange={(event) =>
                  check.mutate({
                    code: item.item_code,
                    checked: event.target.checked,
                  })
                }
              />
              <label htmlFor={`check-${contract.id}-${item.item_code}`}>
                {item.label_ru}{" "}
                <span className="text-muted-foreground">({item.rule_ref})</span>
              </label>
            </li>
          ))}
        </ul>
      )}

      <div className="flex flex-wrap items-center gap-3">
        {contract.next_stage != null &&
          contract.next_stage !== "registered" && (
            <Button
              variant="outline"
              size="sm"
              data-testid="advance-contract"
              disabled={advance.isPending}
              onClick={() => advance.mutate(contract.next_stage as string)}
            >
              {m.contract_advance({ stage: stageLabel(contract.next_stage) })}
            </Button>
          )}
        {contract.has_pdf && (
          <a
            href={`/api/v1/contracts/${contract.id}/pdf`}
            className="text-sm underline-offset-4 hover:underline"
          >
            {m.contract_pdf()}
          </a>
        )}
        {contract.next_stage === "registered" && (
          <form
            className="flex items-center gap-2"
            onSubmit={(event) => {
              event.preventDefault()
              register.mutate()
            }}
          >
            <Input
              aria-label={m.contract_reg_number_label()}
              className="max-w-48"
              placeholder={m.contract_reg_number_label()}
              value={regNumber}
              onChange={(event) => setRegNumber(event.target.value)}
            />
            <Button
              type="submit"
              size="sm"
              data-testid="register-contract"
              disabled={register.isPending || regNumber === ""}
            >
              {m.contract_register()}
            </Button>
          </form>
        )}
      </div>

      {contract.evaded ? (
        <p className="border-t pt-3 text-sm text-destructive">
          {m.evasion_contract_terminated()}
        </p>
      ) : (
        contract.stage === "handed_to_tenant" && (
          <EvasionSection contractId={contract.id} tenderId={tenderId} />
        )
      )}

      {contract.registered_at != null && (
        <ActsSection contractId={contract.id} tenderId={tenderId} />
      )}

      {/* FR-1003 (п. 132–136): депозит по заключенному договору. Движение
          денег оформляет финблок, здесь оно видно ведущим процесс */}
      {contract.registered_at != null && (
        <DepositPanel contractId={contract.id} canManage={canManageDeposit} />
      )}

      {/* FR-906 (п. 125): допсоглашение - к заключенному договору */}
      {contract.registered_at != null && (
        <ContractAmendmentsPanel contractId={contract.id} canAmend />
      )}

      {error != null && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(error)}
        </p>
      )}
    </article>
  )
}

/**
 * Уклонение от подписания договора (FR-903, п. 116): признается, пока
 * экземпляр передан, а подписанный не возвращен. Взнос удерживается,
 * договор прекращается, право на договор переходит к участнику № 2.
 */
function EvasionSection({
  contractId,
  tenderId,
}: {
  contractId: string
  tenderId: string
}) {
  const queryClient = useQueryClient()
  const { data: grounds } = useQuery(evasionGroundsQuery)
  const [ground, setGround] = useState("")

  const declare = useMutation({
    mutationFn: () => declareEvasion(contractId, ground),
    onSuccess: async () => {
      setGround("")
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: tenderContractsQuery(tenderId).queryKey,
        }),
        queryClient.invalidateQueries({
          queryKey: tenderEvasionsQuery(tenderId).queryKey,
        }),
      ])
    },
  })

  return (
    <div className="flex flex-col gap-2 border-t pt-3">
      <h4 className="font-medium">{m.evasion_declare_title()}</h4>
      <div className="flex flex-wrap items-center gap-2">
        <NativeSelect
          aria-label={m.evasion_ground_label()}
          value={ground}
          onChange={(event) => setGround(event.target.value)}
        >
          <NativeSelectOption value="">
            {m.evasion_ground_label()}
          </NativeSelectOption>
          {(grounds ?? []).map((item) => (
            <NativeSelectOption key={item.code} value={item.code}>
              {item.label_ru} ({item.rule_ref})
            </NativeSelectOption>
          ))}
        </NativeSelect>
        <Button
          variant="destructive"
          size="sm"
          data-testid="declare-evasion"
          disabled={declare.isPending || ground === ""}
          onClick={() => declare.mutate()}
        >
          {m.evasion_declare()}
        </Button>
      </div>
      {declare.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(declare.error)}
        </p>
      )}
    </div>
  )
}

/**
 * Акты приема-передачи и возврата (FR-904, Прил. 7–8): с даты передачи
 * начисляется плата, возврат закрывает договор и освобождает объект.
 */
function ActsSection({
  contractId,
  tenderId,
}: {
  contractId: string
  tenderId: string
}) {
  const queryClient = useQueryClient()
  const { data: acts } = useQuery(contractActsQuery(contractId))
  const [actDate, setActDate] = useState("")

  const create = useMutation({
    mutationFn: (kind: string) => createAct(contractId, kind, actDate),
    onSuccess: async () => {
      setActDate("")
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: contractActsQuery(contractId).queryKey,
        }),
        queryClient.invalidateQueries({
          queryKey: tenderContractsQuery(tenderId).queryKey,
        }),
      ])
    },
  })

  const kinds = new Set((acts ?? []).map((act) => act.kind))

  return (
    <div className="flex flex-col gap-2 border-t pt-3">
      <h4 className="font-medium">{m.acts_title()}</h4>
      {acts !== undefined && acts.length > 0 && (
        <ul className="flex flex-col gap-1 text-sm" data-testid="acts">
          {acts.map((act) => (
            <li key={act.id} className="flex flex-wrap items-center gap-x-3">
              <span>{act.title_ru}</span>
              <span className="text-muted-foreground">{act.appendix}</span>
              <span suppressHydrationWarning>{act.act_date}</span>
              {act.has_pdf && (
                <a
                  href={`/api/v1/acts/${act.id}/pdf`}
                  className="underline-offset-4 hover:underline"
                >
                  {m.acts_pdf()}
                </a>
              )}
            </li>
          ))}
        </ul>
      )}
      {!kinds.has("return") && (
        <div className="flex flex-wrap items-center gap-2">
          <Input
            type="date"
            aria-label={m.acts_date_label()}
            className="max-w-44"
            value={actDate}
            onChange={(event) => setActDate(event.target.value)}
          />
          <Button
            variant="outline"
            size="sm"
            data-testid="create-act"
            disabled={create.isPending || actDate === ""}
            onClick={() =>
              create.mutate(kinds.has("handover") ? "return" : "handover")
            }
          >
            {kinds.has("handover")
              ? m.acts_create_return()
              : m.acts_create_handover()}
          </Button>
        </div>
      )}
      {create.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(create.error)}
        </p>
      )}
    </div>
  )
}

function stageLabel(stage: string | null | undefined): string {
  switch (stage) {
    case "drafted":
      return m.contract_stage_drafted()
    case "handed_to_tenant":
      return m.contract_stage_handed()
    case "tenant_signed":
      return m.contract_stage_tenant_signed()
    case "documents_received":
      return m.contract_stage_documents()
    case "checklist_completed":
      return m.contract_stage_checklist()
    case "landlord_signed":
      return m.contract_stage_landlord_signed()
    case "copy_sent":
      return m.contract_stage_copy_sent()
    case "registered":
      return m.contract_stage_registered()
    default:
      return "-"
  }
}
