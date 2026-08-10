import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { problemMessage } from "@/lib/auth"
import {
  amendableFieldsQuery,
  contractAmendmentsQuery,
  createAmendment,
} from "@/lib/contract-amendments"

/**
 * Допсоглашения к договору (FR-906, п. 125): отдельная сущность с
 * diff-контролем. Существенные условия (ставка, объект, срок, наниматель)
 * в перечень изменяемых полей не входят - их не меняет ни форма, ни API
 * (FR-901), поэтому в селекте их попросту нет.
 */
export function ContractAmendmentsPanel({
  contractId,
  canAmend = false,
}: {
  contractId: string
  canAmend?: boolean
}) {
  const queryClient = useQueryClient()
  const { data: amendments } = useQuery(contractAmendmentsQuery(contractId))
  const { data: fields } = useQuery({
    ...amendableFieldsQuery,
    enabled: canAmend,
  })

  const [ground, setGround] = useState("")
  const [effectiveOn, setEffectiveOn] = useState("")
  const [fieldCode, setFieldCode] = useState("")
  const [oldValue, setOldValue] = useState("")
  const [newValue, setNewValue] = useState("")

  const create = useMutation({
    mutationFn: () =>
      createAmendment(contractId, {
        ground,
        effective_on: effectiveOn,
        changes: [
          { field_code: fieldCode, old_value: oldValue, new_value: newValue },
        ],
      }),
    onSuccess: async () => {
      setGround("")
      setOldValue("")
      setNewValue("")
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: contractAmendmentsQuery(contractId).queryKey,
        }),
        // Допсоглашение ложится в досье триггером БД (FR-1602)
        queryClient.invalidateQueries({ queryKey: ["dossier"] }),
      ])
    },
  })

  if (amendments === undefined) return null
  if (amendments.length === 0 && !canAmend) return null

  return (
    <div className="flex flex-col gap-2 border-t pt-3">
      <h4 className="font-medium">{m.amendments_title()}</h4>

      {amendments.length > 0 && (
        <ul className="flex flex-col gap-2 text-sm" data-testid="amendments">
          {amendments.map((amendment) => (
            <li key={amendment.id} className="flex flex-col gap-1">
              <div className="flex flex-wrap items-center gap-x-3">
                <span className="font-medium">
                  {m.amendment_number({ seq: amendment.seq })}
                </span>
                <span suppressHydrationWarning>{amendment.effective_on}</span>
                <span className="text-muted-foreground">
                  {amendment.ground}
                </span>
                {amendment.has_pdf && (
                  <a
                    href={`/api/v1/amendments/${amendment.id}/pdf`}
                    className="underline-offset-4 hover:underline"
                  >
                    {m.amendment_pdf()}
                  </a>
                )}
              </div>
              <ul className="text-muted-foreground">
                {amendment.changes.map((change) => (
                  <li key={change.field_code}>
                    {change.field_label}: {change.old_value} →{" "}
                    {change.new_value}
                  </li>
                ))}
              </ul>
            </li>
          ))}
        </ul>
      )}

      {canAmend && (
        <form
          className="flex flex-wrap items-end gap-2"
          onSubmit={(event) => {
            event.preventDefault()
            create.mutate()
          }}
        >
          <div className="flex min-w-56 flex-col gap-1.5">
            <Label htmlFor={`amend-field-${contractId}`}>
              {m.amendment_field_label()}
            </Label>
            <NativeSelect
              id={`amend-field-${contractId}`}
              value={fieldCode}
              onChange={(event) => setFieldCode(event.target.value)}
            >
              <NativeSelectOption value="">
                {m.amendment_field_none()}
              </NativeSelectOption>
              {(fields ?? []).map((field) => (
                <NativeSelectOption key={field.code} value={field.code}>
                  {field.label_ru}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </div>
          <div className="flex w-44 flex-col gap-1.5">
            <Label htmlFor={`amend-old-${contractId}`}>
              {m.amendment_old_value()}
            </Label>
            <Input
              id={`amend-old-${contractId}`}
              value={oldValue}
              onChange={(event) => setOldValue(event.target.value)}
            />
          </div>
          <div className="flex w-44 flex-col gap-1.5">
            <Label htmlFor={`amend-new-${contractId}`}>
              {m.amendment_new_value()}
            </Label>
            <Input
              id={`amend-new-${contractId}`}
              required
              value={newValue}
              onChange={(event) => setNewValue(event.target.value)}
            />
          </div>
          <div className="flex w-44 flex-col gap-1.5">
            <Label htmlFor={`amend-date-${contractId}`}>
              {m.amendment_effective_on()}
            </Label>
            <Input
              id={`amend-date-${contractId}`}
              type="date"
              required
              value={effectiveOn}
              onChange={(event) => setEffectiveOn(event.target.value)}
            />
          </div>
          <div className="flex w-full flex-col gap-1.5">
            <Label htmlFor={`amend-ground-${contractId}`}>
              {m.amendment_ground_label()}
            </Label>
            <Input
              id={`amend-ground-${contractId}`}
              required
              value={ground}
              onChange={(event) => setGround(event.target.value)}
            />
          </div>
          {create.isError && (
            <p role="alert" className="w-full text-sm text-destructive">
              {problemMessage(create.error)}
            </p>
          )}
          <Button
            type="submit"
            variant="outline"
            size="sm"
            data-testid="create-amendment"
            disabled={create.isPending || fieldCode === ""}
          >
            {m.amendment_submit()}
          </Button>
          <p className="w-full text-sm text-muted-foreground">
            {m.amendments_hint()}
          </p>
        </form>
      )}
    </div>
  )
}
