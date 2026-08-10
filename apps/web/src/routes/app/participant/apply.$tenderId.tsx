import { useState } from "react"
import { createFileRoute, notFound, useNavigate } from "@tanstack/react-router"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { Textarea } from "@/components/ui/textarea"
import { api, tenderQuery } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import { formatTenge } from "@/lib/format"
import { myApplicationsQuery } from "@/lib/participant"

import type { TenderDto } from "@/lib/api"
import type { ApplicantKind } from "@/lib/participant"

// FR-401: мастер подачи - сведения (Прил. 2), квалификация (Прил. 11),
// ценовое предложение (Прил. 9). Поля Прил. 2 - приближение контура 1 (A-020).
export const Route = createFileRoute("/app/participant/apply/$tenderId")({
  loader: async ({ context, params }) => {
    const tender = await context.queryClient.ensureQueryData(
      tenderQuery(params.tenderId)
    )
    if (tender === null) throw notFound()
  },
  component: ApplyPage,
})

function ApplyPage() {
  const { tenderId } = Route.useParams()
  const { data: tender } = useSuspenseQuery(tenderQuery(tenderId))

  if (tender === null) throw notFound()
  return <ApplyForm tender={tender} />
}

function ApplyForm({ tender }: { tender: TenderDto }) {
  const navigate = useNavigate()
  const queryClient = useQueryClient()

  const [lotId, setLotId] = useState(tender.lots[0]?.id ?? "")
  const [kind, setKind] = useState<ApplicantKind>("legal_entity")
  const [name, setName] = useState("")
  const [idNumber, setIdNumber] = useState("")
  const [address, setAddress] = useState("")
  const [phone, setPhone] = useState("")
  const [email, setEmail] = useState("")
  const [qualification, setQualification] = useState("")
  const [price, setPrice] = useState("")

  const selectedLot = tender.lots.find((lot) => lot.id === lotId)

  const submit = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST(
        "/api/v1/tenders/{id}/applications",
        {
          params: { path: { id: tender.id } },
          body: {
            lot_id: lotId,
            applicant_kind: kind,
            applicant_details: {
              name,
              id_number: idNumber,
              address,
              phone,
              email: email === "" ? null : email,
            },
            qualification: qualification === "" ? null : qualification,
            price_amount: price,
          },
        }
      )
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("failed to submit application")
      }
      return data
    },
    onSuccess: async (application) => {
      await queryClient.invalidateQueries({
        queryKey: myApplicationsQuery.queryKey,
      })
      await navigate({
        to: "/app/participant/applications/$applicationId",
        params: { applicationId: application.id },
      })
    },
  })

  return (
    <form
      className="flex max-w-3xl flex-col gap-6"
      onSubmit={(event) => {
        event.preventDefault()
        submit.mutate()
      }}
    >
      <header className="flex flex-col gap-1">
        <h2 className="font-heading text-lg font-semibold">
          {m.apply_title()}
        </h2>
        <p className="text-muted-foreground">{tender.title}</p>
      </header>

      <div className="flex flex-col gap-1.5">
        <Label htmlFor="apply-lot">{m.tender_lot_object()}</Label>
        <NativeSelect
          id="apply-lot"
          value={lotId}
          onChange={(event) => setLotId(event.target.value)}
        >
          {tender.lots.map((lot) => (
            <NativeSelectOption key={lot.id} value={lot.id}>
              №{lot.seq} - {lot.purpose}
            </NativeSelectOption>
          ))}
        </NativeSelect>
        {selectedLot && (
          <p className="text-sm text-muted-foreground" suppressHydrationWarning>
            {m.lot_base_rate()}: {formatTenge(selectedLot.base_rate_monthly)} ·{" "}
            {m.lot_guarantee_fee()}: {formatTenge(selectedLot.guarantee_fee)}
          </p>
        )}
      </div>

      <fieldset className="flex flex-col gap-4 rounded-lg border p-4">
        <legend className="px-1 font-medium">{m.apply_details_legend()}</legend>
        <div className="flex flex-wrap gap-3">
          <div className="flex min-w-56 flex-col gap-1.5">
            <Label htmlFor="apply-kind">{m.applicant_kind_label()}</Label>
            <NativeSelect
              id="apply-kind"
              value={kind}
              onChange={(event) => setKind(event.target.value as ApplicantKind)}
            >
              <NativeSelectOption value="legal_entity">
                {m.applicant_kind_legal()}
              </NativeSelectOption>
              <NativeSelectOption value="individual">
                {m.applicant_kind_individual()}
              </NativeSelectOption>
            </NativeSelect>
          </div>
          <div className="flex min-w-64 flex-1 flex-col gap-1.5">
            <Label htmlFor="apply-name">{m.applicant_name_label()}</Label>
            <Input
              id="apply-name"
              required
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </div>
          <div className="flex w-48 flex-col gap-1.5">
            <Label htmlFor="apply-idnum">{m.applicant_id_number_label()}</Label>
            <Input
              id="apply-idnum"
              required
              value={idNumber}
              onChange={(event) => setIdNumber(event.target.value)}
            />
          </div>
        </div>
        <div className="flex flex-wrap gap-3">
          <div className="flex min-w-64 flex-1 flex-col gap-1.5">
            <Label htmlFor="apply-address">{m.object_address_label()}</Label>
            <Input
              id="apply-address"
              required
              value={address}
              onChange={(event) => setAddress(event.target.value)}
            />
          </div>
          <div className="flex w-52 flex-col gap-1.5">
            <Label htmlFor="apply-phone">{m.applicant_phone_label()}</Label>
            <Input
              id="apply-phone"
              required
              type="tel"
              value={phone}
              onChange={(event) => setPhone(event.target.value)}
            />
          </div>
          <div className="flex min-w-56 flex-1 flex-col gap-1.5">
            <Label htmlFor="apply-email">{m.auth_email()}</Label>
            <Input
              id="apply-email"
              type="email"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
            />
          </div>
        </div>
      </fieldset>

      <div className="flex flex-col gap-1.5">
        <Label htmlFor="apply-qualification">
          {m.apply_qualification_label()}
        </Label>
        <Textarea
          id="apply-qualification"
          rows={4}
          value={qualification}
          onChange={(event) => setQualification(event.target.value)}
        />
      </div>

      <div className="flex flex-col gap-1.5">
        <Label htmlFor="apply-price">{m.apply_price_label()}</Label>
        <Input
          id="apply-price"
          required
          type="number"
          min="0.01"
          step="0.01"
          className="max-w-56"
          value={price}
          onChange={(event) => setPrice(event.target.value)}
        />
        <p className="text-sm text-muted-foreground">{m.apply_price_hint()}</p>
      </div>

      {submit.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(submit.error)}
        </p>
      )}
      <div>
        <Button type="submit" disabled={submit.isPending}>
          {m.apply_submit()}
        </Button>
      </div>
    </form>
  )
}
