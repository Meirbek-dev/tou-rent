import { useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { InvestmentContracts } from "@/components/investment-contracts"
import { PageHeader } from "@/components/page-header"
import { RateOptionsFields } from "@/components/rate-fields"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { api } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import {
  investmentAttachmentsQuery,
  investmentContractsQuery,
} from "@/lib/investment"
import { defaultRateOptions, rateOptionsQuery } from "@/lib/organizer"
import { specialRequestsQuery } from "@/lib/special"

import type { RateOptions } from "@/lib/organizer"

// FR-1204 (п. 91–94): инвестиционные договоры организатора - составление
// по удовлетворенной заявке, комплект приложений п. 91 и печатная форма.
export const Route = createFileRoute("/app/organizer/investment")({
  loader: async ({ context }) => {
    await Promise.all([
      context.queryClient.ensureQueryData(investmentContractsQuery),
      context.queryClient.ensureQueryData(investmentAttachmentsQuery),
      context.queryClient.ensureQueryData(grantedRequestsQuery),
      context.queryClient.ensureQueryData(rateOptionsQuery),
    ])
  },
  head: () => ({ meta: [{ title: `${m.org_nav_investment()} - ToU Rent` }] }),
  component: InvestmentPage,
})

/** Удовлетворенные заявки, по которым договор еще не составлен (п. 90–91). */
const grantedRequestsQuery = specialRequestsQuery("granted")

function InvestmentPage() {
  const queryClient = useQueryClient()
  const { data: contracts } = useSuspenseQuery(investmentContractsQuery)
  const { data: requests } = useSuspenseQuery(grantedRequestsQuery)

  // Договор составляется по заявке, у которой его еще нет
  const available = requests.filter(
    (request) =>
      request.investment_amount != null &&
      !contracts.some((contract) => contract.special_request_id === request.id)
  )

  const [requestId, setRequestId] = useState("")
  // FR-201 (FR-1403, п. 97): ставку и ее обоснование считает сервер по
  // выбранным коэффициентам Прил. 4 - снимок расчета публикуется
  const [options, setOptions] = useState<RateOptions>(defaultRateOptions())
  const [term, setTerm] = useState("60")

  const draft = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST("/api/v1/investment-contracts", {
        body: {
          special_request_id: requestId,
          rate_options: options,
          term_months: Number(term),
        },
      })
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("draft failed")
      }
      return data
    },
    onSuccess: async () => {
      setOptions(defaultRateOptions())
      await queryClient.invalidateQueries({
        queryKey: investmentContractsQuery.queryKey,
      })
    },
  })

  return (
    <div className="flex flex-col gap-6">
      <PageHeader title={m.org_nav_investment()} />
      {available.length > 0 && (
        <form
          className="flex flex-wrap items-end gap-3 rounded-lg border p-4"
          onSubmit={(event) => {
            event.preventDefault()
            draft.mutate()
          }}
        >
          <div className="flex min-w-64 flex-col gap-1.5">
            <Label htmlFor="investment-request">
              {m.investment_request_label()}
            </Label>
            <NativeSelect
              id="investment-request"
              value={requestId}
              onChange={(event) => setRequestId(event.target.value)}
            >
              <NativeSelectOption value="">
                {m.investment_request_none()}
              </NativeSelectOption>
              {available.map((request) => (
                <NativeSelectOption key={request.id} value={request.id}>
                  {request.category_label} · {request.object_name ?? "-"}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </div>
          <div className="flex w-40 flex-col gap-1.5">
            <Label htmlFor="investment-term">{m.investment_term_label()}</Label>
            <Input
              id="investment-term"
              type="number"
              min="1"
              max="84"
              required
              value={term}
              onChange={(event) => setTerm(event.target.value)}
            />
          </div>
          <Button
            type="submit"
            data-testid="investment-draft-submit"
            disabled={draft.isPending || requestId === ""}
          >
            {m.investment_draft_submit()}
          </Button>
          <div className="w-full">
            <p className="mb-2 text-sm text-muted-foreground">
              {m.investment_rate_hint()}
            </p>
            <RateOptionsFields
              value={options}
              onChange={setOptions}
              idPrefix="investment-rate"
            />
          </div>
          <p className="w-full text-sm text-muted-foreground">
            {m.investment_term_hint()}
          </p>
          {draft.isError && (
            <p role="alert" className="w-full text-sm text-destructive">
              {problemMessage(draft.error)}
            </p>
          )}
        </form>
      )}

      <InvestmentContracts roles={["organizer"]} />
    </div>
  )
}
