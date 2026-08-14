import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { BadgePercentIcon } from "lucide-react"

import { m } from "#/paraglide/messages"
import { FormAlert } from "@/components/form-alert"
import { Panel } from "@/components/panel"
import { QueryBoundary } from "@/components/query-boundary"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { Skeleton } from "@/components/ui/skeleton"
import { api } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import {
  benefitSchemeLabel,
  benefitSchemesQuery,
  contractBenefitQuery,
  yearRuleLabel,
} from "@/lib/benefit"
import { formatTenge } from "@/lib/format"
import { notifySuccess } from "@/lib/toast"

import type { BenefitSchemeDto } from "@/lib/benefit"

// FR-1205 (п. 95–96): льготная схема договора. Первый год - коммунальные
// расходы, со второго доля ставки Прил. 4; условия схемы (согласование
// Ученого совета, кредиты спин-оффа) проверяет сервер - INV-095, INV-096.
export function BenefitPanel({
  contractId,
  editable,
}: {
  contractId: string
  editable: boolean
}) {
  const schemes = useQuery(benefitSchemesQuery)
  const grant = useQuery(contractBenefitQuery(contractId))

  return (
    <Panel
      titleAs="h3"
      title={m.benefit_title()}
      contentClassName="flex flex-col gap-4"
    >
      <QueryBoundary
        query={grant}
        skeleton={<Skeleton className="h-20 w-full rounded-lg" />}
        // `null` - ответ сервера «льгота к договору не применялась»,
        // а не незаконченная загрузка
        empty={{
          when: (data) => data == null,
          icon: BadgePercentIcon,
          title: m.benefit_none_title(),
          description: m.benefit_none(),
        }}
      >
        {(data) =>
          data == null ? null : (
            <div className="flex flex-col gap-2">
              <p className="text-sm">
                {benefitSchemeLabel(data.scheme)}
                {data.council_decision != null &&
                  ` · ${data.council_decision} (${data.council_date ?? "-"})`}
                {data.study_credits > 0 &&
                  ` · ${m.benefit_credits({ credits: data.study_credits })}`}
              </p>
              <ul className="flex flex-col gap-1 text-sm">
                {data.schedule.map((payment) => (
                  <li key={payment.year} suppressHydrationWarning>
                    {m.benefit_year({ year: payment.year })}:{" "}
                    {formatTenge(payment.monthly)} ·{" "}
                    {yearRuleLabel(payment.rule)}
                  </li>
                ))}
              </ul>
            </div>
          )
        }
      </QueryBoundary>

      {editable && (
        <QueryBoundary
          query={schemes}
          skeleton={<Skeleton className="h-24 w-full rounded-lg" />}
        >
          {(list) => <BenefitForm contractId={contractId} schemes={list} />}
        </QueryBoundary>
      )}
    </Panel>
  )
}

/**
 * Форма применения льготы. Живет отдельным компонентом: перечень схем
 * приходит запросом, а выбирать схему до его загрузки не из чего - список
 * условий формы (Ученый совет, кредиты, стажировки) задает сама схема.
 */
function BenefitForm({
  contractId,
  schemes,
}: {
  contractId: string
  schemes: BenefitSchemeDto[]
}) {
  const queryClient = useQueryClient()
  const [scheme, setScheme] = useState("educational_equipment")
  const [communal, setCommunal] = useState("")
  const [council, setCouncil] = useState("")
  const [councilDate, setCouncilDate] = useState("")
  const [credits, setCredits] = useState("0")
  const [internships, setInternships] = useState("0")

  const selected = schemes.find((item) => item.code === scheme)

  const apply = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST("/api/v1/contracts/{id}/benefit", {
        params: { path: { id: contractId } },
        body: {
          scheme,
          communal_monthly: communal === "" ? "0" : communal,
          council_decision: council === "" ? null : council,
          council_date: councilDate === "" ? null : councilDate,
          study_credits: Number(credits),
          internships: Number(internships),
        },
      })
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("benefit failed")
      }
      return data
    },
    onSuccess: async () => {
      notifySuccess(m.benefit_applied_toast())
      await queryClient.invalidateQueries({
        queryKey: contractBenefitQuery(contractId).queryKey,
      })
    },
  })

  return (
    <form
      className="flex flex-wrap items-end gap-3"
      onSubmit={(event) => {
        event.preventDefault()
        apply.mutate()
      }}
    >
      <div className="flex w-full flex-col gap-1.5 sm:min-w-56">
        <Label htmlFor={`benefit-scheme-${contractId}`}>
          {m.benefit_scheme_label()}
        </Label>
        <NativeSelect
          id={`benefit-scheme-${contractId}`}
          value={scheme}
          onChange={(event) => setScheme(event.target.value)}
        >
          {schemes.map((item) => (
            <NativeSelectOption key={item.code} value={item.code}>
              {benefitSchemeLabel(item.code)}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      </div>
      <div className="flex w-48 flex-col gap-1.5">
        <Label htmlFor={`benefit-communal-${contractId}`}>
          {m.benefit_communal_label()}
        </Label>
        <Input
          id={`benefit-communal-${contractId}`}
          type="number"
          min="0"
          step="0.01"
          required
          value={communal}
          onChange={(event) => setCommunal(event.target.value)}
        />
      </div>
      {selected?.requires_council === true && (
        <>
          <div className="flex w-full flex-col gap-1.5 sm:min-w-56">
            <Label htmlFor={`benefit-council-${contractId}`}>
              {m.benefit_council_label()}
            </Label>
            <Input
              id={`benefit-council-${contractId}`}
              required
              value={council}
              onChange={(event) => setCouncil(event.target.value)}
            />
          </div>
          <div className="flex w-44 flex-col gap-1.5">
            <Label htmlFor={`benefit-council-date-${contractId}`}>
              {m.benefit_council_date()}
            </Label>
            <Input
              id={`benefit-council-date-${contractId}`}
              type="date"
              required
              value={councilDate}
              onChange={(event) => setCouncilDate(event.target.value)}
            />
          </div>
        </>
      )}
      {(selected?.min_study_credits ?? 0) > 0 && (
        <div className="flex w-40 flex-col gap-1.5">
          <Label htmlFor={`benefit-credits-${contractId}`}>
            {m.benefit_credits_label()}
          </Label>
          <Input
            id={`benefit-credits-${contractId}`}
            type="number"
            min="0"
            max="60"
            required
            value={credits}
            onChange={(event) => setCredits(event.target.value)}
          />
        </div>
      )}
      {(selected?.internship_quota ?? 0) > 0 && (
        <div className="flex w-40 flex-col gap-1.5">
          <Label htmlFor={`benefit-internships-${contractId}`}>
            {m.benefit_internships_label()}
          </Label>
          <Input
            id={`benefit-internships-${contractId}`}
            type="number"
            min="0"
            required
            value={internships}
            onChange={(event) => setInternships(event.target.value)}
          />
        </div>
      )}
      <Button type="submit" disabled={apply.isPending}>
        {m.benefit_submit()}
      </Button>
      {apply.isError && (
        <FormAlert className="w-full">{problemMessage(apply.error)}</FormAlert>
      )}
    </form>
  )
}
