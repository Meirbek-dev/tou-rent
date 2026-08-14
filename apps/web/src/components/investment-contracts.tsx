import { useRef, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  BanIcon,
  CheckCheckIcon,
  CircleCheckIcon,
  CircleXIcon,
  FilePenIcon,
  FileTextIcon,
  PenLineIcon,
  type LucideIcon,
} from "lucide-react"

import { m } from "#/paraglide/messages"
import { BenefitPanel } from "@/components/benefit-panel"
import { ConfirmAction } from "@/components/confirm-action"
import { FormAlert } from "@/components/form-alert"
import { Panel } from "@/components/panel"
import { QueryBoundary } from "@/components/query-boundary"
import { Badge } from "@/components/ui/badge"
import { Button, buttonVariants } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { Skeleton } from "@/components/ui/skeleton"
import { api } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import { formatDate, formatTenge } from "@/lib/format"
import {
  attachmentLabel,
  extensionLabel,
  investmentAcceptancesQuery,
  investmentAttachmentsQuery,
  investmentContractsQuery,
} from "@/lib/investment"
import { notifySuccess } from "@/lib/toast"
import { cn } from "@/lib/utils"
import { UPLOAD_ACCEPT, uploadError } from "@/lib/validation"

import type { InvestmentAttachment, InvestmentContract } from "@/lib/investment"

/** Что пользователь может делать с инвестиционным договором (A-072). */
export type InvestmentRole = "organizer" | "board" | "secretary"

/**
 * Состояние договора (БД `core.contract_status`).
 *
 * Контракт отдает его строкой, поэтому союз объявлен здесь: исчерпывающие
 * `Record` ниже ломают typecheck, когда в перечень добавят состояние, -
 * и машинный код (`signing`, `terminated`) в интерфейс не попадет.
 */
type ContractStatus =
  | "draft"
  | "signing"
  | "active"
  | "completed"
  | "terminated"
  | "cancelled"

const STATUS_LABELS: Record<ContractStatus, () => string> = {
  draft: m.contract_status_draft,
  signing: m.contract_status_signing,
  active: m.contract_status_active,
  completed: m.contract_status_completed,
  terminated: m.contract_status_terminated,
  cancelled: m.contract_status_cancelled,
}

type StatusView = {
  variant: "info" | "success" | "warning" | "neutral" | "destructive"
  icon: LucideIcon
}

/** Тот же словарь тонов, что у тендера и заявки: цвет значит одно и то же. */
const STATUS_VIEWS: Record<ContractStatus, StatusView> = {
  draft: { variant: "neutral", icon: FilePenIcon },
  signing: { variant: "warning", icon: PenLineIcon },
  active: { variant: "success", icon: CircleCheckIcon },
  completed: { variant: "info", icon: CheckCheckIcon },
  terminated: { variant: "destructive", icon: CircleXIcon },
  cancelled: { variant: "neutral", icon: BanIcon },
}

/** Состояние договора бейджем; неизвестный код наружу не показывается. */
function ContractStatusBadge({ status }: { status: string }) {
  if (!(status in STATUS_VIEWS)) return null
  const { variant, icon: Icon } = STATUS_VIEWS[status as ContractStatus]

  return (
    <Badge variant={variant}>
      <Icon aria-hidden="true" />
      {STATUS_LABELS[status as ContractStatus]()}
    </Badge>
  )
}

// FR-1204 (п. 91–94): инвестиционные договоры - комплект приложений,
// приемка инвестиций и продление. Действия разведены по ролям: организатор
// ведет договор, секретарь оформляет приемку, Правление продлевает.
export function InvestmentContracts({ roles }: { roles: InvestmentRole[] }) {
  const contracts = useQuery(investmentContractsQuery)

  return (
    <Panel title={m.investment_title()}>
      <QueryBoundary
        query={contracts}
        skeleton={<Skeleton className="h-40 w-full rounded-xl" />}
        empty={{
          when: (rows) => rows.length === 0,
          icon: FileTextIcon,
          title: m.investment_empty(),
        }}
      >
        {(rows) => (
          <ul className="flex flex-col gap-4">
            {rows.map((contract) => (
              <li key={contract.id}>
                <ContractCard contract={contract} roles={roles} />
              </li>
            ))}
          </ul>
        )}
      </QueryBoundary>
    </Panel>
  )
}

function ContractCard({
  contract,
  roles,
}: {
  contract: InvestmentContract
  roles: InvestmentRole[]
}) {
  const queryClient = useQueryClient()
  const attachments = useQuery(investmentAttachmentsQuery)
  const acceptances = useQuery(investmentAcceptancesQuery(contract.id))
  const [actDate, setActDate] = useState("")
  const [amount, setAmount] = useState("")

  const refresh = () =>
    Promise.all([
      queryClient.invalidateQueries({
        queryKey: investmentContractsQuery.queryKey,
      }),
      queryClient.invalidateQueries({
        queryKey: ["investment-acceptances", contract.id],
      }),
    ])

  const accept = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST(
        "/api/v1/investment-contracts/{id}/acceptances",
        {
          params: { path: { id: contract.id } },
          body: { act_date: actDate, accepted_amount: amount, note: null },
        }
      )
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("acceptance failed")
      }
      return data
    },
    onSuccess: async () => {
      setAmount("")
      notifySuccess(m.investment_accepted_toast())
      await refresh()
    },
  })

  const extend = useMutation({
    mutationFn: async (extension: string) => {
      const { data, error } = await api.POST(
        "/api/v1/investment-contracts/{id}/extend",
        {
          params: { path: { id: contract.id } },
          body: { extension, months: null },
        }
      )
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("extension failed")
      }
      return data
    },
    onSuccess: async () => {
      notifySuccess(m.investment_extended_toast())
      await refresh()
    },
  })

  return (
    <article className="flex flex-col gap-4 rounded-xl border p-4">
      <header className="flex flex-col gap-2">
        <div className="flex flex-wrap items-center gap-3">
          <ContractStatusBadge status={contract.contract_status} />
          <span className="text-sm text-muted-foreground">
            {contract.object_name} · {contract.tenant_name}
          </span>
        </div>
        <dl className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <div className="flex flex-col gap-0.5">
            <dt className="text-sm text-muted-foreground">
              {m.investment_amount_label()}
            </dt>
            <dd className="font-medium" suppressHydrationWarning>
              {formatTenge(contract.investment_amount)}
            </dd>
          </div>
          <div className="flex flex-col gap-0.5">
            <dt className="text-sm text-muted-foreground">
              {m.investment_accepted_label()}
            </dt>
            <dd className="font-medium" suppressHydrationWarning>
              {formatTenge(contract.accepted_amount)}
              {contract.performance_complete && ` · ${m.investment_complete()}`}
            </dd>
          </div>
          <div className="flex flex-col gap-0.5">
            <dt className="text-sm text-muted-foreground">
              {m.investment_term_label()}
            </dt>
            <dd className="font-medium">
              {m.investment_term_months({ months: contract.term_months })}
              {contract.extension_months != null &&
                ` + ${m.investment_term_months({ months: contract.extension_months })}`}
              {contract.prolongation_months != null &&
                ` + ${m.investment_term_months({ months: contract.prolongation_months })}`}
            </dd>
          </div>
        </dl>

        {contract.missing_attachments.length > 0 ? (
          // Названия позиций перечня п. 91 живут в справочнике: пока он не
          // загружен, показывать коды нечестно - поэтому заглушка
          <QueryBoundary
            query={attachments}
            skeleton={<Skeleton className="h-12 w-full rounded-lg" />}
          >
            {(list) => (
              <p className="rounded-lg border border-dashed p-3 text-sm">
                {m.investment_missing_attachments()}:{" "}
                {contract.missing_attachments
                  .map((item) => attachmentLabel(list, item))
                  .join(", ")}
              </p>
            )}
          </QueryBoundary>
        ) : (
          <p className="text-sm text-muted-foreground">
            {m.investment_attachments_complete()}
          </p>
        )}

        <div className="flex flex-wrap gap-3">
          <a
            href={`/api/v1/investment-contracts/${contract.id}/contract.pdf`}
            className={cn(buttonVariants({ variant: "outline" }))}
          >
            {m.investment_pdf()}
          </a>
          {roles.includes("board") &&
            contract.permitted_extensions.map((extension) => (
              // Продление срока - решение Правления (п. 93): пересмотреть
              // его тем же экраном нельзя
              <ConfirmAction
                key={extension}
                title={m.investment_extend_confirm_title()}
                description={m.investment_extend_confirm_description({
                  extension: extensionLabel(extension),
                })}
                confirmLabel={extensionLabel(extension)}
                variant="default"
                disabled={extend.isPending}
                onConfirm={() => extend.mutate(extension)}
                trigger={
                  <Button variant="outline">{extensionLabel(extension)}</Button>
                }
              />
            ))}
        </div>
        {extend.isError && (
          <FormAlert>{problemMessage(extend.error)}</FormAlert>
        )}
      </header>

      <QueryBoundary
        query={acceptances}
        skeleton={<Skeleton className="h-10 w-full rounded-lg" />}
      >
        {(rows) =>
          rows.length === 0 ? null : (
            <ul className="flex flex-col gap-1 border-t pt-3 text-sm">
              {rows.map((acceptance) => (
                <li key={acceptance.id} suppressHydrationWarning>
                  {formatDate(acceptance.act_date)} -{" "}
                  {formatTenge(acceptance.accepted_amount)}
                  {acceptance.accepted_by_name != null &&
                    ` · ${acceptance.accepted_by_name}`}
                </li>
              ))}
            </ul>
          )
        }
      </QueryBoundary>

      {/* FR-1205 (п. 95–96): льготная схема договора и расписание платы */}
      <BenefitPanel
        contractId={contract.contract_id}
        editable={roles.includes("organizer")}
      />

      {roles.includes("organizer") && (
        <AttachmentForm contractId={contract.id} onUploaded={refresh} />
      )}

      {roles.includes("secretary") && (
        <form
          className="flex flex-col gap-3 border-t pt-4 sm:flex-row sm:flex-wrap sm:items-end"
          onSubmit={(event) => {
            event.preventDefault()
            accept.mutate()
          }}
        >
          <div className="flex w-full flex-col gap-1.5 sm:w-48">
            <Label htmlFor={`act-date-${contract.id}`}>
              {m.investment_act_date()}
            </Label>
            <Input
              id={`act-date-${contract.id}`}
              type="date"
              required
              value={actDate}
              onChange={(event) => setActDate(event.target.value)}
            />
          </div>
          <div className="flex w-full flex-col gap-1.5 sm:w-56">
            <Label htmlFor={`act-amount-${contract.id}`}>
              {m.investment_act_amount()}
            </Label>
            <Input
              id={`act-amount-${contract.id}`}
              type="number"
              min="0.01"
              step="0.01"
              required
              value={amount}
              onChange={(event) => setAmount(event.target.value)}
            />
          </div>
          <Button type="submit" disabled={accept.isPending}>
            {m.investment_accept_submit()}
          </Button>
          {accept.isError && (
            <FormAlert className="w-full">
              {problemMessage(accept.error)}
            </FormAlert>
          )}
        </form>
      )}
    </article>
  )
}

/**
 * Приложение к договору (п. 91): перечень закрыт, и пока справочник не
 * пришел, формы нет - иначе выбор документа стоял бы пустым, а отправить
 * его все равно было бы можно.
 */
function AttachmentForm({
  contractId,
  onUploaded,
}: {
  contractId: string
  onUploaded: () => Promise<unknown>
}) {
  const attachments = useQuery(investmentAttachmentsQuery)

  return (
    <QueryBoundary
      query={attachments}
      skeleton={<Skeleton className="h-20 w-full rounded-lg" />}
    >
      {(list) => (
        <AttachmentFields
          contractId={contractId}
          attachments={list}
          onUploaded={onUploaded}
        />
      )}
    </QueryBoundary>
  )
}

function AttachmentFields({
  contractId,
  attachments,
  onUploaded,
}: {
  contractId: string
  attachments: InvestmentAttachment[]
  onUploaded: () => Promise<unknown>
}) {
  const fileInput = useRef<HTMLInputElement>(null)
  // Приложения договора ложатся в то же досье под Object Lock (INV-042):
  // формат и потолок 10 МБ проверяются до отправки (`upload.rs`)
  const [fileError, setFileError] = useState<string | undefined>(undefined)
  const [code, setCode] = useState(attachments[0]?.code ?? "")

  const upload = useMutation({
    mutationFn: async () => {
      const file = fileInput.current?.files?.[0]
      if (file === undefined) throw new Error(m.file_not_selected())

      const body = new FormData()
      body.append("file", file)
      const { error, response } = await api.POST(
        "/api/v1/investment-contracts/{id}/attachments/{code}",
        {
          params: { path: { id: contractId, code } },
          body: body as unknown as number[],
          bodySerializer: (b: unknown) => b as FormData,
        }
      )
      if (error !== undefined || !response.ok) {
        throw error ?? new Error("upload failed")
      }
    },
    onSuccess: async () => {
      if (fileInput.current) fileInput.current.value = ""
      setFileError(undefined)
      notifySuccess(m.investment_uploaded_toast())
      await onUploaded()
    },
  })

  return (
    <form
      className="flex flex-col gap-3 border-t pt-4 sm:flex-row sm:flex-wrap sm:items-end"
      onSubmit={(event) => {
        event.preventDefault()
        const problem = uploadError(fileInput.current?.files?.[0])
        setFileError(problem)
        if (problem !== undefined) return
        upload.mutate()
      }}
    >
      <div className="flex w-full flex-col gap-1.5 sm:w-auto sm:min-w-56">
        <Label htmlFor={`attachment-${contractId}`}>
          {m.investment_attachment_label()}
        </Label>
        <NativeSelect
          className="w-full"
          id={`attachment-${contractId}`}
          required
          value={code}
          onChange={(event) => setCode(event.target.value)}
        >
          {attachments.map((attachment) => (
            <NativeSelectOption key={attachment.code} value={attachment.code}>
              {attachmentLabel(attachments, attachment.code)}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      </div>
      <div className="flex w-full flex-col gap-1.5 sm:w-auto">
        <Label htmlFor={`attachment-file-${contractId}`}>
          {m.file_upload_label()}
        </Label>
        <Input
          id={`attachment-file-${contractId}`}
          type="file"
          required
          accept={UPLOAD_ACCEPT}
          aria-invalid={fileError !== undefined}
          ref={fileInput}
          onChange={() => {
            const file = fileInput.current?.files?.[0]
            setFileError(file === undefined ? undefined : uploadError(file))
          }}
        />
      </div>
      <Button type="submit" disabled={upload.isPending || code === ""}>
        {m.file_upload_submit()}
      </Button>
      {fileError !== undefined && (
        <FormAlert className="w-full">{fileError}</FormAlert>
      )}
      {upload.isError && (
        <FormAlert className="w-full">{problemMessage(upload.error)}</FormAlert>
      )}
    </form>
  )
}
