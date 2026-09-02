import { useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { DepositPanel } from "@/components/deposit-panel"
import { PageHeader } from "@/components/page-header"
import { Panel } from "@/components/panel"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { api } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import { formatDateTime, formatTenge } from "@/lib/format"
import {
  ledgerAccountsQuery,
  ledgerEntriesQuery,
  refundReasonsQuery,
} from "@/lib/ledger"

import type { LedgerAccountDto } from "@/lib/ledger"

// Кабинет департамента финансов (М10): подтверждение поступления взносов
// вручную (FR-405, банк-интеграции нет) и депозитная книга (FR-1001).
export const Route = createFileRoute("/app/finance/")({
  head: () => ({ meta: [{ title: `${m.cabinet_finance()} - ToU Rent` }] }),
  component: FinanceHome,
})

function FinanceHome() {
  const { data: accounts } = useQuery(ledgerAccountsQuery())

  return (
    <div className="flex flex-col gap-6">
      {/* Имя кабинета - заголовок страницы: из макета он ушел вместе
          с прежней шапкой (каркас называет кабинет группой боковой
          навигации) */}
      <PageHeader
        title={m.cabinet_finance()}
        description={m.finance_dash_subtitle()}
      />
      {/* Подтверждение взноса - первым: это единственное действие кабинета,
          книга под ним - его последствие */}
      <ConfirmFeeForm />
      <Panel title={m.ledger_title()} titleAs="h2">
        {accounts === undefined || accounts.length === 0 ? (
          <p className="text-sm text-muted-foreground">{m.ledger_empty()}</p>
        ) : (
          <ul className="flex flex-col gap-3" data-testid="ledger-accounts">
            {accounts.map((account) => (
              <AccountCard key={account.id} account={account} />
            ))}
          </ul>
        )}
      </Panel>
    </div>
  )
}

/** FR-405: оператор вводит сумму и дату поступления по банковской выписке. */
function ConfirmFeeForm() {
  const queryClient = useQueryClient()
  const [applicationId, setApplicationId] = useState("")
  const [amount, setAmount] = useState("")
  const [paidAt, setPaidAt] = useState("")

  const confirm = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST("/api/v1/applications/{id}/fee", {
        params: { path: { id: applicationId } },
        body: { amount, paid_at: paidAt },
      })
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("fee confirmation failed")
      }
      return data
    },
    onSuccess: async () => {
      setApplicationId("")
      setAmount("")
      await queryClient.invalidateQueries({ queryKey: ["ledger"] })
    },
  })

  return (
    <Panel
      title={m.fee_confirm_title()}
      titleAs="h2"
      description={m.fee_confirm_hint()}
    >
      <form
        className="flex flex-wrap items-end gap-3"
        data-testid="fee-confirm-form"
        onSubmit={(event) => {
          event.preventDefault()
          confirm.mutate()
        }}
      >
        {/* На узком экране `min-w-72` вылезало за поле страницы: минимум
            включается только там, где ширины хватает */}
        <div className="flex w-full flex-col gap-1.5 sm:w-auto sm:min-w-72">
          <Label htmlFor="fee-application">{m.fee_application_label()}</Label>
          <Input
            id="fee-application"
            required
            value={applicationId}
            onChange={(event) => setApplicationId(event.target.value)}
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="fee-amount">{m.fee_amount_label()}</Label>
          <Input
            id="fee-amount"
            inputMode="decimal"
            required
            value={amount}
            onChange={(event) => setAmount(event.target.value)}
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="fee-paid-at">{m.fee_paid_at_label()}</Label>
          <Input
            id="fee-paid-at"
            type="date"
            required
            value={paidAt}
            onChange={(event) => setPaidAt(event.target.value)}
          />
        </div>
        <Button
          type="submit"
          data-testid="fee-confirm-submit"
          disabled={confirm.isPending}
        >
          {m.fee_confirm_submit()}
        </Button>
      </form>
      {confirm.isError && (
        <p role="alert" className="mt-3 text-sm text-destructive">
          {problemMessage(confirm.error)}
        </p>
      )}
    </Panel>
  )
}

function AccountCard({ account }: { account: LedgerAccountDto }) {
  const [open, setOpen] = useState(false)

  return (
    <li className="flex flex-col gap-3 rounded-lg border p-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-col">
          <span className="font-medium">{account.owner_name}</span>
          <span className="text-sm text-muted-foreground">
            {account.kind === "participant_fee"
              ? m.ledger_kind_fee()
              : m.ledger_kind_deposit()}
            {account.tender_title !== null && ` - ${account.tender_title}`}
            {account.lot_seq != null && ` (${m.lot_seq()} ${account.lot_seq})`}
          </span>
        </div>
        <span className="font-medium" data-testid="ledger-balance">
          {formatTenge(account.balance)}
        </span>
      </div>
      <div className="flex flex-wrap gap-3">
        <Button variant="outline" size="sm" onClick={() => setOpen(!open)}>
          {open ? m.ledger_hide_entries() : m.ledger_show_entries()}
        </Button>
        {account.application_id != null && Number(account.balance) > 0 && (
          <RefundForm applicationId={account.application_id} />
        )}
      </div>
      {/* Депозит по договору (FR-1003, п. 132–136): внесение и возврат
          оформляет финблок - здесь же, где ведется книга */}
      {account.contract_id != null && (
        <DepositPanel contractId={account.contract_id} canManage />
      )}
      {open && <Entries accountId={account.id} />}
    </li>
  )
}

function Entries({ accountId }: { accountId: string }) {
  const { data: page } = useQuery(ledgerEntriesQuery(accountId))
  if (page === undefined) return null

  return (
    <>
      {/* Обрезанная выписка - это недосчитанные деньги: о потолке выборки
          финблок узнает из ответа, а не из расхождения в сверке */}
      {page.truncated && (
        <p
          role="status"
          className="rounded-lg bg-amber-500/10 px-3 py-2 text-sm text-amber-700 ring-1 ring-amber-500/30 dark:text-amber-400"
          data-testid="ledger-truncated"
        >
          {m.list_truncated({ count: page.items.length })}
        </p>
      )}
      <ul className="flex flex-col gap-1 text-sm" data-testid="ledger-entries">
        {page.items.map((entry) => (
          <li key={entry.id} className="flex flex-wrap items-center gap-x-3">
            <span className="text-muted-foreground" suppressHydrationWarning>
              {formatDateTime(entry.occurred_at)}
            </span>
            <span>{opLabel(entry.op)}</span>
            <span
              className={Number(entry.credit) > 0 ? "" : "text-destructive"}
            >
              {Number(entry.credit) > 0
                ? `+${formatTenge(entry.credit)}`
                : `−${formatTenge(entry.debit)}`}
            </span>
          </li>
        ))}
      </ul>
    </>
  )
}

/** FR-1002: возврат оформляется с основанием из закрытого перечня п. 26. */
function RefundForm({ applicationId }: { applicationId: string }) {
  const queryClient = useQueryClient()
  const { data: reasons } = useQuery(refundReasonsQuery)
  const [reason, setReason] = useState("")

  const refund = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST(
        "/api/v1/applications/{id}/fee/refund",
        {
          params: { path: { id: applicationId } },
          body: { reason },
        }
      )
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("refund failed")
      }
      return data
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["ledger"] })
    },
  })

  return (
    <form
      className="flex flex-wrap items-center gap-2"
      data-testid="refund-form"
      onSubmit={(event) => {
        event.preventDefault()
        refund.mutate()
      }}
    >
      <NativeSelect
        aria-label={m.refund_reason_label()}
        required
        value={reason}
        onChange={(event) => setReason(event.target.value)}
      >
        <NativeSelectOption value="">
          {m.refund_reason_label()}
        </NativeSelectOption>
        {(reasons ?? []).map((item) => (
          <NativeSelectOption key={item.code} value={item.code}>
            {item.label_ru}
          </NativeSelectOption>
        ))}
      </NativeSelect>
      <Button
        type="submit"
        variant="outline"
        size="sm"
        data-testid="refund-submit"
        disabled={refund.isPending || reason === ""}
      >
        {m.refund_submit()}
      </Button>
      {refund.isError && (
        <span role="alert" className="text-sm text-destructive">
          {problemMessage(refund.error)}
        </span>
      )}
    </form>
  )
}

function opLabel(op: string): string {
  switch (op) {
    case "receipt_confirmed":
      return m.ledger_op_receipt()
    case "hold":
      return m.ledger_op_hold()
    case "offset":
      return m.ledger_op_offset()
    case "refund":
      return m.ledger_op_refund()
    case "writeoff":
      return m.ledger_op_writeoff()
    case "replenish":
      return m.ledger_op_replenish()
    default:
      return op
  }
}
