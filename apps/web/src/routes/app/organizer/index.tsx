import { useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { MyDeadlines } from "@/components/my-deadlines"
import { Button } from "@/components/ui/button"
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

import type { ObjectKind } from "@/lib/organizer"

// FR-101: реестр объектов организатора - список + создание.
export const Route = createFileRoute("/app/organizer/")({
  loader: ({ context }) => context.queryClient.ensureQueryData(objectsQuery),
  component: ObjectsPage,
})

const KIND_LABELS: Record<ObjectKind, () => string> = {
  building: m.object_kind_building,
  premises: m.object_kind_premises,
  structure: m.object_kind_structure,
  land_plot: m.object_kind_land_plot,
}

const STATUS_LABELS: Record<string, () => string> = {
  free: m.object_status_free,
  in_tender: m.object_status_in_tender,
  leased: m.object_status_leased,
}

function ObjectsPage() {
  const { data: page } = useSuspenseQuery(objectsQuery)
  const queryClient = useQueryClient()

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
      await queryClient.invalidateQueries({ queryKey: objectsQuery.queryKey })
    },
  })

  return (
    <div className="flex flex-col gap-8">
      <MyDeadlines />

      <section aria-labelledby="objects-create">
        <h2
          id="objects-create"
          className="mb-3 font-heading text-lg font-semibold"
        >
          {m.object_create_title()}
        </h2>
        <form
          className="flex flex-wrap items-end gap-3 rounded-lg border p-4"
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
          <div className="flex min-w-56 flex-1 flex-col gap-1.5">
            <Label htmlFor="obj-name">{m.object_name_label()}</Label>
            <Input
              id="obj-name"
              required
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </div>
          <div className="flex min-w-56 flex-1 flex-col gap-1.5">
            <Label htmlFor="obj-address">{m.object_address_label()}</Label>
            <Input
              id="obj-address"
              required
              value={address}
              onChange={(event) => setAddress(event.target.value)}
            />
          </div>
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
          <div className="flex w-40 flex-col gap-1.5">
            <Label htmlFor="obj-floor">{m.object_floor_label()}</Label>
            <Input
              id="obj-floor"
              value={floorPart}
              onChange={(event) => setFloorPart(event.target.value)}
            />
          </div>
          <Button
            type="submit"
            data-testid="create-object"
            disabled={create.isPending}
          >
            {m.object_create_submit()}
          </Button>
          {create.isError && (
            <p role="alert" className="w-full text-sm text-destructive">
              {problemMessage(create.error)}
            </p>
          )}
        </form>
      </section>

      <section aria-labelledby="objects-list">
        <h2
          id="objects-list"
          className="mb-3 font-heading text-lg font-semibold"
        >
          {m.objects_list_title()}
        </h2>
        {page.items.length === 0 ? (
          <p className="text-muted-foreground">{m.objects_empty()}</p>
        ) : (
          <div className="rounded-lg border">
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
                      {STATUS_LABELS[object.status]?.() ?? object.status}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
      </section>
    </div>
  )
}
