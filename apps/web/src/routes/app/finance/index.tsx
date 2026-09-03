import { useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import {
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { ConfirmAction } from "@/components/confirm-action"
import { DepositPanel } from "@/components/deposit-panel"
import { PageHeader } from "@/components/page-header"
import { Panel } from "@/components/panel"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { api } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import {
  LEDGER_ACCOUNT_KINDS,
  codeLabel,
  matchesLedgerAccount,
} from "@/lib/finance"
import { formatDate, formatDateTime, formatTenge } from "@/lib/format"
import {
  ledgerAccountsQuery,
  ledgerEntriesQuery,
  pendingFeesQuery,
  refundReasonsQuery,
} from "@/lib/ledger"
import { serverLabel } from "@/lib/server-label"
import { notifyError, notifySuccess } from "@/lib/toast"

import type { FinancePendingFeeDto, LedgerAccountDto } from "@/lib/ledger"

/** Черновик формы подтверждения: очередь подставляет в него заявку и сумму. */
type FeeDraft = { applicationId: string; amount: string }

// Кабинет департамента финансов (М10): подтверждение поступления взносов
// вручную (FR-405, банк-интеграции нет) и депозитная книга (FR-1001).
export const Route = createFileRoute("/app/finance/")({
  head: () => ({ meta: [{ title: `${m.cabinet_finance()} - ToU Rent` }] }),
  component: FinanceHome,
})

function FinanceHome() {
  // Черновик живет над формой: очередь взносов заполняет его нажатием,
  // а свободное поле остается на случай заявки, которой в очереди нет
  const [draft, setDraft] = useState<FeeDraft>({
    applicationId: "",
    amount: "",
  })
  const [filter, setFilter] = useState({ kind: "", query: "" })

  // Вид счета отбирает сервер (`kind`), подстроку - экран: постраничной
  // выдачи у перечня счетов в контракте нет, он приходит целиком
  const { data: accounts } = useQuery(
    ledgerAccountsQuery(filter.kind === "" ? undefined : filter.kind)
  )
  const shown = (accounts ?? []).filter((account) =>
    matchesLedgerAccount(account, filter)
  )

  const pick = (row: FinancePendingFeeDto) => {
    setDraft({
      applicationId: row.application_id,
      amount: row.guarantee_fee,
    })
    // Фокус переводится на форму: подставленные значения надо увидеть
    // до нажатия, а форма стоит выше очереди
    document.getElementById("fee-application")?.focus()
  }

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
          очередь и книга под ним - его повод и его последствие */}
      <ConfirmFeeForm draft={draft} onDraftChange={setDraft} />
      <PendingFeesPanel onPick={pick} />
      <Panel
        title={m.ledger_title()}
        titleAs="h2"
        contentClassName="flex flex-col gap-4"
      >
        {/* Тридцать счетов одной лентой не искались вовсе: фильтр по виду
            счета и подстроке - единственный способ найти нужный */}
        <div className="flex flex-wrap items-end gap-3">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="ledger-kind">{m.ledger_filter_kind_label()}</Label>
            <NativeSelect
              id="ledger-kind"
              value={filter.kind}
              onChange={(event) =>
                setFilter((current) => ({
                  ...current,
                  kind: event.target.value,
                }))
              }
            >
              <NativeSelectOption value="">
                {m.ledger_filter_all_kinds()}
              </NativeSelectOption>
              {LEDGER_ACCOUNT_KINDS.map((kind) => (
                <NativeSelectOption key={kind} value={kind}>
                  {ledgerKindLabel(kind)}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </div>
          <div className="flex w-full flex-col gap-1.5 sm:w-72">
            <Label htmlFor="ledger-query">
              {m.ledger_filter_query_label()}
            </Label>
            <Input
              id="ledger-query"
              data-testid="ledger-query"
              value={filter.query}
              onChange={(event) =>
                setFilter((current) => ({
                  ...current,
                  query: event.target.value,
                }))
              }
            />
          </div>
        </div>
        {accounts === undefined || accounts.length === 0 ? (
          <p className="text-sm text-muted-foreground">{m.ledger_empty()}</p>
        ) : shown.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {m.ledger_filtered_empty()}
          </p>
        ) : (
          <>
            <p className="text-sm text-muted-foreground">
              {m.registry_found()}:{" "}
              <span
                className="font-medium text-foreground tabular-nums"
                data-testid="ledger-count"
              >
                {shown.length}
              </span>
            </p>
            <ul className="flex flex-col gap-3" data-testid="ledger-accounts">
              {shown.map((account) => (
                <AccountCard key={account.id} account={account} />
              ))}
            </ul>
          </>
        )}
      </Panel>
      {/* Реестру уклонистов (FR-505, п. 52.4, 120) здесь не место, пока
          политика доступа его не открывает: `GET /api/v1/evaders` требует
          `ApplicationReadAll`, которого у роли финансов нет, и панель
          отвечала бы 403. Гаунтлет G-130 закрыт наполовину - подписью
          основания уклонения в самом реестре у секретаря */}
    </div>
  )
}

/**
 * Очередь заявок, ждущих подтверждения взноса (FR-405).
 *
 * До нее подтверждение начиналось с UUID, которого кабинет не показывал:
 * оператор должен был взять его из банковской выписки, где его нет.
 */
function PendingFeesPanel({
  onPick,
}: {
  onPick: (row: FinancePendingFeeDto) => void
}) {
  const queue = useInfiniteQuery(pendingFeesQuery)
  const rows = queue.data?.pages.flatMap((page) => page.items) ?? []

  return (
    <Panel
      title={m.finance_pending_title()}
      titleAs="h2"
      description={m.finance_pending_hint()}
      contentClassName="flex flex-col gap-3"
    >
      {rows.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          {m.finance_pending_empty()}
        </p>
      ) : (
        <>
          <ul className="flex flex-col gap-2" data-testid="pending-fees">
            {rows.map((row) => (
              <li
                key={row.application_id}
                className="flex flex-wrap items-center justify-between gap-3 rounded-lg border p-3"
              >
                <div className="flex flex-col">
                  <span className="font-medium">{row.applicant_name}</span>
                  <span className="text-sm text-muted-foreground">
                    {row.tender_title} ({m.lot_seq()} {row.lot_seq})
                  </span>
                  <span
                    className="text-sm text-muted-foreground"
                    suppressHydrationWarning
                  >
                    {m.application_submitted_at()}:{" "}
                    {formatDateTime(row.submitted_at)}
                  </span>
                </div>
                <div className="flex flex-wrap items-center gap-3">
                  <span className="font-medium tabular-nums">
                    {formatTenge(row.guarantee_fee)}
                  </span>
                  <Button
                    variant="outline"
                    size="sm"
                    data-testid={`pending-fee-${row.application_id}`}
                    onClick={() => onPick(row)}
                  >
                    {m.finance_pending_confirm()}
                  </Button>
                </div>
              </li>
            ))}
          </ul>
          {queue.hasNextPage && (
            <div>
              <Button
                variant="outline"
                size="sm"
                data-testid="pending-fees-more"
                disabled={queue.isFetchingNextPage}
                onClick={() => void queue.fetchNextPage()}
              >
                {m.list_load_more()}
              </Button>
            </div>
          )}
        </>
      )}
    </Panel>
  )
}

/** FR-405: оператор вводит сумму и дату поступления по банковской выписке. */
function ConfirmFeeForm({
  draft,
  onDraftChange,
}: {
  draft: FeeDraft
  onDraftChange: (next: FeeDraft) => void
}) {
  const queryClient = useQueryClient()
  const [paidAt, setPaidAt] = useState("")

  const confirm = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST("/api/v1/applications/{id}/fee", {
        params: { path: { id: draft.applicationId } },
        body: { amount: draft.amount, paid_at: paidAt },
      })
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("fee confirmation failed")
      }
      return data
    },
    onSuccess: async () => {
      onDraftChange({ applicationId: "", amount: "" })
      notifySuccess(m.fee_confirmed_toast())
      // Ветка `ledger` держит и книгу, и очередь: подтвержденная заявка
      // уходит из очереди тем же вызовом, каким обновляется счет
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
            value={draft.applicationId}
            onChange={(event) =>
              onDraftChange({ ...draft, applicationId: event.target.value })
            }
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="fee-amount">{m.fee_amount_label()}</Label>
          <Input
            id="fee-amount"
            inputMode="decimal"
            required
            value={draft.amount}
            onChange={(event) =>
              onDraftChange({ ...draft, amount: event.target.value })
            }
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
            {ledgerKindLabel(account.kind)}
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
  // Основание возврата приходит кодом закрытого перечня (п. 26): подпись
  // к нему ведет справочник, а не Paraglide
  const { data: reasons } = useQuery(refundReasonsQuery)
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
        {page.items.map((entry) => {
          const reason = codeLabel(entry.refund_reason, reasons)
          return (
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
              {/* Дата поступления денег - не дата проводки: взнос, внесенный
                  первого числа, подтверждается третьим, и в книге стояло
                  только третье */}
              {entry.paid_at != null && (
                <span
                  className="text-muted-foreground"
                  data-testid="ledger-paid-at"
                >
                  {m.fee_paid_at_label()}: {formatDate(entry.paid_at)}
                </span>
              )}
              {reason !== null && (
                <span
                  className="text-muted-foreground"
                  data-testid="ledger-refund-reason"
                >
                  {m.refund_reason_label()}: {reason}
                </span>
              )}
              {/* Пункт Правил - основание проводки: книга append-only, и
                  каждая строка обязана называть, чем она оправдана */}
              {entry.rule_ref != null && (
                <span
                  className="text-muted-foreground"
                  data-testid="ledger-rule-ref"
                >
                  {entry.rule_ref}
                </span>
              )}
            </li>
          )
        })}
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
      notifySuccess(m.fee_refunded_toast())
      await queryClient.invalidateQueries({ queryKey: ["ledger"] })
    },
    onError: (error: unknown) => notifyError(problemMessage(error)),
  })

  return (
    <div
      className="flex flex-wrap items-center gap-2"
      data-testid="refund-form"
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
            {serverLabel(item)}
          </NativeSelectOption>
        ))}
      </NativeSelect>
      {/* Возврат - движение денег обратно участнику, а книга append-only:
          отыграть проводку назад нельзя, только завести встречную */}
      <ConfirmAction
        title={m.fee_refund_confirm_title()}
        description={m.fee_refund_confirm_description()}
        confirmLabel={m.refund_submit()}
        disabled={refund.isPending || reason === ""}
        onConfirm={() => refund.mutate()}
        trigger={
          <Button variant="outline" size="sm" data-testid="refund-submit">
            {m.refund_submit()}
          </Button>
        }
      />
      {refund.isError && (
        <span role="alert" className="text-sm text-destructive">
          {problemMessage(refund.error)}
        </span>
      )}
    </div>
  )
}

/** Вид счета книги - предметный термин Правил (FR-1001). */
function ledgerKindLabel(kind: string): string {
  return kind === "participant_fee"
    ? m.ledger_kind_fee()
    : m.ledger_kind_deposit()
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
