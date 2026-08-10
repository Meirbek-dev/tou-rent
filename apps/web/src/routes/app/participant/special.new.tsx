import { useState } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import {
  useMutation,
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { Textarea } from "@/components/ui/textarea"
import { api, objectsPageQuery } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import {
  benefitSchemeLabel,
  localeLabel,
  mySpecialRequestsQuery,
  reviewTermLabel,
  specialCategoriesQuery,
} from "@/lib/special"

import type { ApplicantKind } from "@/lib/participant"

// FR-1201 (Прил. 3, п. 87–88): заявка особого порядка. Категория выбирается
// из закрытого перечня п. 87, и она же объявляет требования - перечень
// документов, срок проверки, льготную схему и публикуемость.
export const Route = createFileRoute("/app/participant/special/new")({
  loader: async ({ context }) => {
    await Promise.all([
      context.queryClient.ensureQueryData(specialCategoriesQuery),
      context.queryClient.ensureQueryData(objectsPageQuery({ status: "free" })),
    ])
  },
  component: NewSpecialRequestPage,
})

function NewSpecialRequestPage() {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { data: categories } = useSuspenseQuery(specialCategoriesQuery)
  // Страница объектов ограничена (курсорная пагинация): без поиска объект
  // за ее пределами заявителю недоступен - находка приемки контура 3 (T44)
  const [objectQuery, setObjectQuery] = useState("")
  const { data: objects } = useQuery({
    ...objectsPageQuery({
      status: "free",
      ...(objectQuery.trim() === "" ? {} : { q: objectQuery.trim() }),
    }),
    placeholderData: (previous) => previous,
  })

  const [categoryCode, setCategoryCode] = useState(categories[0]?.code ?? "")
  const [kind, setKind] = useState<ApplicantKind>("legal_entity")
  const [name, setName] = useState("")
  const [idNumber, setIdNumber] = useState("")
  const [address, setAddress] = useState("")
  const [phone, setPhone] = useState("")
  const [email, setEmail] = useState("")
  const [objectId, setObjectId] = useState("")
  const [purpose, setPurpose] = useState("")
  const [months, setMonths] = useState("")
  const [investment, setInvestment] = useState("")

  const category = categories.find((item) => item.code === categoryCode)

  const submit = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST("/api/v1/special-requests", {
        body: {
          category: categoryCode,
          applicant_kind: kind,
          applicant_details: {
            name,
            id_number: idNumber,
            address,
            phone,
            email: email === "" ? null : email,
          },
          object_id: objectId === "" ? null : objectId,
          purpose,
          requested_months: months === "" ? null : Number(months),
          investment_amount: investment === "" ? null : investment,
        },
      })
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("failed to submit special request")
      }
      return data
    },
    onSuccess: async (request) => {
      await queryClient.invalidateQueries({
        queryKey: mySpecialRequestsQuery.queryKey,
      })
      await navigate({
        to: "/app/participant/special/$requestId",
        params: { requestId: request.id },
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
          {m.special_new_title()}
        </h2>
        <p className="text-muted-foreground">{m.special_new_subtitle()}</p>
      </header>

      <div className="flex flex-col gap-1.5">
        <Label htmlFor="special-category">{m.special_category_label()}</Label>
        <NativeSelect
          id="special-category"
          value={categoryCode}
          onChange={(event) => setCategoryCode(event.target.value)}
        >
          {categories.map((item) => (
            <NativeSelectOption key={item.code} value={item.code}>
              {localeLabel(item)} - {item.rule_ref}
            </NativeSelectOption>
          ))}
        </NativeSelect>
        {category && (
          <div className="mt-2 flex flex-col gap-2 rounded-lg border p-4">
            <p className="text-sm text-muted-foreground">
              {m.special_review_label()}: {reviewTermLabel(category)} ·{" "}
              {m.special_benefit_label()}:{" "}
              {benefitSchemeLabel(category.benefit_scheme)} ·{" "}
              {category.publishable
                ? m.special_publishable_yes()
                : m.special_publishable_no()}
            </p>
            <div>
              <p className="text-sm font-medium">
                {m.special_documents_title()}
              </p>
              <ul className="mt-1 list-disc pl-5 text-sm text-muted-foreground">
                {category.documents.map((document) => (
                  <li key={document.code}>
                    {localeLabel(document)}
                    {document.required
                      ? ""
                      : ` (${m.special_document_optional()})`}
                  </li>
                ))}
              </ul>
            </div>
          </div>
        )}
      </div>

      <fieldset className="flex flex-col gap-4 rounded-lg border p-4">
        <legend className="px-1 font-medium">{m.apply_details_legend()}</legend>
        <div className="flex flex-wrap gap-3">
          <div className="flex min-w-56 flex-col gap-1.5">
            <Label htmlFor="special-kind">{m.applicant_kind_label()}</Label>
            <NativeSelect
              id="special-kind"
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
            <Label htmlFor="special-name">{m.applicant_name_label()}</Label>
            <Input
              id="special-name"
              required
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </div>
          <div className="flex w-48 flex-col gap-1.5">
            <Label htmlFor="special-idnum">
              {m.applicant_id_number_label()}
            </Label>
            <Input
              id="special-idnum"
              required
              value={idNumber}
              onChange={(event) => setIdNumber(event.target.value)}
            />
          </div>
        </div>
        <div className="flex flex-wrap gap-3">
          <div className="flex min-w-64 flex-1 flex-col gap-1.5">
            <Label htmlFor="special-address">{m.object_address_label()}</Label>
            <Input
              id="special-address"
              required
              value={address}
              onChange={(event) => setAddress(event.target.value)}
            />
          </div>
          <div className="flex w-52 flex-col gap-1.5">
            <Label htmlFor="special-phone">{m.applicant_phone_label()}</Label>
            <Input
              id="special-phone"
              required
              type="tel"
              value={phone}
              onChange={(event) => setPhone(event.target.value)}
            />
          </div>
          <div className="flex min-w-56 flex-1 flex-col gap-1.5">
            <Label htmlFor="special-email">{m.auth_email()}</Label>
            <Input
              id="special-email"
              type="email"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
            />
          </div>
        </div>
      </fieldset>

      <div className="flex flex-wrap gap-3">
        <div className="flex min-w-64 flex-1 flex-col gap-1.5">
          <Label htmlFor="special-object-search">
            {m.special_object_search()}
          </Label>
          <Input
            id="special-object-search"
            type="search"
            value={objectQuery}
            onChange={(event) => setObjectQuery(event.target.value)}
          />
        </div>
        <div className="flex min-w-64 flex-1 flex-col gap-1.5">
          <Label htmlFor="special-object">{m.special_object_label()}</Label>
          <NativeSelect
            id="special-object"
            value={objectId}
            onChange={(event) => setObjectId(event.target.value)}
          >
            <NativeSelectOption value="">
              {m.special_object_any()}
            </NativeSelectOption>
            {(objects?.items ?? []).map((object) => (
              <NativeSelectOption key={object.id} value={object.id}>
                {object.name} - {object.address}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </div>
        <div className="flex w-48 flex-col gap-1.5">
          <Label htmlFor="special-months">{m.special_months_label()}</Label>
          <Input
            id="special-months"
            type="number"
            min="1"
            max="240"
            value={months}
            onChange={(event) => setMonths(event.target.value)}
          />
        </div>
        {/* FR-1203 (п. 97): по инвестиционной категории заявки ранжируются
            суммой, поэтому без нее заявка не подается */}
        {category?.competition === "highest_amount" && (
          <div className="flex min-w-56 flex-col gap-1.5">
            <Label htmlFor="special-investment">
              {m.special_investment_label()}
            </Label>
            <Input
              id="special-investment"
              required
              type="number"
              min="0.01"
              step="0.01"
              value={investment}
              onChange={(event) => setInvestment(event.target.value)}
            />
            <p className="text-sm text-muted-foreground">
              {m.special_investment_hint()}
            </p>
          </div>
        )}
      </div>

      <div className="flex flex-col gap-1.5">
        <Label htmlFor="special-purpose">{m.special_purpose_label()}</Label>
        <Textarea
          id="special-purpose"
          required
          rows={4}
          value={purpose}
          onChange={(event) => setPurpose(event.target.value)}
        />
        <p className="text-sm text-muted-foreground">
          {m.special_purpose_hint()}
        </p>
      </div>

      {submit.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(submit.error)}
        </p>
      )}
      <div>
        <Button
          type="submit"
          data-testid="special-submit"
          disabled={submit.isPending}
        >
          {m.special_submit()}
        </Button>
      </div>
    </form>
  )
}
