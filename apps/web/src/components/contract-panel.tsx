import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { FileTextIcon } from "lucide-react"

import { m } from "#/paraglide/messages"
import { ConfirmAction } from "@/components/confirm-action"
import { ContractAmendmentsPanel } from "@/components/contract-amendments-panel"
import { DepositPanel } from "@/components/deposit-panel"
import { FormAlert } from "@/components/form-alert"
import { Panel } from "@/components/panel"
import { QueryBoundary } from "@/components/query-boundary"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { Skeleton } from "@/components/ui/skeleton"
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
import { formatDate, formatDateTime, formatTenge } from "@/lib/format"
import { serverLabel } from "@/lib/server-label"
import { notifySuccess } from "@/lib/toast"

import type { ContractDto } from "@/lib/contracts"

/**
 * Шаг конвейера договора (п. 110–115, `domain::contract::Stage`).
 *
 * Контракт отдает шаг простой строкой, поэтому союз объявлен здесь:
 * исчерпывающий `Record` ниже ломает typecheck, когда в конвейер добавят
 * шаг, - вместо молчаливого прочерка на экране.
 */
export type ContractStage =
  | "drafted"
  | "handed_to_tenant"
  | "tenant_signed"
  | "documents_received"
  | "checklist_completed"
  | "landlord_signed"
  | "copy_sent"
  | "registered"

const STAGE_LABELS: Record<ContractStage, () => string> = {
  drafted: m.contract_stage_drafted,
  handed_to_tenant: m.contract_stage_handed,
  tenant_signed: m.contract_stage_tenant_signed,
  documents_received: m.contract_stage_documents,
  checklist_completed: m.contract_stage_checklist,
  landlord_signed: m.contract_stage_landlord_signed,
  copy_sent: m.contract_stage_copy_sent,
  registered: m.contract_stage_registered,
}

/** Вид акта (Прил. 7–8, `domain::act::ActKind`). */
export type ActKind = "handover" | "return"

const ACT_KIND_LABELS: Record<ActKind, () => string> = {
  handover: m.contract_act_kind_handover,
  return: m.contract_act_kind_return,
}

/** Подпись шага конвейера: `null` - договор только составлен. */
export function stageLabel(stage: string | null | undefined): string {
  if (stage != null && stage in STAGE_LABELS) {
    return STAGE_LABELS[stage as ContractStage]()
  }
  return "-"
}

/** Подпись вида акта: машинный код наружу не идет ни в одном случае. */
export function actKindLabel(kind: string | null | undefined): string {
  if (kind != null && kind in ACT_KIND_LABELS) {
    return ACT_KIND_LABELS[kind as ActKind]()
  }
  return "-"
}

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
  const contracts = useQuery(tenderContractsQuery(tenderId))

  const draft = useMutation({
    mutationFn: (lotId: string) => draftContract(lotId),
    onSuccess: async () => {
      notifySuccess(m.contract_drafted_toast())
      await queryClient.invalidateQueries({
        queryKey: tenderContractsQuery(tenderId).queryKey,
      })
    },
  })

  return (
    <QueryBoundary
      query={contracts}
      skeleton={<Skeleton className="h-40 w-full rounded-xl" />}
    >
      {(rows) => {
        // Договор составляется по лоту, у которого нет действующего:
        // прекращенный уклонением места не занимает (FR-903, п. 117)
        const pending = canDraft
          ? lots.filter(
              (lot) =>
                !rows.some(
                  (contract) => contract.lot_id === lot.id && !contract.evaded
                )
            )
          : []

        // Ни договоров, ни лотов под договор. Раздел все равно называет
        // себя: у организатора это отдельная вкладка, и молчаливая пустота
        // читается как поломка, а не как «договоров еще нет»
        if (rows.length === 0 && pending.length === 0) {
          return (
            <Panel titleAs="h3" title={m.contracts_title()}>
              <p className="text-sm text-muted-foreground">
                {m.contracts_empty()}
              </p>
            </Panel>
          )
        }

        return (
          <Panel
            titleAs="h3"
            title={m.contracts_title()}
            contentClassName="flex flex-col gap-4"
          >
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
              <FormAlert>{problemMessage(draft.error)}</FormAlert>
            )}

            {rows.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                {m.contracts_empty()}
              </p>
            ) : (
              rows.map((contract) => (
                <ContractCard
                  key={contract.id}
                  contract={contract}
                  tenderId={tenderId}
                  canManageDeposit={canManageDeposit}
                />
              ))
            )}
          </Panel>
        )
      }}
    </QueryBoundary>
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
  const checklist = useQuery(contractChecklistQuery(contract.id))

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
    onSuccess: async (_data, stage) => {
      notifySuccess(m.contract_advanced_toast({ stage: stageLabel(stage) }))
      await refresh()
    },
  })
  const check = useMutation({
    mutationFn: ({ code, checked }: { code: string; checked: boolean }) =>
      checkChecklistItem(contract.id, code, checked),
    // Тоста здесь нет намеренно: отметка сверки сама себе подтверждение -
    // флажок перерисовывается ответом сервера, а отказ виден сообщением ниже
    onSuccess: refresh,
  })
  const register = useMutation({
    mutationFn: () => registerContract(contract.id, regNumber),
    onSuccess: async () => {
      notifySuccess(m.contract_registered_toast())
      await refresh()
    },
  })

  const error = advance.error ?? check.error ?? register.error

  return (
    <article
      className="flex flex-col gap-4 rounded-xl border p-4"
      data-testid="contract-card"
    >
      <div className="flex flex-wrap items-start justify-between gap-3">
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

      <QueryBoundary
        query={checklist}
        skeleton={<Skeleton className="h-16 w-full rounded-lg" />}
      >
        {(items) =>
          items.length === 0 ? null : (
            <ul className="flex flex-col gap-1 text-sm" data-testid="checklist">
              {items.map((item) => (
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
                    {serverLabel(item)}{" "}
                    <span className="text-muted-foreground">
                      ({item.rule_ref})
                    </span>
                  </label>
                </li>
              ))}
            </ul>
          )
        }
      </QueryBoundary>

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
        <FormAlert>{m.evasion_contract_terminated()}</FormAlert>
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

      {error != null && <FormAlert>{problemMessage(error)}</FormAlert>}
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
  const grounds = useQuery(evasionGroundsQuery)
  const [ground, setGround] = useState("")

  const declare = useMutation({
    mutationFn: () => declareEvasion(contractId, ground),
    onSuccess: async () => {
      setGround("")
      notifySuccess(m.contract_evasion_declared_toast())
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
    <Panel
      titleAs="h3"
      title={m.evasion_declare_title()}
      description={m.evasion_consequence()}
      contentClassName="flex flex-col gap-3"
    >
      {/* Перечень оснований закрыт (п. 116): пока он не загружен, выбирать
          не из чего - и кнопка признания вместе с ним не показывается */}
      <QueryBoundary
        query={grounds}
        skeleton={<Skeleton className="h-9 w-full rounded-lg sm:max-w-96" />}
      >
        {(items) => (
          <div className="flex flex-col gap-2 sm:flex-row sm:flex-wrap sm:items-center">
            <NativeSelect
              className="w-full sm:w-auto"
              aria-label={m.evasion_ground_label()}
              value={ground}
              onChange={(event) => setGround(event.target.value)}
            >
              <NativeSelectOption value="">
                {m.evasion_ground_label()}
              </NativeSelectOption>
              {items.map((item) => (
                <NativeSelectOption key={item.code} value={item.code}>
                  {serverLabel(item)} ({item.rule_ref})
                </NativeSelectOption>
              ))}
            </NativeSelect>
            {/* Признание уклонения прекращает договор и удерживает взнос
                (п. 116) - назад Правила это не отыгрывают */}
            <ConfirmAction
              title={m.contract_evasion_confirm_title()}
              description={m.contract_evasion_confirm_description()}
              confirmLabel={m.evasion_declare()}
              disabled={declare.isPending || ground === ""}
              onConfirm={() => declare.mutate()}
              trigger={
                <Button
                  variant="destructive"
                  size="sm"
                  data-testid="declare-evasion"
                >
                  {m.evasion_declare()}
                </Button>
              }
            />
          </div>
        )}
      </QueryBoundary>
      {declare.isError && (
        <FormAlert>{problemMessage(declare.error)}</FormAlert>
      )}
    </Panel>
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
  const acts = useQuery(contractActsQuery(contractId))
  const [actDate, setActDate] = useState("")

  const create = useMutation({
    mutationFn: (kind: ActKind) => createAct(contractId, kind, actDate),
    onSuccess: async (_data, kind) => {
      setActDate("")
      notifySuccess(m.contract_act_created_toast({ kind: actKindLabel(kind) }))
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

  return (
    <Panel
      titleAs="h3"
      title={m.acts_title()}
      contentClassName="flex flex-col gap-3"
    >
      <QueryBoundary
        query={acts}
        skeleton={<Skeleton className="h-16 w-full rounded-lg" />}
        empty={{
          when: (rows) => rows.length === 0,
          icon: FileTextIcon,
          title: m.contract_acts_empty(),
        }}
      >
        {(rows) => (
          <ul className="flex flex-col gap-1 text-sm" data-testid="acts">
            {rows.map((act) => (
              <li
                key={act.id}
                className="flex flex-wrap items-center gap-x-3 gap-y-1"
              >
                <span>{actKindLabel(act.kind)}</span>
                {/* `appendix` приходит с сервера ссылкой на Правила
                    («Прил. 7»), как и `rule_ref`: это цитата, а не код */}
                <Badge variant="neutral">{act.appendix}</Badge>
                <span suppressHydrationWarning>{formatDate(act.act_date)}</span>
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
      </QueryBoundary>

      <QueryBoundary
        query={acts}
        skeleton={<Skeleton className="h-9 w-full rounded-lg sm:max-w-80" />}
      >
        {(rows) => {
          const kinds = new Set(rows.map((act) => act.kind))
          // Возврат закрывает договор: после него составлять нечего
          if (kinds.has("return")) return null
          const handed = kinds.has("handover")
          const disabled = create.isPending || actDate === ""

          return (
            <div className="flex flex-col gap-2 sm:flex-row sm:flex-wrap sm:items-center">
              <Input
                type="date"
                aria-label={m.acts_date_label()}
                className="w-full sm:max-w-44"
                value={actDate}
                onChange={(event) => setActDate(event.target.value)}
              />
              {handed ? (
                // Возврат объекта закрывает договор и освобождает объект
                // (FR-103, п. 129) - отыграть это назад нельзя
                <ConfirmAction
                  title={m.contract_act_return_confirm_title()}
                  description={m.contract_act_return_confirm_description()}
                  confirmLabel={m.acts_create_return()}
                  disabled={disabled}
                  onConfirm={() => create.mutate("return")}
                  trigger={
                    <Button
                      variant="destructive"
                      size="sm"
                      data-testid="create-act"
                    >
                      {m.acts_create_return()}
                    </Button>
                  }
                />
              ) : (
                <Button
                  variant="outline"
                  size="sm"
                  data-testid="create-act"
                  disabled={disabled}
                  onClick={() => create.mutate("handover")}
                >
                  {m.acts_create_handover()}
                </Button>
              )}
            </div>
          )
        }}
      </QueryBoundary>

      {create.isError && <FormAlert>{problemMessage(create.error)}</FormAlert>}
    </Panel>
  )
}
