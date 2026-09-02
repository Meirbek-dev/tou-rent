import { useState } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { BuildingIcon } from "lucide-react"
import { Link } from "@tanstack/react-router"
import { m } from "#/paraglide/messages"
import { EmptyState } from "@/components/empty-state"
import {
  LotDraftFields,
  emptyLot,
  lotDraftToRequest,
} from "@/components/lot-draft-fields"
import { Button, buttonVariants } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { api } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import {
  objectsQuery,
  organizerTendersQuery,
  rateOptionsQuery,
} from "@/lib/organizer"

import type { LotDraft } from "@/components/lot-draft-fields"

// FR-301: тендер с лотами; снимок ставки считает сервер по опциям Прил. 4.
export const Route = createFileRoute("/app/organizer/tenders/new")({
  loader: async ({ context }) => {
    await Promise.all([
      context.queryClient.ensureQueryData(objectsQuery),
      context.queryClient.ensureQueryData(rateOptionsQuery),
    ])
  },
  head: () => ({ meta: [{ title: `${m.tender_create_title()} - ToU Rent` }] }),
  component: NewTenderPage,
})

function NewTenderPage() {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { data: objectsPage } = useSuspenseQuery(objectsQuery)
  const firstObjectId = objectsPage.items[0]?.id ?? ""

  const [title, setTitle] = useState("")
  const [titleKk, setTitleKk] = useState("")
  const [lots, setLots] = useState<LotDraft[]>([emptyLot(firstObjectId)])

  const patchLot = (index: number, patch: Partial<LotDraft>) => {
    setLots((current) =>
      current.map((lot, i) => (i === index ? { ...lot, ...patch } : lot))
    )
  }

  const create = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST("/api/v1/tenders", {
        body: {
          title,
          title_kk: titleKk,
          lots: lots.map(lotDraftToRequest),
        },
      })
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("failed to create tender")
      }
      return data
    },
    onSuccess: async (tender) => {
      await queryClient.invalidateQueries({
        queryKey: organizerTendersQuery.queryKey,
      })
      await navigate({
        to: "/app/organizer/tenders/$tenderId",
        params: { tenderId: tender.id },
      })
    },
  })

  // Тупик «объектов нет» был абзацем без выхода: лот без объекта не создать,
  // а куда идти заводить объект - страница не говорила
  if (objectsPage.items.length === 0) {
    return (
      <EmptyState
        icon={BuildingIcon}
        titleAs="h1"
        title={m.objects_empty_title()}
        description={m.tender_new_no_objects()}
        action={
          <Link to="/app/organizer/objects" className={buttonVariants()}>
            {m.object_create_title()}
          </Link>
        }
      />
    )
  }

  return (
    <form
      className="flex flex-col gap-6"
      onSubmit={(event) => {
        event.preventDefault()
        create.mutate()
      }}
    >
      <h1 className="font-heading text-2xl font-semibold">
        {m.tender_create_title()}
      </h1>

      <div className="grid gap-4 md:grid-cols-2">
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="tender-title">{m.tender_title_ru_label()}</Label>
          <Input
            id="tender-title"
            required
            value={title}
            onChange={(event) => setTitle(event.target.value)}
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="tender-title-kk">{m.tender_title_kk_label()}</Label>
          <Input
            id="tender-title-kk"
            required
            value={titleKk}
            onChange={(event) => setTitleKk(event.target.value)}
          />
        </div>
      </div>

      {lots.map((lot, index) => (
        <LotDraftFields
          key={index}
          lot={lot}
          n={index + 1}
          idPrefix={`lot-${index}`}
          objects={objectsPage.items}
          onChange={(patch) => patchLot(index, patch)}
          onRemove={
            lots.length > 1
              ? () =>
                  setLots((current) => current.filter((_, i) => i !== index))
              : undefined
          }
        />
      ))}

      <div className="flex flex-wrap gap-3">
        <Button
          type="button"
          variant="outline"
          onClick={() =>
            setLots((current) => [...current, emptyLot(firstObjectId)])
          }
        >
          {m.tender_lot_add()}
        </Button>
        <Button
          type="submit"
          data-testid="create-tender"
          disabled={create.isPending}
        >
          {m.tender_create_submit()}
        </Button>
      </div>
      {create.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(create.error)}
        </p>
      )}
    </form>
  )
}
