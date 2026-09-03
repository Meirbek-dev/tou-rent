import { useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { BuildingIcon } from "lucide-react"
import { m } from "#/paraglide/messages"
import { ConfirmAction } from "@/components/confirm-action"
import { EmptyState } from "@/components/empty-state"
import { FormAlert } from "@/components/form-alert"
import { ObjectStatusBadge } from "@/components/object-status-badge"
import { PageHeader } from "@/components/page-header"
import { Panel } from "@/components/panel"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { api } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import {
  OBJECT_OPTION_FIELDS,
  deleteObject,
  objectsQuery,
  rateOptionLabel,
  rateOptionsQuery,
  updateObject,
} from "@/lib/organizer"
import { problemFieldMessages } from "@/lib/problem-fields"
import { notifyError, notifySuccess } from "@/lib/toast"

import type { FieldLabels } from "@/lib/problem-fields"
import type {
  ObjectDto,
  ObjectKind,
  ObjectOptionField,
  ObjectRequest,
} from "@/lib/organizer"

// FR-101: реестр объектов организатора - список, создание, правка, удаление.
//
// Реестр съехал с обзорной страницы кабинета на собственный адрес: обзор
// отвечает на вопрос «где сегодня работа», а таблица объектов - справочник,
// который открывают намеренно. Форма вынесена в диалог по той же причине:
// развернутой она занимала первый экран у всех, а нужна изредка.
export const Route = createFileRoute("/app/organizer/objects")({
  loader: async ({ context }) => {
    await Promise.all([
      context.queryClient.ensureQueryData(objectsQuery),
      // Опции Прил. 4 нужны форме объекта: коды классификации выбираются
      // из того же справочника, что и в калькуляторе (FR-202)
      context.queryClient.ensureQueryData(rateOptionsQuery),
    ])
  },
  head: () => ({ meta: [{ title: `${m.org_nav_objects()} - ToU Rent` }] }),
  component: ObjectsPage,
})

const KIND_LABELS: Record<ObjectKind, () => string> = {
  building: m.object_kind_building,
  premises: m.object_kind_premises,
  structure: m.object_kind_structure,
  land_plot: m.object_kind_land_plot,
}

/** Подписи полей опций Прил. 4: те же слова в форме и в отказе сервера. */
const OPTION_LABELS: Record<ObjectOptionField, () => string> = {
  premises_type_code: m.object_premises_type_label,
  premises_kind_code: m.object_premises_kind_label,
  comfort_code: m.object_comfort_label,
  location_code: m.object_location_label,
}

/**
 * Поля формы для разбора отказа (G-106).
 *
 * Ключи - имена полей контракта и общие части имен ограничений БД
 * (`premises_type` для `objects_premises_type_option_fkey`): сервер называет
 * поле то так, то этак, а пользователю нужна подпись, которую он видит.
 */
const FIELD_LABELS: FieldLabels = {
  name: m.object_name_ru_label,
  name_kk: m.object_name_kk_label,
  address: m.object_address_ru_label,
  address_kk: m.object_address_kk_label,
  area_m2: m.object_area_label,
  floor_part: m.object_floor_label,
  premises_type: m.object_premises_type_label,
  premises_kind: m.object_premises_kind_label,
  comfort: m.object_comfort_label,
  location: m.object_location_label,
}

function ObjectsPage() {
  const { data: page } = useSuspenseQuery(objectsQuery)

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        title={m.org_nav_objects()}
        description={m.objects_page_hint()}
        actions={<ObjectDialog />}
      />

      {page.items.length === 0 ? (
        <EmptyState
          icon={BuildingIcon}
          title={m.objects_empty_title()}
          description={m.objects_empty()}
          action={<ObjectDialog />}
        />
      ) : (
        <Panel
          title={m.objects_list_title()}
          description={m.objects_count({ count: page.items.length })}
          contentClassName="px-0"
        >
          <div className="overflow-x-auto">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead scope="col">{m.object_name_label()}</TableHead>
                  <TableHead scope="col">{m.object_kind_label()}</TableHead>
                  <TableHead scope="col">{m.object_address_label()}</TableHead>
                  <TableHead scope="col" className="text-right">
                    {m.object_area_label()}
                  </TableHead>
                  <TableHead scope="col">{m.object_status_label()}</TableHead>
                  <TableHead scope="col">{m.object_actions_label()}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {page.items.map((object) => (
                  <TableRow key={object.id}>
                    <TableCell className="font-medium">{object.name}</TableCell>
                    <TableCell>{KIND_LABELS[object.kind]()}</TableCell>
                    <TableCell className="max-w-md whitespace-normal text-muted-foreground">
                      {object.address}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {object.area_m2}
                    </TableCell>
                    <TableCell>
                      <ObjectStatusBadge status={object.status} />
                    </TableCell>
                    <TableCell>
                      <div className="flex flex-wrap items-center gap-2">
                        <ObjectDialog object={object} />
                        <DeleteObject object={object} />
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        </Panel>
      )}
    </div>
  )
}

/** Черновик формы: контракт хочет строки, а поле ввода не знает про null. */
type Draft = {
  kind: ObjectKind
  name: string
  nameKk: string
  address: string
  addressKk: string
  area: string
  floorPart: string
  options: Record<ObjectOptionField, string>
}

function draftFrom(object: ObjectDto | undefined): Draft {
  return {
    kind: object?.kind ?? "premises",
    name: object?.name ?? "",
    nameKk: object?.name_kk ?? "",
    address: object?.address ?? "",
    addressKk: object?.address_kk ?? "",
    area: object?.area_m2 ?? "",
    floorPart: object?.floor_part ?? "",
    options: {
      premises_type_code: object?.premises_type_code ?? "",
      premises_kind_code: object?.premises_kind_code ?? "",
      comfort_code: object?.comfort_code ?? "",
      location_code: object?.location_code ?? "",
    },
  }
}

/** Пустой ввод - это «не указано» (null), а не пустая строка справочника. */
const optional = (value: string) => (value === "" ? null : value)

function requestFrom(draft: Draft): ObjectRequest {
  return {
    kind: draft.kind,
    name: draft.name,
    name_kk: draft.nameKk,
    address: draft.address,
    address_kk: draft.addressKk,
    area_m2: draft.area,
    floor_part: optional(draft.floorPart),
    premises_type_code: optional(draft.options.premises_type_code),
    premises_kind_code: optional(draft.options.premises_kind_code),
    comfort_code: optional(draft.options.comfort_code),
    location_code: optional(draft.options.location_code),
  }
}

/**
 * Создание и правка объекта (FR-101) одной формой: поля у них одни и те же,
 * а держать две копии значило бы забыть про классификацию Прил. 4 в одной
 * из них. Форма открывается кнопкой, а не занимает экран.
 */
function ObjectDialog({ object }: { object?: ObjectDto }) {
  const queryClient = useQueryClient()
  const editing = object !== undefined
  const [open, setOpen] = useState(false)
  const [draft, setDraft] = useState<Draft>(() => draftFrom(object))
  const { data: catalog } = useSuspenseQuery(rateOptionsQuery)
  const idPrefix = editing ? `obj-${object.id}` : "obj-new"

  const save = useMutation({
    mutationFn: async () => {
      const body = requestFrom(draft)
      if (editing) return updateObject(object.id, body)
      const { data, error } = await api.POST("/api/v1/objects", { body })
      if (error !== undefined) throw error
      return data
    },
    onSuccess: async () => {
      if (!editing) setDraft(draftFrom(undefined))
      setOpen(false)
      notifySuccess(
        editing ? m.object_updated_toast() : m.object_created_toast()
      )
      await queryClient.invalidateQueries({ queryKey: objectsQuery.queryKey })
    },
    onError: (error: unknown) => notifyError(problemMessage(error)),
  })

  // Машинную подробность отказа («value_out_of_range», имя FK) сюда не
  // пускаем: поле называется своей подписью, а неразобранное остается
  // общим текстом (G-106)
  const fieldErrors = save.isError
    ? problemFieldMessages(save.error, FIELD_LABELS, "area_m2")
    : []

  const patch = (next: Partial<Draft>) =>
    setDraft((current) => ({ ...current, ...next }))

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        // Диалог правки открывается на актуальной строке реестра, а не на
        // том, что осталось от прошлого раза
        if (next && editing) setDraft(draftFrom(object))
        setOpen(next)
      }}
    >
      <DialogTrigger
        render={
          editing ? (
            <Button
              variant="outline"
              size="sm"
              data-testid={`edit-object-${object.id}`}
            />
          ) : (
            <Button data-testid="open-create-object" />
          )
        }
      >
        {editing ? m.object_edit() : m.object_create_title()}
      </DialogTrigger>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>
            {editing ? m.object_edit_title() : m.object_create_title()}
          </DialogTitle>
          <DialogDescription>
            {editing ? m.object_edit_hint() : m.object_create_hint()}
          </DialogDescription>
        </DialogHeader>
        <form
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault()
            save.mutate()
          }}
        >
          <div className="flex flex-col gap-1.5">
            <Label htmlFor={`${idPrefix}-kind`}>{m.object_kind_label()}</Label>
            <NativeSelect
              id={`${idPrefix}-kind`}
              value={draft.kind}
              onChange={(event) =>
                patch({ kind: event.target.value as ObjectKind })
              }
            >
              {(Object.keys(KIND_LABELS) as ObjectKind[]).map((value) => (
                <NativeSelectOption key={value} value={value}>
                  {KIND_LABELS[value]()}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor={`${idPrefix}-name`}>
              {m.object_name_ru_label()}
            </Label>
            <Input
              id={`${idPrefix}-name`}
              required
              value={draft.name}
              onChange={(event) => patch({ name: event.target.value })}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor={`${idPrefix}-name-kk`}>
              {m.object_name_kk_label()}
            </Label>
            <Input
              id={`${idPrefix}-name-kk`}
              required
              value={draft.nameKk}
              onChange={(event) => patch({ nameKk: event.target.value })}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor={`${idPrefix}-address`}>
              {m.object_address_ru_label()}
            </Label>
            <Input
              id={`${idPrefix}-address`}
              required
              value={draft.address}
              onChange={(event) => patch({ address: event.target.value })}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor={`${idPrefix}-address-kk`}>
              {m.object_address_kk_label()}
            </Label>
            <Input
              id={`${idPrefix}-address-kk`}
              required
              value={draft.addressKk}
              onChange={(event) => patch({ addressKk: event.target.value })}
            />
          </div>
          <div className="flex flex-wrap gap-3">
            <div className="flex w-32 flex-col gap-1.5">
              <Label htmlFor={`${idPrefix}-area`}>
                {m.object_area_label()}
              </Label>
              <Input
                id={`${idPrefix}-area`}
                required
                type="number"
                min="0.01"
                step="0.01"
                value={draft.area}
                onChange={(event) => patch({ area: event.target.value })}
              />
            </div>
            <div className="flex min-w-40 flex-1 flex-col gap-1.5">
              <Label htmlFor={`${idPrefix}-floor`}>
                {m.object_floor_label()}
              </Label>
              <Input
                id={`${idPrefix}-floor`}
                value={draft.floorPart}
                onChange={(event) => patch({ floorPart: event.target.value })}
              />
            </div>
          </div>

          {/* Классификация Прил. 4 (FR-101): без нее объект уходил в тендер
              без коэффициентов, и ставка лота считалась по базовым */}
          <fieldset className="flex flex-col gap-3">
            <legend className="text-sm text-muted-foreground">
              {m.object_options_hint()}
            </legend>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              {OBJECT_OPTION_FIELDS.map(([field, coefficient]) => (
                <div key={field} className="flex flex-col gap-1.5">
                  <Label htmlFor={`${idPrefix}-${field}`}>
                    {OPTION_LABELS[field]()}
                  </Label>
                  <NativeSelect
                    id={`${idPrefix}-${field}`}
                    data-testid={`object-${field}`}
                    value={draft.options[field]}
                    onChange={(event) =>
                      patch({
                        options: {
                          ...draft.options,
                          [field]: event.target.value,
                        },
                      })
                    }
                  >
                    <NativeSelectOption value="">
                      {m.object_option_unset()}
                    </NativeSelectOption>
                    {catalog.options
                      .filter((option) => option.coefficient === coefficient)
                      .map((option) => (
                        <NativeSelectOption
                          key={option.option_code}
                          value={option.option_code}
                        >
                          {rateOptionLabel(option)} ({option.value})
                        </NativeSelectOption>
                      ))}
                  </NativeSelect>
                </div>
              ))}
            </div>
          </fieldset>

          {save.isError && (
            <div className="flex flex-col gap-2">
              {fieldErrors.length === 0 ? (
                <FormAlert>{problemMessage(save.error)}</FormAlert>
              ) : (
                fieldErrors.map((message) => (
                  <FormAlert key={message}>{message}</FormAlert>
                ))
              )}
            </div>
          )}
          <div className="flex justify-end">
            <Button
              type="submit"
              data-testid={editing ? "save-object" : "create-object"}
              disabled={save.isPending}
            >
              {editing ? m.object_edit_submit() : m.object_create_submit()}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}

/**
 * Удаление объекта (FR-101): справочник теряет строку без возможности
 * восстановления, а занятый лотом или договором объект сервер не отдаст -
 * отказ по правилу `object_in_use` остается на экране.
 */
function DeleteObject({ object }: { object: ObjectDto }) {
  const queryClient = useQueryClient()

  const remove = useMutation({
    mutationFn: () => deleteObject(object.id),
    onSuccess: async () => {
      notifySuccess(m.object_deleted_toast())
      await queryClient.invalidateQueries({ queryKey: objectsQuery.queryKey })
    },
    onError: (error: unknown) => notifyError(problemMessage(error)),
  })

  return (
    <ConfirmAction
      title={m.object_delete_confirm_title()}
      description={m.object_delete_confirm_description({ name: object.name })}
      confirmLabel={m.object_delete()}
      variant="destructive-solid"
      disabled={remove.isPending}
      onConfirm={() => remove.mutate()}
      trigger={
        <Button
          type="button"
          variant="destructive"
          size="sm"
          data-testid={`delete-object-${object.id}`}
        >
          {m.object_delete()}
        </Button>
      }
    />
  )
}
