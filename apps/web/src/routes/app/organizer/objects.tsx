import { useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { BuildingIcon } from "lucide-react"
import { m } from "#/paraglide/messages"
import { EmptyState } from "@/components/empty-state"
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
import { objectsQuery } from "@/lib/organizer"
import { notifyError, notifySuccess } from "@/lib/toast"

import type { ObjectKind } from "@/lib/organizer"

// FR-101: реестр объектов организатора - список + создание.
//
// Реестр съехал с обзорной страницы кабинета на собственный адрес: обзор
// отвечает на вопрос «где сегодня работа», а таблица объектов - справочник,
// который открывают намеренно. Форма создания вынесена в диалог по той же
// причине: развернутой она занимала первый экран у всех, а нужна изредка.
export const Route = createFileRoute("/app/organizer/objects")({
  loader: ({ context }) => context.queryClient.ensureQueryData(objectsQuery),
  head: () => ({ meta: [{ title: `${m.org_nav_objects()} - ToU Rent` }] }),
  component: ObjectsPage,
})

const KIND_LABELS: Record<ObjectKind, () => string> = {
  building: m.object_kind_building,
  premises: m.object_kind_premises,
  structure: m.object_kind_structure,
  land_plot: m.object_kind_land_plot,
}

function ObjectsPage() {
  const { data: page } = useSuspenseQuery(objectsQuery)

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        title={m.org_nav_objects()}
        description={m.objects_page_hint()}
        actions={<CreateObjectDialog />}
      />

      {page.items.length === 0 ? (
        <EmptyState
          icon={BuildingIcon}
          title={m.objects_empty_title()}
          description={m.objects_empty()}
          action={<CreateObjectDialog />}
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

/** Создание объекта (FR-101): форма открывается кнопкой, а не занимает экран. */
function CreateObjectDialog() {
  const queryClient = useQueryClient()
  const [open, setOpen] = useState(false)

  const [kind, setKind] = useState<ObjectKind>("premises")
  const [name, setName] = useState("")
  const [address, setAddress] = useState("")
  const [area, setArea] = useState("")
  const [floorPart, setFloorPart] = useState("")

  const create = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST("/api/v1/objects", {
        body: {
          kind,
          name,
          address,
          area_m2: area,
          floor_part: floorPart === "" ? null : floorPart,
        },
      })
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("failed to create object")
      }
      return data
    },
    onSuccess: async () => {
      setName("")
      setAddress("")
      setArea("")
      setFloorPart("")
      setOpen(false)
      notifySuccess(m.object_created_toast())
      await queryClient.invalidateQueries({ queryKey: objectsQuery.queryKey })
    },
    onError: (error: unknown) => notifyError(problemMessage(error)),
  })

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button data-testid="open-create-object" />}>
        {m.object_create_title()}
      </DialogTrigger>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{m.object_create_title()}</DialogTitle>
          <DialogDescription>{m.object_create_hint()}</DialogDescription>
        </DialogHeader>
        <form
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault()
            create.mutate()
          }}
        >
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="obj-kind">{m.object_kind_label()}</Label>
            <NativeSelect
              id="obj-kind"
              value={kind}
              onChange={(event) => setKind(event.target.value as ObjectKind)}
            >
              {(Object.keys(KIND_LABELS) as ObjectKind[]).map((value) => (
                <NativeSelectOption key={value} value={value}>
                  {KIND_LABELS[value]()}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="obj-name">{m.object_name_label()}</Label>
            <Input
              id="obj-name"
              required
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="obj-address">{m.object_address_label()}</Label>
            <Input
              id="obj-address"
              required
              value={address}
              onChange={(event) => setAddress(event.target.value)}
            />
          </div>
          <div className="flex flex-wrap gap-3">
            <div className="flex w-32 flex-col gap-1.5">
              <Label htmlFor="obj-area">{m.object_area_label()}</Label>
              <Input
                id="obj-area"
                required
                type="number"
                min="0.01"
                step="0.01"
                value={area}
                onChange={(event) => setArea(event.target.value)}
              />
            </div>
            <div className="flex min-w-40 flex-1 flex-col gap-1.5">
              <Label htmlFor="obj-floor">{m.object_floor_label()}</Label>
              <Input
                id="obj-floor"
                value={floorPart}
                onChange={(event) => setFloorPart(event.target.value)}
              />
            </div>
          </div>
          {create.isError && (
            <p role="alert" className="text-sm text-destructive">
              {problemMessage(create.error)}
            </p>
          )}
          <div className="flex justify-end">
            <Button
              type="submit"
              data-testid="create-object"
              disabled={create.isPending}
            >
              {m.object_create_submit()}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
