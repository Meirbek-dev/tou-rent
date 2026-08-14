import { useRef, useState } from "react"
import { createFileRoute, notFound, useNavigate } from "@tanstack/react-router"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { ConfirmAction } from "@/components/confirm-action"
import { DeadlineBlock } from "@/components/deadline-block"
import { PageHeader } from "@/components/page-header"
import { Panel } from "@/components/panel"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { Textarea } from "@/components/ui/textarea"
import { api, tenderQuery } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import { formatDateTime, formatTenge } from "@/lib/format"
import { myApplicationsQuery } from "@/lib/participant"
import { ID_NUMBER_LENGTH, applicantDetailsErrors } from "@/lib/validation"

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
  head: () => ({ meta: [{ title: `${m.apply_title()} - ToU Rent` }] }),
  component: ApplyPage,
})

function ApplyPage() {
  const { tenderId } = Route.useParams()
  const { data: tender } = useSuspenseQuery(tenderQuery(tenderId))

  if (tender === null) throw notFound()
  return <ApplyForm tender={tender} />
}

/**
 * Подача заявки одной формой, но тремя названными шагами.
 *
 * Мастера с экранами здесь нет намеренно: заявка подается один раз и целиком,
 * и прятать от участника то, что он подписывает, за кнопкой «далее» - плохая
 * сделка. Шаги пронумерованы потому, что так они названы Правилами: сведения
 * заявителя (Прил. 2), квалификация (Прил. 11), цена (Прил. 9).
 *
 * Справа - неподвижная сводка: назначение лота, стартовая ставка, гарантийный
 * взнос и срок приема. Взнос вынесен крупной цифрой: это единственная сумма,
 * которую участник обязан внести до подачи, и раньше она стояла подписью
 * в одну строку с ценой.
 */
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
  // Пустое обязательное поле - ошибка, но кричать о ней, пока участник до
  // него не дошел, незачем: до первой попытки отправки показываются только
  // ошибки заполненных полей
  const [submitAttempted, setSubmitAttempted] = useState(false)

  const selectedLot = tender.lots.find((lot) => lot.id === lotId)

  // Реквизиты сохраняются и печатаются в договоре как есть, поэтому в заявку
  // уходит то же значение, которое проверено, - без краевых пробелов
  const details = {
    name: name.trim(),
    id_number: idNumber.trim(),
    address: address.trim(),
    phone: phone.trim(),
    email: email.trim(),
  }
  const errors = applicantDetailsErrors(details)
  const shownError = (field: keyof typeof details) =>
    submitAttempted || details[field] !== "" ? errors[field] : undefined

  // Ключ идемпотентности живет столько же, сколько открытая форма (ТЗ § 7):
  // второе нажатие и повтор после обрыва идут с тем же ключом и получают
  // ответ первой попытки, а не второй отказ по UNIQUE (lot_id, participant_id).
  // Ключ на каждый вызов не годится - двойное нажатие дало бы два разных
  // ключа, то есть ровно ту дырку, ради которой заголовок и заводился
  const idempotencyKey = useRef(crypto.randomUUID())

  const submit = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST(
        "/api/v1/tenders/{id}/applications",
        {
          headers: { "Idempotency-Key": idempotencyKey.current },
          params: { path: { id: tender.id } },
          body: {
            lot_id: lotId,
            applicant_kind: kind,
            applicant_details: {
              ...details,
              email: details.email === "" ? null : details.email,
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

  // Подтверждение показывается только по заполненной форме: пока она неполна,
  // та же кнопка остается обычной отправкой и вскрывает ошибки полей
  const complete =
    Object.keys(errors).length === 0 && price !== "" && lotId !== ""

  return (
    <div className="flex flex-col gap-6">
      <PageHeader title={m.apply_title()} description={tender.title} />

      <div className="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,1fr)_20rem]">
        <form
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault()
            // Заявка попадает в журнал регистрации навсегда (Прил. 12), поэтому
            // реквизит проверяется до отправки, а не после отказа api
            setSubmitAttempted(true)
          }}
        >
          <Panel title={`1. ${m.apply_details_legend()}`} titleAs="h2">
            <div className="flex flex-col gap-4">
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
              </div>

              <div className="flex flex-wrap gap-3">
                <div className="flex min-w-56 flex-col gap-1.5">
                  <Label htmlFor="apply-kind">{m.applicant_kind_label()}</Label>
                  <NativeSelect
                    id="apply-kind"
                    value={kind}
                    onChange={(event) =>
                      setKind(event.target.value as ApplicantKind)
                    }
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
                    aria-invalid={shownError("name") !== undefined}
                    value={name}
                    onChange={(event) => setName(event.target.value)}
                  />
                  <FieldError
                    id="apply-name-error"
                    message={shownError("name")}
                  />
                </div>
                <div className="flex w-48 flex-col gap-1.5">
                  <Label htmlFor="apply-idnum">
                    {m.applicant_id_number_label()}
                  </Label>
                  <Input
                    id="apply-idnum"
                    required
                    inputMode="numeric"
                    maxLength={ID_NUMBER_LENGTH}
                    aria-invalid={shownError("id_number") !== undefined}
                    value={idNumber}
                    onChange={(event) => setIdNumber(event.target.value)}
                  />
                  <FieldError
                    id="apply-idnum-error"
                    message={shownError("id_number")}
                  />
                </div>
              </div>

              <div className="flex flex-wrap gap-3">
                <div className="flex min-w-64 flex-1 flex-col gap-1.5">
                  <Label htmlFor="apply-address">
                    {m.object_address_label()}
                  </Label>
                  <Input
                    id="apply-address"
                    required
                    aria-invalid={shownError("address") !== undefined}
                    value={address}
                    onChange={(event) => setAddress(event.target.value)}
                  />
                  <FieldError
                    id="apply-address-error"
                    message={shownError("address")}
                  />
                </div>
                <div className="flex w-52 flex-col gap-1.5">
                  <Label htmlFor="apply-phone">
                    {m.applicant_phone_label()}
                  </Label>
                  <Input
                    id="apply-phone"
                    required
                    type="tel"
                    autoComplete="tel"
                    aria-invalid={shownError("phone") !== undefined}
                    value={phone}
                    onChange={(event) => setPhone(event.target.value)}
                  />
                  <FieldError
                    id="apply-phone-error"
                    message={shownError("phone")}
                  />
                </div>
                <div className="flex min-w-56 flex-1 flex-col gap-1.5">
                  <Label htmlFor="apply-email">{m.auth_email()}</Label>
                  <Input
                    id="apply-email"
                    type="email"
                    aria-invalid={shownError("email") !== undefined}
                    value={email}
                    onChange={(event) => setEmail(event.target.value)}
                  />
                  <FieldError
                    id="apply-email-error"
                    message={shownError("email")}
                  />
                </div>
              </div>
            </div>
          </Panel>

          <Panel title={`2. ${m.apply_section_qualification()}`} titleAs="h2">
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
          </Panel>

          <Panel title={`3. ${m.apply_section_price()}`} titleAs="h2">
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="apply-price">{m.apply_price_label()}</Label>
              <Input
                id="apply-price"
                required
                type="number"
                min="0.01"
                step="0.01"
                className="max-w-56 tabular-nums"
                value={price}
                onChange={(event) => setPrice(event.target.value)}
              />
              <p className="text-sm text-muted-foreground">
                {m.apply_price_hint()}
              </p>
            </div>
          </Panel>

          {submit.isError && (
            <p role="alert" className="text-sm text-destructive">
              {problemMessage(submit.error)}
            </p>
          )}
          <div>
            {complete ? (
              <ConfirmAction
                variant="default"
                title={m.apply_confirm_title()}
                description={m.apply_confirm_description({
                  price: formatTenge(price),
                  fee: formatTenge(selectedLot?.guarantee_fee ?? "0"),
                  deadline:
                    formatDateTime(tender.submission_deadline) ??
                    m.tender_date_tbd(),
                })}
                confirmLabel={m.apply_submit()}
                disabled={submit.isPending}
                onConfirm={() => submit.mutate()}
                trigger={
                  <Button type="button" data-testid="apply-submit">
                    {m.apply_submit()}
                  </Button>
                }
              />
            ) : (
              <Button type="submit" data-testid="apply-submit">
                {m.apply_submit()}
              </Button>
            )}
          </div>
        </form>

        <aside className="lg:sticky lg:top-6 lg:self-start">
          <Panel title={m.apply_summary_title()} titleAs="h2">
            <div className="flex flex-col gap-4">
              <div className="flex flex-col gap-0.5">
                <span className="text-xs text-muted-foreground">
                  {m.lot_purpose()}
                </span>
                <span className="font-medium">
                  {selectedLot?.purpose ?? "-"}
                </span>
              </div>
              <div className="flex flex-col gap-0.5">
                <span className="text-xs text-muted-foreground">
                  {m.lot_base_rate()}
                </span>
                <span
                  className="font-medium tabular-nums"
                  suppressHydrationWarning
                >
                  {selectedLot === undefined
                    ? "-"
                    : formatTenge(selectedLot.base_rate_monthly)}
                </span>
              </div>
              {/* Гарантийный взнос - единственная сумма, которую участник
                  вносит до подачи: она набирается крупно, а не подписью */}
              <div className="flex flex-col gap-0.5 border-t pt-4">
                <span className="text-xs text-muted-foreground">
                  {m.lot_guarantee_fee()}
                </span>
                <span
                  className="text-2xl leading-none font-semibold tabular-nums"
                  data-testid="apply-guarantee-fee"
                  suppressHydrationWarning
                >
                  {selectedLot === undefined
                    ? "-"
                    : formatTenge(selectedLot.guarantee_fee)}
                </span>
              </div>
              <DeadlineBlock
                value={tender.submission_deadline}
                size="lg"
                className="border-t pt-4"
              />
            </div>
          </Panel>
        </aside>
      </div>
    </div>
  )
}

/** Ошибка поля рядом с самим полем: `role="alert"` озвучивает ее сразу. */
function FieldError({
  id,
  message,
}: {
  id: string
  message: string | undefined
}) {
  if (message === undefined) return null
  return (
    <p id={id} role="alert" className="text-sm text-destructive">
      {message}
    </p>
  )
}
