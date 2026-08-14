import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { WalletIcon } from "lucide-react"

import { m } from "#/paraglide/messages"
import { ConfirmAction } from "@/components/confirm-action"
import { FormAlert } from "@/components/form-alert"
import { Panel } from "@/components/panel"
import { QueryBoundary } from "@/components/query-boundary"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Skeleton } from "@/components/ui/skeleton"
import { problemMessage } from "@/lib/auth"
import {
  contractDepositQuery,
  payDeposit,
  refundDeposit,
} from "@/lib/contracts"
import { formatTenge } from "@/lib/format"
import { notifySuccess } from "@/lib/toast"

/**
 * Депозит по договору (FR-1003, п. 132–136).
 *
 * Одна панель на два кабинета: наниматель видит, сколько и до какого срока
 * он должен внести, финблок - те же цифры и кнопки движения денег. Суммы
 * не редактируются на глаз: размер депозита равен месячной плате, и
 * проверяет это сервер (п. 132).
 */
export function DepositPanel({
  contractId,
  canManage,
}: {
  contractId: string
  /** Финблок оформляет проводки; наниматель только смотрит (FR-1001) */
  canManage: boolean
}) {
  const queryClient = useQueryClient()
  const deposit = useQuery(contractDepositQuery(contractId))
  const [paidAt, setPaidAt] = useState("")
  const [note, setNote] = useState("")

  const invalidate = () =>
    queryClient.invalidateQueries({
      queryKey: contractDepositQuery(contractId).queryKey,
    })

  // Размер депозита приходит со счетом (месячная плата договора, п. 132):
  // на экране он не редактируется, а проверяет его все равно сервер
  const account = deposit.data ?? null
  const required = account?.required_amount ?? null
  const due =
    account == null || required == null
      ? null
      : Number(required) - Number(account.balance)
  const settled = due != null && due <= 0

  const pay = useMutation({
    mutationFn: () =>
      payDeposit(contractId, String(due ?? 0), paidAt, note || undefined),
    onSuccess: async () => {
      setNote("")
      notifySuccess(m.deposit_paid_toast())
      await invalidate()
    },
  })

  const refund = useMutation({
    mutationFn: () => refundDeposit(contractId, note || undefined),
    onSuccess: async () => {
      setNote("")
      notifySuccess(m.deposit_refunded_toast())
      await invalidate()
    },
  })

  return (
    <Panel
      titleAs="h3"
      title={m.deposit_title()}
      description={m.deposit_rule()}
      contentClassName="flex flex-col gap-3"
    >
      <QueryBoundary
        query={deposit}
        skeleton={<FiguresSkeleton />}
        // Счета нет, пока договор не заключен: обязанность начинается
        // с п. 126 - и это ответ сервера, а не состояние загрузки
        empty={{
          when: (data) => data == null,
          icon: WalletIcon,
          title: m.deposit_not_open_title(),
          description: m.deposit_not_open(),
        }}
      >
        {(data) =>
          data == null ? null : (
            <div className="flex flex-col gap-3" data-testid="deposit-panel">
              <dl className="grid gap-3 sm:grid-cols-3">
                <Figure label={m.deposit_required()}>
                  {required == null ? "-" : formatTenge(required)}
                </Figure>
                <Figure label={m.deposit_balance()} testId="deposit-balance">
                  {formatTenge(data.balance)}
                </Figure>
                <Figure label={m.deposit_due()} testId="deposit-due">
                  {settled ? m.deposit_settled() : formatTenge(String(due))}
                </Figure>
              </dl>

              {canManage && (
                <div className="flex flex-col gap-3 border-t pt-3">
                  <div className="flex flex-wrap items-end gap-3">
                    <div className="flex w-44 flex-col gap-1.5">
                      <Label htmlFor={`deposit-paid-at-${contractId}`}>
                        {m.fee_paid_at_label()}
                      </Label>
                      <Input
                        id={`deposit-paid-at-${contractId}`}
                        type="date"
                        value={paidAt}
                        onChange={(event) => setPaidAt(event.target.value)}
                      />
                    </div>
                    <div className="flex w-full flex-1 flex-col gap-1.5 sm:min-w-56">
                      <Label htmlFor={`deposit-note-${contractId}`}>
                        {m.deposit_note_label()}
                      </Label>
                      <Input
                        id={`deposit-note-${contractId}`}
                        value={note}
                        onChange={(event) => setNote(event.target.value)}
                      />
                    </div>
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <Button
                      onClick={() => pay.mutate()}
                      disabled={settled || paidAt === "" || pay.isPending}
                    >
                      {m.deposit_pay_cta()}
                    </Button>
                    {/* Возврат - движение денег обратно нанимателю (п. 136):
                        отыграть его назад в книге нельзя */}
                    <ConfirmAction
                      title={m.deposit_refund_confirm_title()}
                      description={m.deposit_refund_confirm_description()}
                      confirmLabel={m.deposit_refund_cta()}
                      disabled={Number(data.balance) <= 0 || refund.isPending}
                      onConfirm={() => refund.mutate()}
                      trigger={
                        <Button variant="outline">
                          {m.deposit_refund_cta()}
                        </Button>
                      }
                    />
                  </div>
                </div>
              )}

              {(pay.isError || refund.isError) && (
                <FormAlert>
                  {problemMessage(pay.error ?? refund.error)}
                </FormAlert>
              )}
            </div>
          )
        }
      </QueryBoundary>
    </Panel>
  )
}

/** Заглушка ровно по форме строки цифр: три поля в ряд (п. 132). */
function FiguresSkeleton() {
  return (
    <div className="grid gap-3 sm:grid-cols-3" aria-hidden="true">
      <Skeleton className="h-11 w-full rounded-lg" />
      <Skeleton className="h-11 w-full rounded-lg" />
      <Skeleton className="h-11 w-full rounded-lg" />
    </div>
  )
}

function Figure({
  label,
  children,
  testId,
}: {
  label: string
  children: React.ReactNode
  testId?: string
}) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-sm text-muted-foreground">{label}</dt>
      <dd className="font-medium" data-testid={testId}>
        {children}
      </dd>
    </div>
  )
}
