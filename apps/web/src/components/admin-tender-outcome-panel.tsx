import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { m } from "#/paraglide/messages"
import { ConfirmAction } from "@/components/confirm-action"
import { Panel } from "@/components/panel"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { Textarea } from "@/components/ui/textarea"
import {
  adminSuccessfulOutcomeQuery,
  dataOverviewQuery,
  recordAdminSuccessfulOutcome,
  tenderSchedulesQuery,
} from "@/lib/admin"
import { problemMessage } from "@/lib/auth"
import { notifyError, notifySuccess } from "@/lib/toast"

import type { AdminSuccessfulOutcomeStateDto } from "@/lib/admin"

type Decision = { application_id: string; price: string }

/**
 * Не универсальный редактор статуса, а одна auditable-операция:
 * failed -> summed_up на основании подписанного документа и решений по лотам.
 */
export function AdminTenderOutcomePanel() {
  const { data: overview } = useQuery(dataOverviewQuery)
  const { data: schedules } = useQuery(tenderSchedulesQuery)
  const [tenderId, setTenderId] = useState("")

  if (overview === undefined || schedules === undefined) return null
  const failed = schedules.items.filter((tender) => tender.status === "failed")
  const selectedId = failed.some((tender) => tender.id === tenderId)
    ? tenderId
    : ""

  return (
    <div className="flex flex-col gap-6">
      {!overview.purge_enabled && (
        <div
          role="alert"
          className="flex flex-col gap-1 rounded-lg border border-destructive p-3 text-sm"
        >
          <span className="font-medium">
            {m.admin_outcome_disabled_title()}
          </span>
          <span className="text-muted-foreground">
            {m.admin_outcome_disabled_hint()}
          </span>
        </div>
      )}
      <Panel
        title={m.admin_outcome_title()}
        description={m.admin_outcome_hint()}
      >
        {failed.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {m.admin_outcome_empty()}
          </p>
        ) : (
          <Field className="max-w-2xl">
            <FieldLabel htmlFor="admin-outcome-tender">
              {m.admin_outcome_tender()}
            </FieldLabel>
            <NativeSelect
              id="admin-outcome-tender"
              className="w-full"
              value={selectedId}
              onChange={(event) => setTenderId(event.target.value)}
            >
              <NativeSelectOption value="">
                {m.admin_outcome_select_tender()}
              </NativeSelectOption>
              {failed.map((tender) => (
                <NativeSelectOption key={tender.id} value={tender.id}>
                  {tender.title}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </Field>
        )}
      </Panel>
      {selectedId !== "" && (
        <OutcomeLoader
          key={selectedId}
          tenderId={selectedId}
          enabled={overview.purge_enabled}
          onDone={() => setTenderId("")}
        />
      )}
    </div>
  )
}

function OutcomeLoader({
  tenderId,
  enabled,
  onDone,
}: {
  tenderId: string
  enabled: boolean
  onDone: () => void
}) {
  const { data, error } = useQuery(adminSuccessfulOutcomeQuery(tenderId))
  if (error !== null) {
    return (
      <p role="alert" className="text-sm text-destructive">
        {problemMessage(error)}
      </p>
    )
  }
  if (data === undefined) return null
  return <OutcomeForm state={data} enabled={enabled} onDone={onDone} />
}

function initialDecisions(state: AdminSuccessfulOutcomeStateDto) {
  return Object.fromEntries(
    state.lots.map((lot) => {
      const application =
        lot.applications.length === 1 ? lot.applications[0] : undefined
      return [
        lot.id,
        {
          application_id: application?.id ?? "",
          price: application?.price ?? "",
        },
      ]
    })
  ) as Record<string, Decision>
}

function OutcomeForm({
  state,
  enabled,
  onDone,
}: {
  state: AdminSuccessfulOutcomeStateDto
  enabled: boolean
  onDone: () => void
}) {
  const queryClient = useQueryClient()
  const [protocolId, setProtocolId] = useState(state.protocols[0]?.id ?? "")
  const [reason, setReason] = useState("")
  const [decisions, setDecisions] = useState(() => initialDecisions(state))
  const complete =
    enabled &&
    state.eligible &&
    protocolId !== "" &&
    reason.trim() !== "" &&
    state.lots.length > 0 &&
    state.lots.every((lot) => {
      const decision = decisions[lot.id]
      return decision?.application_id !== "" && Number(decision?.price) > 0
    })

  const save = useMutation({
    mutationFn: () =>
      recordAdminSuccessfulOutcome(state.tender_id, {
        protocol_id: protocolId,
        reason: reason.trim(),
        confirmed_signed_protocol: true,
        lots: state.lots.map((lot) => ({
          lot_id: lot.id,
          application_id: decisions[lot.id]?.application_id ?? "",
          price: decisions[lot.id]?.price.trim() ?? "",
        })),
      }),
    onSuccess: async () => {
      notifySuccess(m.admin_outcome_saved_toast({ title: state.title }))
      await queryClient.invalidateQueries()
      onDone()
    },
    onError: (error: unknown) => notifyError(problemMessage(error)),
  })

  return (
    <Panel
      title={m.admin_outcome_editing({ title: state.title })}
      description={m.admin_outcome_editor_hint()}
    >
      <form
        className="flex flex-col gap-5"
        onSubmit={(event) => event.preventDefault()}
      >
        <Field>
          <FieldLabel htmlFor="admin-outcome-protocol">
            {m.admin_outcome_protocol()}
          </FieldLabel>
          <NativeSelect
            id="admin-outcome-protocol"
            className="w-full"
            value={protocolId}
            onChange={(event) => setProtocolId(event.target.value)}
          >
            <NativeSelectOption value="">
              {m.admin_outcome_select_protocol()}
            </NativeSelectOption>
            {state.protocols.map((protocol) => (
              <NativeSelectOption key={protocol.id} value={protocol.id}>
                {protocol.title} · {protocol.number} · {protocol.document_date}
              </NativeSelectOption>
            ))}
          </NativeSelect>
          {state.protocols.length === 0 && (
            <FieldDescription className="text-destructive">
              {m.admin_outcome_no_protocols()}
            </FieldDescription>
          )}
        </Field>

        <Field>
          <FieldLabel htmlFor="admin-outcome-reason">
            {m.admin_outcome_reason()}
          </FieldLabel>
          <Textarea
            id="admin-outcome-reason"
            maxLength={2000}
            value={reason}
            onChange={(event) => setReason(event.target.value)}
          />
        </Field>

        <div className="flex flex-col gap-4">
          {state.lots.map((lot) => {
            const decision = decisions[lot.id] ?? {
              application_id: "",
              price: "",
            }
            return (
              <fieldset key={lot.id} className="rounded-lg border p-4">
                <legend className="px-1 font-medium">
                  {m.admin_outcome_lot({ seq: lot.seq })}: {lot.purpose}
                </legend>
                <div className="grid gap-4 md:grid-cols-2">
                  <Field>
                    <FieldLabel htmlFor={`outcome-app-${lot.id}`}>
                      {m.admin_outcome_winner()}
                    </FieldLabel>
                    <NativeSelect
                      id={`outcome-app-${lot.id}`}
                      className="w-full"
                      value={decision.application_id}
                      onChange={(event) => {
                        const application = lot.applications.find(
                          (item) => item.id === event.target.value
                        )
                        setDecisions((current) => ({
                          ...current,
                          [lot.id]: {
                            application_id: event.target.value,
                            price: application?.price ?? "",
                          },
                        }))
                      }}
                    >
                      <NativeSelectOption value="">
                        {m.admin_outcome_select_application()}
                      </NativeSelectOption>
                      {lot.applications.map((application) => (
                        <NativeSelectOption
                          key={application.id}
                          value={application.id}
                        >
                          {application.applicant}
                        </NativeSelectOption>
                      ))}
                    </NativeSelect>
                    {lot.applications.length === 0 && (
                      <FieldDescription className="text-destructive">
                        {m.admin_outcome_no_applications()}
                      </FieldDescription>
                    )}
                  </Field>
                  <Field>
                    <FieldLabel htmlFor={`outcome-price-${lot.id}`}>
                      {m.admin_outcome_price()}
                    </FieldLabel>
                    <Input
                      id={`outcome-price-${lot.id}`}
                      inputMode="decimal"
                      value={decision.price}
                      onChange={(event) =>
                        setDecisions((current) => ({
                          ...current,
                          [lot.id]: {
                            ...decision,
                            price: event.target.value,
                          },
                        }))
                      }
                    />
                  </Field>
                </div>
              </fieldset>
            )
          })}
        </div>

        <ConfirmAction
          title={m.admin_outcome_confirm_title()}
          description={m.admin_outcome_confirm_description({
            title: state.title,
          })}
          confirmLabel={m.admin_outcome_save()}
          variant="default"
          disabled={!complete || save.isPending}
          onConfirm={() => save.mutate()}
          trigger={<Button type="button">{m.admin_outcome_save()}</Button>}
        />
      </form>
    </Panel>
  )
}
