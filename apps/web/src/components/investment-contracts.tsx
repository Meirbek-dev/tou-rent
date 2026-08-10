import { useRef, useState } from "react"
import {
  useMutation,
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { BenefitPanel } from "@/components/benefit-panel"
import { Button, buttonVariants } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { api } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import { formatTenge } from "@/lib/format"
import {
  attachmentLabel,
  extensionLabel,
  investmentAcceptancesQuery,
  investmentAttachmentsQuery,
  investmentContractsQuery,
} from "@/lib/investment"
import { cn } from "@/lib/utils"

import type { InvestmentContract } from "@/lib/investment"

/** Что пользователь может делать с инвестиционным договором (A-072). */
export type InvestmentRole = "organizer" | "board" | "secretary"

// FR-1204 (п. 91–94): инвестиционные договоры - комплект приложений,
// приемка инвестиций и продление. Действия разведены по ролям: организатор
// ведет договор, секретарь оформляет приемку, Правление продлевает.
export function InvestmentContracts({ roles }: { roles: InvestmentRole[] }) {
  const { data: contracts } = useSuspenseQuery(investmentContractsQuery)

  return (
    <section aria-labelledby="investment-contracts">
      <h2
        id="investment-contracts"
        className="mb-3 font-heading text-lg font-semibold"
      >
        {m.investment_title()}
      </h2>
      {contracts.length === 0 ? (
        <p className="text-muted-foreground">{m.investment_empty()}</p>
      ) : (
        <ul className="flex flex-col gap-4">
          {contracts.map((contract) => (
            <li key={contract.id}>
              <ContractCard contract={contract} roles={roles} />
            </li>
          ))}
        </ul>
      )}
    </section>
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
  const { data: attachments } = useSuspenseQuery(investmentAttachmentsQuery)
  const { data: acceptances } = useQuery(
    investmentAcceptancesQuery(contract.id)
  )
  const fileInput = useRef<HTMLInputElement>(null)
  const [code, setCode] = useState(attachments[0]?.code ?? "")
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

  const upload = useMutation({
    mutationFn: async () => {
      const file = fileInput.current?.files?.[0]
      if (file === undefined) throw new Error(m.file_not_selected())

      const body = new FormData()
      body.append("file", file)
      const { error, response } = await api.POST(
        "/api/v1/investment-contracts/{id}/attachments/{code}",
        {
          params: { path: { id: contract.id, code } },
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
      await refresh()
    },
  })

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
    onSuccess: refresh,
  })

  return (
    <article className="flex flex-col gap-4 rounded-lg border p-4">
      <header className="flex flex-col gap-2">
        <div className="flex flex-wrap items-center gap-3">
          <span className="rounded-md border px-2 py-0.5 text-sm">
            {contract.contract_status}
          </span>
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
          <p className="rounded-lg border border-dashed p-3 text-sm">
            {m.investment_missing_attachments()}:{" "}
            {contract.missing_attachments
              .map((item) => attachmentLabel(attachments, item))
              .join(", ")}
          </p>
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
              <Button
                key={extension}
                variant="outline"
                onClick={() => extend.mutate(extension)}
                disabled={extend.isPending}
              >
                {extensionLabel(extension)}
              </Button>
            ))}
        </div>
        {extend.isError && (
          <p role="alert" className="text-sm text-destructive">
            {problemMessage(extend.error)}
          </p>
        )}
      </header>

      {(acceptances?.length ?? 0) > 0 && (
        <ul className="flex flex-col gap-1 border-t pt-3 text-sm">
          {acceptances?.map((acceptance) => (
            <li key={acceptance.id} suppressHydrationWarning>
              {acceptance.act_date} - {formatTenge(acceptance.accepted_amount)}
              {acceptance.accepted_by_name != null &&
                ` · ${acceptance.accepted_by_name}`}
            </li>
          ))}
        </ul>
      )}

      {/* FR-1205 (п. 95–96): льготная схема договора и расписание платы */}
      <BenefitPanel
        contractId={contract.contract_id}
        editable={roles.includes("organizer")}
      />

      {roles.includes("organizer") && (
        <form
          className="flex flex-wrap items-end gap-3 border-t pt-4"
          onSubmit={(event) => {
            event.preventDefault()
            upload.mutate()
          }}
        >
          <div className="flex min-w-56 flex-col gap-1.5">
            <Label htmlFor={`attachment-${contract.id}`}>
              {m.investment_attachment_label()}
            </Label>
            <NativeSelect
              id={`attachment-${contract.id}`}
              value={code}
              onChange={(event) => setCode(event.target.value)}
            >
              {attachments.map((attachment) => (
                <NativeSelectOption
                  key={attachment.code}
                  value={attachment.code}
                >
                  {attachmentLabel(attachments, attachment.code)}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor={`attachment-file-${contract.id}`}>
              {m.file_upload_label()}
            </Label>
            <Input
              id={`attachment-file-${contract.id}`}
              type="file"
              required
              ref={fileInput}
            />
          </div>
          <Button type="submit" disabled={upload.isPending}>
            {m.file_upload_submit()}
          </Button>
          {upload.isError && (
            <p role="alert" className="w-full text-sm text-destructive">
              {problemMessage(upload.error)}
            </p>
          )}
        </form>
      )}

      {roles.includes("secretary") && (
        <form
          className="flex flex-wrap items-end gap-3 border-t pt-4"
          onSubmit={(event) => {
            event.preventDefault()
            accept.mutate()
          }}
        >
          <div className="flex w-48 flex-col gap-1.5">
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
          <div className="flex w-56 flex-col gap-1.5">
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
            <p role="alert" className="w-full text-sm text-destructive">
              {problemMessage(accept.error)}
            </p>
          )}
        </form>
      )}
    </article>
  )
}
