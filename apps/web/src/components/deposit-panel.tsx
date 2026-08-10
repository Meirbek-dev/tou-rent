import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { m } from "#/paraglide/messages"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { problemMessage } from "@/lib/auth"
import {
  contractDepositQuery,
  payDeposit,
  refundDeposit,
} from "@/lib/contracts"
import { formatTenge } from "@/lib/format"

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
  const { data: account } = useQuery(contractDepositQuery(contractId))
  const [paidAt, setPaidAt] = useState("")
  const [note, setNote] = useState("")

  const invalidate = () =>
    queryClient.invalidateQueries({
      queryKey: contractDepositQuery(contractId).queryKey,
    })

  // Размер депозита приходит со счетом (месячная плата договора, п. 132):
  // на экране он не редактируется, а проверяет его все равно сервер
  const required = account?.required_amount ?? null
  const due =
    account == null || required == null
      ? null
      : Number(required) - Number(account.balance)

  const pay = useMutation({
    mutationFn: () =>
      payDeposit(contractId, String(due ?? 0), paidAt, note || undefined),
    onSuccess: async () => {
      setNote("")
      await invalidate()
    },
  })

  const refund = useMutation({
    mutationFn: () => refundDeposit(contractId, note || undefined),
    onSuccess: async () => {
      setNote("")
      await invalidate()
    },
  })

  // Счета нет, пока договор не заключен: обязанность начинается с п. 126
  if (account == null) {
    return (
      <section aria-labelledby="deposit" className="flex flex-col gap-2">
        <h3 id="deposit" className="font-heading text-base font-medium">
          {m.deposit_title()}
        </h3>
        <p className="text-sm text-muted-foreground">{m.deposit_not_open()}</p>
      </section>
    )
  }

  const settled = due != null && due <= 0

  return (
    <section
      aria-labelledby="deposit"
      className="flex flex-col gap-3 rounded-lg border p-4"
      data-testid="deposit-panel"
    >
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h3 id="deposit" className="font-heading text-base font-medium">
          {m.deposit_title()}
        </h3>
        <span className="text-sm text-muted-foreground">
          {m.deposit_rule()}
        </span>
      </div>

      <dl className="grid gap-3 sm:grid-cols-3">
        <Figure label={m.deposit_required()}>
          {required == null ? "-" : formatTenge(required)}
        </Figure>
        <Figure label={m.deposit_balance()} testId="deposit-balance">
          {formatTenge(account.balance)}
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
            <div className="flex min-w-56 flex-1 flex-col gap-1.5">
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
            <Button
              variant="outline"
              onClick={() => refund.mutate()}
              disabled={Number(account.balance) <= 0 || refund.isPending}
            >
              {m.deposit_refund_cta()}
            </Button>
          </div>
        </div>
      )}

      {(pay.isError || refund.isError) && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(pay.error ?? refund.error)}
        </p>
      )}
    </section>
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
