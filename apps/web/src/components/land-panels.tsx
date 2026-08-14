import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  BanIcon,
  CircleCheckIcon,
  CircleXIcon,
  FileTextIcon,
  InboxIcon,
  MapPinIcon,
} from "lucide-react"

import { m } from "#/paraglide/messages"
import { ConfirmAction } from "@/components/confirm-action"
import { FormAlert } from "@/components/form-alert"
import { Panel } from "@/components/panel"
import { QueryBoundary } from "@/components/query-boundary"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { Skeleton } from "@/components/ui/skeleton"
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
import { serverLabel } from "@/lib/server-label"
import { notifySuccess } from "@/lib/toast"

import type { LucideIcon } from "lucide-react"
import type { QueryLike } from "@/components/query-boundary"
import type { LandApplication, LandPlot, LandRefdata } from "@/lib/land"

type StatusView = {
  variant: "info" | "success" | "destructive" | "neutral"
  icon: LucideIcon
}

/**
 * Состояние заявки цветом: предоставлено и отказано различаются не только
 * словом (п. 106). Раньше здесь была одна серая рамка на все четыре исхода,
 * и отказ читался так же, как удовлетворение.
 */
const STATUS_VIEWS: Record<string, StatusView> = {
  submitted: { variant: "info", icon: InboxIcon },
  granted: { variant: "success", icon: CircleCheckIcon },
  refused: { variant: "destructive", icon: CircleXIcon },
  withdrawn: { variant: "neutral", icon: BanIcon },
}

function LandStatusBadge({ status }: { status: string }) {
  const view: StatusView = STATUS_VIEWS[status] ?? {
    variant: "neutral",
    icon: FileTextIcon,
  }

  return (
    <Badge variant={view.variant}>
      <view.icon aria-hidden="true" />
      {landStatusLabel(status)}
    </Badge>
  )
}

/**
 * Земельные участки инвестора (FR-1801, п. 104–105): опубликованные участки
 * и заявка с проектом, объемом инвестиций и сроком. Заявку по неопубликованному
 * участку не примет БД.
 */
export function LandInvestorPanel() {
  const queryClient = useQueryClient()
  const plots = useQuery(landPlotsQuery)
  const mine = useQuery(myLandApplicationsQuery)

  const [plotId, setPlotId] = useState("")
  const [project, setProject] = useState("")
  const [amount, setAmount] = useState("")
  const [term, setTerm] = useState("120")

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
      notifySuccess(m.land_application_submitted_toast())
      await queryClient.invalidateQueries({
        queryKey: myLandApplicationsQuery.queryKey,
      })
    },
  })

  // Раздел показывается по двум спискам сразу: пока не пришли оба, решать
  // «участков нет» не по чему
  const both: QueryLike<{
    published: LandPlot[]
    applications: LandApplication[]
  }> = {
    data:
      plots.data === undefined || mine.data === undefined
        ? undefined
        : {
            published: plots.data.filter((plot) => plot.published_at != null),
            applications: mine.data,
          },
    isPending: plots.isPending || mine.isPending,
    isError: plots.isError || mine.isError,
    error: plots.error ?? mine.error,
    refetch: () => {
      void plots.refetch()
      void mine.refetch()
    },
  }

  return (
    <QueryBoundary
      query={both}
      skeleton={<Skeleton className="h-40 w-full rounded-xl" />}
    >
      {({ published, applications }) =>
        // Инвесторов среди нанимателей меньшинство: когда предлагать и
        // показывать нечего, раздел кабинета не занимает место
        published.length === 0 && applications.length === 0 ? null : (
          <Panel
            title={m.land_investor_title()}
            contentClassName="flex flex-col gap-4"
          >
            {published.length > 0 && (
              <form
                className="flex flex-wrap items-end gap-3"
                onSubmit={(event) => {
                  event.preventDefault()
                  submit.mutate()
                }}
              >
                <div className="flex w-full flex-col gap-1.5 sm:min-w-64">
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
                      <NativeSelectOption
                        key={plot.object_id}
                        value={plot.object_id}
                      >
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
                  <FormAlert className="w-full">
                    {problemMessage(submit.error)}
                  </FormAlert>
                )}
                <Button
                  type="submit"
                  disabled={submit.isPending || plotId === ""}
                >
                  {m.land_submit()}
                </Button>
              </form>
            )}

            {applications.length > 0 && (
              <ul className="flex flex-col gap-3">
                {applications.map((application) => (
                  <li key={application.id}>
                    <ApplicationCard application={application} />
                  </li>
                ))}
              </ul>
            )}
          </Panel>
        )
      }
    </QueryBoundary>
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
        <LandStatusBadge status={application.status} />
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
  const applications = useQuery(landApplicationsQuery)

  return (
    <Panel title={m.land_board_title()}>
      <QueryBoundary
        query={applications}
        empty={{
          when: (data) =>
            data.every((application) => application.status !== "submitted"),
          icon: InboxIcon,
          title: m.land_board_empty(),
        }}
      >
        {(data) => (
          <ul className="flex flex-col gap-3">
            {data
              .filter((application) => application.status === "submitted")
              .map((application) => (
                <li key={application.id}>
                  <DecisionCard application={application} />
                </li>
              ))}
          </ul>
        )}
      </QueryBoundary>
    </Panel>
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
      notifySuccess(m.land_decided_toast())
      await queryClient.invalidateQueries({
        queryKey: landApplicationsQuery.queryKey,
      })
    },
  })

  return (
    <ApplicationCard application={application}>
      {/* Решение Правления - не форма с отправкой: оно необратимо (п. 106),
          и последний шаг здесь - подтверждение, а не нажатие Enter в поле */}
      <div className="flex flex-col gap-3 border-t pt-4">
        <div className="flex w-full max-w-sm flex-col gap-1.5 sm:min-w-64">
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
          <FormAlert>{problemMessage(decide.error)}</FormAlert>
        )}
        <div>
          <ConfirmAction
            title={m.land_decide_confirm_title()}
            description={m.land_decide_confirm_description()}
            confirmLabel={m.land_decide_submit()}
            variant="default"
            disabled={decide.isPending || rationale.trim() === ""}
            onConfirm={() => decide.mutate()}
            trigger={<Button type="button">{m.land_decide_submit()}</Button>}
          />
        </div>
      </div>
    </ApplicationCard>
  )
}

/**
 * Земельные участки организатора (FR-1801, п. 104, 107): характеристики
 * участка и их публикация, а по удовлетворенной заявке - договор с особыми
 * условиями (INV-105 закрепляет их целиком).
 */
export function LandOrganizerPanel() {
  const plots = useQuery(landPlotsQuery)
  const refdata = useQuery(landRefdataQuery)
  const applications = useQuery(landApplicationsQuery)

  const all: QueryLike<{
    plots: LandPlot[]
    refdata: LandRefdata
    granted: LandApplication[]
  }> = {
    data:
      plots.data === undefined ||
      refdata.data === undefined ||
      applications.data === undefined
        ? undefined
        : {
            plots: plots.data,
            refdata: refdata.data,
            granted: applications.data.filter(
              (application) =>
                application.status === "granted" &&
                application.contract_id == null
            ),
          },
    isPending: plots.isPending || refdata.isPending || applications.isPending,
    isError: plots.isError || refdata.isError || applications.isError,
    error: plots.error ?? refdata.error ?? applications.error,
    refetch: () => {
      void plots.refetch()
      void refdata.refetch()
      void applications.refetch()
    },
  }

  return (
    <Panel
      title={m.land_organizer_title()}
      contentClassName="flex flex-col gap-4"
    >
      <QueryBoundary query={all}>
        {(data) => (
          <div className="flex flex-col gap-4">
            <PlotForm designations={data.refdata.designations} />

            {data.plots.length > 0 && (
              <ul className="flex flex-col gap-3">
                {data.plots.map((plot) => (
                  <li key={plot.object_id}>
                    <PlotRow plot={plot} />
                  </li>
                ))}
              </ul>
            )}

            {data.granted.length > 0 && (
              <ul className="flex flex-col gap-3">
                {data.granted.map((application) => (
                  <li key={application.id}>
                    <ContractCard application={application} />
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
      </QueryBoundary>
    </Panel>
  )
}

function PlotForm({
  designations,
}: {
  designations: LandRefdata["designations"]
}) {
  const queryClient = useQueryClient()
  const objects = useQuery({
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
      notifySuccess(m.land_plot_saved_toast())
      await queryClient.invalidateQueries({ queryKey: landPlotsQuery.queryKey })
    },
  })

  return (
    <QueryBoundary
      query={objects}
      skeleton={<Skeleton className="h-40 w-full rounded-xl" />}
      // Участок заводится поверх объекта реестра: пока объектов вида
      // «Земельный участок» нет, заполнять форму не из чего (FR-101)
      empty={{
        when: (data) => data.length === 0,
        icon: MapPinIcon,
        title: m.land_no_objects_title(),
        description: m.land_no_objects(),
      }}
    >
      {(data) => (
        <form
          className="flex flex-wrap items-end gap-3 rounded-lg border p-4"
          onSubmit={(event) => {
            event.preventDefault()
            save.mutate()
          }}
        >
          <div className="flex w-full flex-col gap-1.5 sm:min-w-64">
            <Label htmlFor="land-object">{m.land_object_label()}</Label>
            <NativeSelect
              id="land-object"
              value={objectId}
              onChange={(event) => setObjectId(event.target.value)}
            >
              <NativeSelectOption value="">
                {m.land_plot_none()}
              </NativeSelectOption>
              {data.map((object) => (
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
          <div className="flex w-full flex-col gap-1.5 sm:min-w-56">
            <Label htmlFor="land-designation">
              {m.land_designation_label()}
            </Label>
            <NativeSelect
              id="land-designation"
              value={designation}
              onChange={(event) => setDesignation(event.target.value)}
            >
              {designations.map((item) => (
                <NativeSelectOption key={item.code} value={item.code}>
                  {serverLabel(item)}
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
            <FormAlert className="w-full">
              {problemMessage(save.error)}
            </FormAlert>
          )}
          <Button type="submit" disabled={save.isPending || objectId === ""}>
            {m.land_save_plot()}
          </Button>
        </form>
      )}
    </QueryBoundary>
  )
}

function PlotRow({ plot }: { plot: LandPlot }) {
  const queryClient = useQueryClient()

  const publish = useMutation({
    mutationFn: () => publishLandPlot(plot.object_id),
    onSuccess: async () => {
      notifySuccess(m.land_published_toast())
      await queryClient.invalidateQueries({ queryKey: landPlotsQuery.queryKey })
    },
  })

  return (
    <article className="flex flex-wrap items-center gap-3 rounded-lg border p-4">
      <span className="font-medium">{plot.name}</span>
      <span className="text-sm text-muted-foreground">
        {plot.designation_label} · {plot.cadastral_number}
      </span>
      {plot.published_at == null ? (
        // Публикация открывает участок инвесторам: снять ее портал не умеет
        <ConfirmAction
          title={m.land_publish_confirm_title()}
          description={m.land_publish_confirm_description()}
          confirmLabel={m.land_publish_plot()}
          variant="default"
          disabled={publish.isPending}
          onConfirm={() => publish.mutate()}
          trigger={
            <Button type="button" size="sm" variant="outline">
              {m.land_publish_plot()}
            </Button>
          }
        />
      ) : (
        <span
          className="text-sm text-muted-foreground"
          suppressHydrationWarning
        >
          {m.land_published_at()}: {formatDateTime(plot.published_at)}
        </span>
      )}
      {publish.isError && (
        <FormAlert className="w-full">
          {problemMessage(publish.error)}
        </FormAlert>
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
      notifySuccess(m.land_contract_drafted_toast())
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
        {draft.isError && <FormAlert>{problemMessage(draft.error)}</FormAlert>}
        <div>
          {/* Договор с особыми условиями п. 107 создается один раз:
              переиграть его составление в портале нельзя */}
          <ConfirmAction
            title={m.land_contract_confirm_title()}
            description={m.land_contract_confirm_description()}
            confirmLabel={m.land_contract_submit()}
            variant="default"
            disabled={draft.isPending}
            onConfirm={() => draft.mutate()}
            trigger={
              <Button type="button" size="sm">
                {m.land_contract_submit()}
              </Button>
            }
          />
        </div>
      </div>
    </ApplicationCard>
  )
}
