import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { Textarea } from "@/components/ui/textarea"
import { api } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import { formatDateTime, formatTenge } from "@/lib/format"
import {
  decideLandApplication,
  landApplicationsQuery,
  landDecisionLabel,
  landPlotsQuery,
  landRefdataQuery,
  landStatusLabel,
  myLandApplicationsQuery,
  publishLandPlot,
} from "@/lib/land"

import type { LandApplication, LandPlot } from "@/lib/land"

/**
 * Земельные участки инвестора (FR-1801, п. 104–105): опубликованные участки
 * и заявка с проектом, объемом инвестиций и сроком. Заявку по неопубликованному
 * участку не примет БД.
 */
export function LandInvestorPanel() {
  const queryClient = useQueryClient()
  const { data: plots } = useQuery(landPlotsQuery)
  const { data: mine } = useQuery(myLandApplicationsQuery)

  const [plotId, setPlotId] = useState("")
  const [project, setProject] = useState("")
  const [amount, setAmount] = useState("")
  const [term, setTerm] = useState("120")

  const published = (plots ?? []).filter((plot) => plot.published_at != null)

  const submit = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST("/api/v1/land-applications", {
        body: {
          plot_id: plotId,
          project,
          investment_amount: amount,
          term_months: Number(term),
        },
      })
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("submit failed")
      }
      return data
    },
    onSuccess: async () => {
      setProject("")
      setAmount("")
      await queryClient.invalidateQueries({
        queryKey: myLandApplicationsQuery.queryKey,
      })
    },
  })

  if (published.length === 0 && (mine ?? []).length === 0) return null

  return (
    <section aria-labelledby="land-investor" className="flex flex-col gap-4">
      <h2 id="land-investor" className="font-heading text-lg font-semibold">
        {m.land_investor_title()}
      </h2>

      {published.length > 0 && (
        <form
          className="flex flex-wrap items-end gap-3 rounded-lg border p-4"
          onSubmit={(event) => {
            event.preventDefault()
            submit.mutate()
          }}
        >
          <div className="flex min-w-64 flex-col gap-1.5">
            <Label htmlFor="land-plot">{m.land_plot_label()}</Label>
            <NativeSelect
              id="land-plot"
              value={plotId}
              onChange={(event) => setPlotId(event.target.value)}
            >
              <NativeSelectOption value="">
                {m.land_plot_none()}
              </NativeSelectOption>
              {published.map((plot) => (
                <NativeSelectOption key={plot.object_id} value={plot.object_id}>
                  {plot.name} · {plot.designation_label}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </div>
          <div className="flex w-56 flex-col gap-1.5">
            <Label htmlFor="land-amount">{m.land_amount_label()}</Label>
            <Input
              id="land-amount"
              type="number"
              min="0.01"
              step="0.01"
              required
              value={amount}
              onChange={(event) => setAmount(event.target.value)}
            />
          </div>
          <div className="flex w-40 flex-col gap-1.5">
            <Label htmlFor="land-term">{m.land_term_label()}</Label>
            <Input
              id="land-term"
              type="number"
              min="1"
              max="600"
              required
              value={term}
              onChange={(event) => setTerm(event.target.value)}
            />
          </div>
          <div className="flex w-full flex-col gap-1.5">
            <Label htmlFor="land-project">{m.land_project_label()}</Label>
            <Textarea
              id="land-project"
              required
              rows={3}
              value={project}
              onChange={(event) => setProject(event.target.value)}
            />
          </div>
          {submit.isError && (
            <p role="alert" className="w-full text-sm text-destructive">
              {problemMessage(submit.error)}
            </p>
          )}
          <Button type="submit" disabled={submit.isPending || plotId === ""}>
            {m.land_submit()}
          </Button>
        </form>
      )}

      {(mine ?? []).length > 0 && (
        <ul className="flex flex-col gap-3">
          {(mine ?? []).map((application) => (
            <li key={application.id}>
              <ApplicationCard application={application} />
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}

/** Карточка заявки: состояние, решение Правления и его обоснование (п. 106). */
function ApplicationCard({
  application,
  children,
}: {
  application: LandApplication
  children?: React.ReactNode
}) {
  return (
    <article className="flex flex-col gap-2 rounded-lg border p-4">
      <div className="flex flex-wrap items-center gap-3">
        <span className="rounded-md border px-2 py-0.5 text-sm">
          {landStatusLabel(application.status)}
        </span>
        <span className="text-sm text-muted-foreground">
          {application.plot_name}
        </span>
        <span
          className="text-sm text-muted-foreground"
          suppressHydrationWarning
        >
          {formatDateTime(application.submitted_at)}
        </span>
      </div>
      <p className="text-sm">{application.project}</p>
      <p className="text-sm text-muted-foreground">
        {m.land_amount_label()}: {formatTenge(application.investment_amount)} ·{" "}
        {m.lot_months({ months: application.term_months })}
      </p>
      {application.decision != null && (
        <p className="text-sm">
          {m.land_decision_label()}: {landDecisionLabel(application.decision)}
          {application.rationale != null && ` - ${application.rationale}`}
        </p>
      )}
      {/* INV-105 (п. 107): особые условия договора на участок */}
      {application.covenants.length > 0 && (
        <p className="text-sm text-muted-foreground">
          {m.land_covenants_label()}: {application.covenants.length}
          {application.missing_covenants.length > 0 &&
            ` · ${m.land_covenants_missing({
              count: application.missing_covenants.length,
            })}`}
        </p>
      )}
      {children}
    </article>
  )
}

/**
 * Заявки на участки в кабинете Правления (FR-1801, п. 106): решение
 * с обоснованием принимается по рассматриваемой заявке.
 */
export function LandBoardPanel() {
  const { data: applications } = useQuery(landApplicationsQuery)
  const open = (applications ?? []).filter(
    (application) => application.status === "submitted"
  )
  if (open.length === 0) return null

  return (
    <section aria-labelledby="land-board" className="flex flex-col gap-3">
      <h2 id="land-board" className="font-heading text-lg font-semibold">
        {m.land_board_title()}
      </h2>
      <ul className="flex flex-col gap-3">
        {open.map((application) => (
          <li key={application.id}>
            <DecisionCard application={application} />
          </li>
        ))}
      </ul>
    </section>
  )
}

function DecisionCard({ application }: { application: LandApplication }) {
  const queryClient = useQueryClient()
  const [decision, setDecision] = useState("grant")
  const [rationale, setRationale] = useState("")

  const decide = useMutation({
    mutationFn: () =>
      decideLandApplication(application.id, decision, rationale),
    onSuccess: async () => {
      setRationale("")
      await queryClient.invalidateQueries({
        queryKey: landApplicationsQuery.queryKey,
      })
    },
  })

  return (
    <ApplicationCard application={application}>
      <form
        className="flex flex-col gap-3 border-t pt-4"
        onSubmit={(event) => {
          event.preventDefault()
          decide.mutate()
        }}
      >
        <div className="flex max-w-sm min-w-64 flex-col gap-1.5">
          <Label htmlFor={`land-decision-${application.id}`}>
            {m.land_decision_label()}
          </Label>
          <NativeSelect
            id={`land-decision-${application.id}`}
            value={decision}
            onChange={(event) => setDecision(event.target.value)}
          >
            <NativeSelectOption value="grant">
              {m.land_decision_grant()}
            </NativeSelectOption>
            <NativeSelectOption value="refuse">
              {m.land_decision_refuse()}
            </NativeSelectOption>
          </NativeSelect>
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor={`land-rationale-${application.id}`}>
            {m.special_rationale_label()}
          </Label>
          <Textarea
            id={`land-rationale-${application.id}`}
            required
            rows={3}
            value={rationale}
            onChange={(event) => setRationale(event.target.value)}
          />
        </div>
        {decide.isError && (
          <p role="alert" className="text-sm text-destructive">
            {problemMessage(decide.error)}
          </p>
        )}
        <div>
          <Button type="submit" disabled={decide.isPending}>
            {m.land_decide_submit()}
          </Button>
        </div>
      </form>
    </ApplicationCard>
  )
}

/**
 * Земельные участки организатора (FR-1801, п. 104, 107): характеристики
 * участка и их публикация, а по удовлетворенной заявке - договор с особыми
 * условиями (INV-105 закрепляет их целиком).
 */
export function LandOrganizerPanel() {
  const queryClient = useQueryClient()
  const { data: plots } = useQuery(landPlotsQuery)
  const { data: refdata } = useQuery(landRefdataQuery)
  const { data: applications } = useQuery(landApplicationsQuery)

  if (plots === undefined || refdata === undefined) return null

  const granted = (applications ?? []).filter(
    (application) =>
      application.status === "granted" && application.contract_id == null
  )

  return (
    <section aria-labelledby="land-organizer" className="flex flex-col gap-4">
      <h2 id="land-organizer" className="font-heading text-lg font-semibold">
        {m.land_organizer_title()}
      </h2>

      <PlotForm designations={refdata.designations} />

      {plots.length > 0 && (
        <ul className="flex flex-col gap-3">
          {plots.map((plot) => (
            <li key={plot.object_id}>
              <PlotRow
                plot={plot}
                onPublished={() =>
                  queryClient.invalidateQueries({
                    queryKey: landPlotsQuery.queryKey,
                  })
                }
              />
            </li>
          ))}
        </ul>
      )}

      {granted.length > 0 && (
        <ul className="flex flex-col gap-3">
          {granted.map((application) => (
            <li key={application.id}>
              <ContractCard application={application} />
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}

function PlotForm({
  designations,
}: {
  designations: { code: string; label_ru: string }[]
}) {
  const queryClient = useQueryClient()
  const { data: objects } = useQuery({
    queryKey: ["objects", "land"],
    queryFn: async () => {
      const { data, error } = await api.GET("/api/v1/objects")
      if (error !== undefined || data === undefined) {
        throw (error as unknown) ?? new Error("objects failed")
      }
      return data.items.filter((object) => object.kind === "land_plot")
    },
  })

  const [objectId, setObjectId] = useState("")
  const [cadastral, setCadastral] = useState("")
  const [designation, setDesignation] = useState(designations[0]?.code ?? "")
  const [use, setUse] = useState("")

  const save = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST("/api/v1/land-plots", {
        body: {
          object_id: objectId,
          cadastral_number: cadastral,
          designation,
          permitted_use: use,
        },
      })
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("save failed")
      }
      return data
    },
    onSuccess: async () => {
      setCadastral("")
      setUse("")
      await queryClient.invalidateQueries({ queryKey: landPlotsQuery.queryKey })
    },
  })

  if ((objects ?? []).length === 0) {
    return (
      <p className="text-sm text-muted-foreground">{m.land_no_objects()}</p>
    )
  }

  return (
    <form
      className="flex flex-wrap items-end gap-3 rounded-lg border p-4"
      onSubmit={(event) => {
        event.preventDefault()
        save.mutate()
      }}
    >
      <div className="flex min-w-64 flex-col gap-1.5">
        <Label htmlFor="land-object">{m.land_object_label()}</Label>
        <NativeSelect
          id="land-object"
          value={objectId}
          onChange={(event) => setObjectId(event.target.value)}
        >
          <NativeSelectOption value="">{m.land_plot_none()}</NativeSelectOption>
          {(objects ?? []).map((object) => (
            <NativeSelectOption key={object.id} value={object.id}>
              {object.name}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      </div>
      <div className="flex w-56 flex-col gap-1.5">
        <Label htmlFor="land-cadastral">{m.land_plot_cadastral()}</Label>
        <Input
          id="land-cadastral"
          required
          value={cadastral}
          onChange={(event) => setCadastral(event.target.value)}
        />
      </div>
      <div className="flex min-w-56 flex-col gap-1.5">
        <Label htmlFor="land-designation">{m.land_designation_label()}</Label>
        <NativeSelect
          id="land-designation"
          value={designation}
          onChange={(event) => setDesignation(event.target.value)}
        >
          {designations.map((item) => (
            <NativeSelectOption key={item.code} value={item.code}>
              {item.label_ru}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      </div>
      <div className="flex w-full flex-col gap-1.5">
        <Label htmlFor="land-use">{m.land_plot_permitted_use()}</Label>
        <Input
          id="land-use"
          required
          value={use}
          onChange={(event) => setUse(event.target.value)}
        />
      </div>
      {save.isError && (
        <p role="alert" className="w-full text-sm text-destructive">
          {problemMessage(save.error)}
        </p>
      )}
      <Button type="submit" disabled={save.isPending || objectId === ""}>
        {m.land_save_plot()}
      </Button>
    </form>
  )
}

function PlotRow({
  plot,
  onPublished,
}: {
  plot: LandPlot
  onPublished: () => Promise<void> | void
}) {
  const publish = useMutation({
    mutationFn: () => publishLandPlot(plot.object_id),
    onSuccess: () => onPublished(),
  })

  return (
    <article className="flex flex-wrap items-center gap-3 rounded-lg border p-4">
      <span className="font-medium">{plot.name}</span>
      <span className="text-sm text-muted-foreground">
        {plot.designation_label} · {plot.cadastral_number}
      </span>
      {plot.published_at == null ? (
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={publish.isPending}
          onClick={() => publish.mutate()}
        >
          {m.land_publish_plot()}
        </Button>
      ) : (
        <span
          className="text-sm text-muted-foreground"
          suppressHydrationWarning
        >
          {m.land_published_at()}: {formatDateTime(plot.published_at)}
        </span>
      )}
      {publish.isError && (
        <p role="alert" className="w-full text-sm text-destructive">
          {problemMessage(publish.error)}
        </p>
      )}
    </article>
  )
}

function ContractCard({ application }: { application: LandApplication }) {
  const queryClient = useQueryClient()

  const draft = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST("/api/v1/land-contracts", {
        body: { land_application_id: application.id },
      })
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("draft failed")
      }
      return data
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: landApplicationsQuery.queryKey,
      })
    },
  })

  return (
    <ApplicationCard application={application}>
      <div className="flex flex-col gap-2 border-t pt-4">
        <p className="text-sm text-muted-foreground">
          {m.land_contract_hint()}
        </p>
        {draft.isError && (
          <p role="alert" className="text-sm text-destructive">
            {problemMessage(draft.error)}
          </p>
        )}
        <div>
          <Button
            type="button"
            size="sm"
            disabled={draft.isPending}
            onClick={() => draft.mutate()}
          >
            {m.land_contract_submit()}
          </Button>
        </div>
      </div>
    </ApplicationCard>
  )
}
