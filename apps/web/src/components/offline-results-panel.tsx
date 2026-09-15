import { useId, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { Panel } from "@/components/panel"
import { QueryBoundary } from "@/components/query-boundary"
import { ConfirmAction } from "@/components/confirm-action"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogTrigger,
} from "@/components/ui/dialog"
import {
  Field,
  FieldGroup,
  FieldLabel,
  FieldSet,
  FieldLegend,
} from "@/components/ui/field"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { Textarea } from "@/components/ui/textarea"
import { problemMessage } from "@/lib/auth"
import { formatDateTime, formatTenge } from "@/lib/format"
import { commissionDocumentsQuery } from "@/lib/commission-documents"
import {
  offlineResultsQuery,
  recordOfflineResults,
} from "@/lib/offline-results"
import { completeOfflineDecisions } from "@/lib/offline-decisions"
import { notifySuccess } from "@/lib/toast"
import type {
  OfflineDecision,
  OfflineResolution,
  OfflineState,
} from "@/lib/offline-results"

export function OfflineResultsPanel({
  tenderId,
  onChanged,
}: {
  tenderId: string
  onChanged: () => Promise<void>
}) {
  const query = useQuery(offlineResultsQuery(tenderId))
  return (
    <Panel title={m.offline_title()} contentClassName="flex flex-col gap-4">
      <QueryBoundary query={query}>
        {(state) =>
          state.recorded ? (
            <OfflineResults state={state} />
          ) : state.eligible ? (
            <OfflineEntry
              tenderId={tenderId}
              state={state}
              onChanged={onChanged}
            />
          ) : (
            <p>{m.offline_unavailable()}</p>
          )
        }
      </QueryBoundary>
    </Panel>
  )
}

function OfflineEntry(props: {
  tenderId: string
  state: OfflineState
  onChanged: () => Promise<void>
}) {
  return (
    <div className="flex flex-col items-start gap-3">
      {props.state.correcting && (
        <p className="text-sm text-destructive">
          {m.offline_correction_help()}
        </p>
      )}
      <p className="text-sm">{m.offline_help()}</p>
      <Dialog>
        <DialogTrigger render={<Button variant="outline" />}>
          {m.offline_enter()}
        </DialogTrigger>
        <DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>{m.offline_title()}</DialogTitle>
            <DialogDescription>{m.offline_help()}</DialogDescription>
          </DialogHeader>
          <OfflineForm {...props} />
        </DialogContent>
      </Dialog>
    </div>
  )
}

function resolutionLabel(resolution: OfflineResolution | null | undefined) {
  switch (resolution) {
    case "no_applications":
      return m.offline_no_applications()
    case "rejected":
      return m.offline_rejected()
    case "single_source":
      return m.offline_single_source()
    default:
      return "—"
  }
}

function OfflineResults({ state }: { state: OfflineState }) {
  return (
    <div className="flex flex-col gap-4">
      <p>
        {m.offline_recorded()} · {formatDateTime(state.recorded_at)}
      </p>
      <a
        className="underline"
        href={`/api/v1/commission-documents/${state.protocol_id}/pdf`}
      >
        {m.offline_protocol()}
      </a>
      <p className="text-sm text-muted-foreground">{m.offline_preserved()}</p>
      {state.superseded_protocol_id !== null && (
        <p className="text-sm text-muted-foreground">
          {m.offline_superseded_protocol()}
        </p>
      )}
      <ol className="flex flex-col gap-4">
        {state.lots.map((lot) => (
          <li key={lot.lot_id} className="flex flex-col gap-1">
            <strong>
              {m.offline_lot({ seq: lot.seq })} ·{" "}
              {lot.applicant ?? m.offline_no_applications()}
            </strong>
            <span>
              {resolutionLabel(lot.resolution)}
              {lot.price == null ? "" : ` · ${formatTenge(lot.price)}`}
            </span>
            <p className="wrap-anywhere whitespace-pre-wrap">{lot.note}</p>
          </li>
        ))}
      </ol>
    </div>
  )
}

function OfflineForm({
  tenderId,
  state,
  onChanged,
}: {
  tenderId: string
  state: OfflineState
  onChanged: () => Promise<void>
}) {
  const id = useId()
  const client = useQueryClient()
  const documents = useQuery(commissionDocumentsQuery(tenderId))
  const [protocolId, setProtocolId] = useState("")
  const [choices, setChoices] = useState<Record<string, OfflineDecision>>({})
  const selected = documents.data?.items.find(
    (doc) => doc.id === protocolId && doc.application_id === null
  )
  const valid =
    selected !== undefined && completeOfflineDecisions(state, choices)
  const save = useMutation({
    mutationFn: () =>
      recordOfflineResults(
        tenderId,
        protocolId,
        state.lots.map((lot) => choices[lot.lot_id]!)
      ),
    onSuccess: async () => {
      notifySuccess(m.offline_recorded())
      await Promise.all([
        client.invalidateQueries({ queryKey: ["offline-results", tenderId] }),
        client.invalidateQueries({ queryKey: ["failure", tenderId] }),
        client.invalidateQueries({ queryKey: ["obligations"] }),
        onChanged(),
      ])
    },
    onError: async () => {
      // A lost response may follow a committed transaction. Refresh before another attempt.
      await client.invalidateQueries({
        queryKey: ["offline-results", tenderId],
      })
    },
  })
  const change = (
    lot: OfflineState["lots"][number],
    patch: Partial<OfflineDecision>
  ) => {
    setChoices((previous) => ({
      ...previous,
      [lot.lot_id]: {
        lot_id: lot.lot_id,
        application_id: lot.application_id,
        note: "",
        ...previous[lot.lot_id],
        ...patch,
      } as OfflineDecision,
    }))
  }
  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm">{m.offline_help()}</p>
      <QueryBoundary query={documents}>
        {(page) => (
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor={`${id}-protocol`}>
                {m.offline_protocol()}
              </FieldLabel>
              <NativeSelect
                id={`${id}-protocol`}
                value={protocolId}
                onChange={(event) => setProtocolId(event.target.value)}
                disabled={save.isPending}
              >
                <NativeSelectOption value="">
                  {m.offline_choose()}
                </NativeSelectOption>
                {page.items
                  .filter((doc) => doc.application_id === null)
                  .map((doc) => (
                    <NativeSelectOption key={doc.id} value={doc.id}>
                      {doc.title} · № {doc.number} · {doc.document_date}
                    </NativeSelectOption>
                  ))}
              </NativeSelect>
              <p className="text-sm text-muted-foreground">
                {m.offline_upload_hint()}
              </p>
            </Field>
            {state.lots.map((lot) => (
              <FieldSet key={lot.lot_id} disabled={save.isPending}>
                <FieldLegend>
                  {m.offline_lot({ seq: lot.seq })} ·{" "}
                  {lot.applicant ?? m.offline_no_applications()}
                </FieldLegend>
                {lot.price != null && (
                  <p>{m.offline_price({ price: lot.price })}</p>
                )}
                <FieldGroup>
                  <Field>
                    <FieldLabel htmlFor={`${id}-${lot.lot_id}-resolution`}>
                      {m.offline_decision()}
                    </FieldLabel>
                    <NativeSelect
                      id={`${id}-${lot.lot_id}-resolution`}
                      value={choices[lot.lot_id]?.resolution ?? ""}
                      onChange={(event) =>
                        change(lot, {
                          resolution: event.target.value as OfflineResolution,
                        })
                      }
                    >
                      <NativeSelectOption value="">
                        {m.offline_choose()}
                      </NativeSelectOption>
                      {lot.application_id === null ? (
                        <NativeSelectOption value="no_applications">
                          {m.offline_no_applications()}
                        </NativeSelectOption>
                      ) : (
                        <>
                          {lot.application_status !== "admitted" && (
                            <NativeSelectOption value="rejected">
                              {m.offline_rejected()}
                            </NativeSelectOption>
                          )}
                          {lot.application_status !== "rejected" &&
                            lot.price !== null && (
                              <NativeSelectOption value="single_source">
                                {m.offline_single_source()}
                              </NativeSelectOption>
                            )}
                        </>
                      )}
                    </NativeSelect>
                  </Field>
                  <Field>
                    <FieldLabel htmlFor={`${id}-${lot.lot_id}-note`}>
                      {m.offline_note()}
                    </FieldLabel>
                    <Textarea
                      id={`${id}-${lot.lot_id}-note`}
                      maxLength={2000}
                      value={choices[lot.lot_id]?.note ?? ""}
                      onChange={(event) =>
                        change(lot, { note: event.target.value })
                      }
                    />
                  </Field>
                </FieldGroup>
              </FieldSet>
            ))}
          </FieldGroup>
        )}
      </QueryBoundary>
      <p className="text-sm text-muted-foreground">{m.offline_preserved()}</p>
      <ConfirmAction
        title={m.offline_confirm_title()}
        description={m.offline_confirm({
          protocol: selected?.title ?? "",
          count: state.lots.length,
        })}
        confirmLabel={m.offline_close()}
        disabled={!valid || save.isPending}
        trigger={
          <Button variant="outline" disabled={!valid || save.isPending}>
            {save.isPending ? m.offline_saving() : m.offline_close()}
          </Button>
        }
        onConfirm={() => {
          if (valid && !save.isPending) save.mutate()
        }}
      />
      {save.isError && (
        <p role="alert" className="text-destructive">
          {problemMessage(save.error)}
        </p>
      )}
    </div>
  )
}
