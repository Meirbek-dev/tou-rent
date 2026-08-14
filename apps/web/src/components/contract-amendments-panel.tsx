import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { m } from "#/paraglide/messages"
import { FormAlert } from "@/components/form-alert"
import { Panel } from "@/components/panel"
import { QueryBoundary } from "@/components/query-boundary"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { Skeleton } from "@/components/ui/skeleton"
import { problemMessage } from "@/lib/auth"
import {
  amendableFieldsQuery,
  contractAmendmentsQuery,
  createAmendment,
} from "@/lib/contract-amendments"
import { formatDate } from "@/lib/format"
import { serverLabel } from "@/lib/server-label"
import { notifySuccess } from "@/lib/toast"

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
  const amendments = useQuery(contractAmendmentsQuery(contractId))

  return (
    <QueryBoundary
      query={amendments}
      skeleton={<Skeleton className="h-24 w-full rounded-xl" />}
    >
      {(rows) => {
        // Ни заключенных допсоглашений, ни права их заключать - показывать
        // нечего. Данные уже пришли: это ответ, а не загрузка
        if (rows.length === 0 && !canAmend) return null

        return (
          <Panel
            titleAs="h3"
            title={m.amendments_title()}
            description={m.amendments_hint()}
            contentClassName="flex flex-col gap-4"
          >
            {rows.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                {m.amendment_empty()}
              </p>
            ) : (
              <ul
                className="flex flex-col gap-2 text-sm"
                data-testid="amendments"
              >
                {rows.map((amendment) => (
                  <li key={amendment.id} className="flex flex-col gap-1">
                    <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                      <span className="font-medium">
                        {m.amendment_number({ seq: amendment.seq })}
                      </span>
                      <span suppressHydrationWarning>
                        {formatDate(amendment.effective_on)}
                      </span>
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

            {canAmend && <AmendmentForm contractId={contractId} />}
          </Panel>
        )
      }}
    </QueryBoundary>
  )
}

/**
 * Форма допсоглашения: перечень изменяемых полей закрыт (п. 125), и пока
 * он не загружен, формы нет вовсе - иначе обязательный выбор поля стоял бы
 * пустым, а отправить его все равно было бы можно.
 */
function AmendmentForm({ contractId }: { contractId: string }) {
  const queryClient = useQueryClient()
  const fields = useQuery(amendableFieldsQuery)

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
      notifySuccess(m.amendment_created_toast())
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: contractAmendmentsQuery(contractId).queryKey,
        }),
        // Допсоглашение ложится в досье триггером БД (FR-1602)
        queryClient.invalidateQueries({ queryKey: ["dossier"] }),
      ])
    },
  })

  return (
    <QueryBoundary
      query={fields}
      skeleton={<Skeleton className="h-40 w-full rounded-xl" />}
    >
      {(items) => (
        <form
          className="grid grid-cols-1 gap-3 border-t pt-4 sm:grid-cols-2"
          onSubmit={(event) => {
            event.preventDefault()
            create.mutate()
          }}
        >
          <div className="flex flex-col gap-1.5">
            <Label htmlFor={`amend-field-${contractId}`}>
              {m.amendment_field_label()}
            </Label>
            <NativeSelect
              className="w-full"
              id={`amend-field-${contractId}`}
              required
              value={fieldCode}
              onChange={(event) => setFieldCode(event.target.value)}
            >
              <NativeSelectOption value="">
                {m.amendment_field_none()}
              </NativeSelectOption>
              {items.map((field) => (
                <NativeSelectOption key={field.code} value={field.code}>
                  {serverLabel(field)}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </div>
          <div className="flex flex-col gap-1.5">
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
          <div className="flex flex-col gap-1.5">
            <Label htmlFor={`amend-old-${contractId}`}>
              {m.amendment_old_value()}
            </Label>
            <Input
              id={`amend-old-${contractId}`}
              value={oldValue}
              onChange={(event) => setOldValue(event.target.value)}
            />
          </div>
          <div className="flex flex-col gap-1.5">
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
          <div className="flex flex-col gap-1.5 sm:col-span-2">
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
            <FormAlert className="sm:col-span-2">
              {problemMessage(create.error)}
            </FormAlert>
          )}
          <div className="sm:col-span-2">
            <Button
              type="submit"
              variant="outline"
              size="sm"
              data-testid="create-amendment"
              disabled={create.isPending || fieldCode === ""}
            >
              {m.amendment_submit()}
            </Button>
          </div>
        </form>
      )}
    </QueryBoundary>
  )
}
