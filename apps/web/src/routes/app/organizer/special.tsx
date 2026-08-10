import { useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { DossierPanel } from "@/components/dossier-panel"
import { PublicationsPanel } from "@/components/publications-panel"
import { SpecialProgress } from "@/components/special-progress"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { Textarea } from "@/components/ui/textarea"
import { api } from "@/lib/api"
import { problemMessage } from "@/lib/auth"
import { formatDateTime } from "@/lib/format"
import { pendingSpecialRequestsQuery, specialStatusLabel } from "@/lib/special"

import type { SpecialRequest } from "@/lib/special"

// FR-1202 (п. 89): проверка заявки уполномоченным подразделением. Заключение
// выносит заявку на рассмотрение Правления и запускает срок решения (п. 90).
export const Route = createFileRoute("/app/organizer/special")({
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(pendingSpecialRequestsQuery),
  component: SpecialReviewPage,
})

function SpecialReviewPage() {
  const { data: requests } = useSuspenseQuery(pendingSpecialRequestsQuery)

  return (
    <div className="flex flex-col gap-6">
      <section aria-labelledby="special-review">
        <h2
          id="special-review"
          className="mb-3 font-heading text-lg font-semibold"
        >
          {m.special_review_queue_title()}
        </h2>
        {requests.length === 0 ? (
          <p className="text-muted-foreground">
            {m.special_review_queue_empty()}
          </p>
        ) : (
          <ul className="flex flex-col gap-4">
            {requests.map((request) => (
              <li key={request.id}>
                <RequestCard request={request} />
              </li>
            ))}
          </ul>
        )}
      </section>

      {/* FR-1403 (п. 97): результат публикуется за пять рабочих дней */}
      <PublicationsPanel />
    </div>
  )
}

function RequestCard({ request }: { request: SpecialRequest }) {
  const queryClient = useQueryClient()
  const [recommendation, setRecommendation] = useState("grant")
  const [conclusion, setConclusion] = useState("")

  const review = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST(
        "/api/v1/special-requests/{id}/review",
        {
          params: { path: { id: request.id } },
          body: { conclusion, recommendation },
        }
      )
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("review failed")
      }
      return data
    },
    onSuccess: async () => {
      setConclusion("")
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: pendingSpecialRequestsQuery.queryKey,
        }),
        queryClient.invalidateQueries({
          queryKey: ["special-progress", request.id],
        }),
        // Заключение попадает в досье триггером БД (FR-1206)
        queryClient.invalidateQueries({
          queryKey: ["dossier", "special-request", request.id],
        }),
      ])
    },
  })

  const awaitsReview = request.status === "submitted"

  return (
    <article className="flex flex-col gap-4 rounded-lg border p-4">
      <header className="flex flex-col gap-2">
        <div className="flex flex-wrap items-center gap-3">
          <span className="rounded-md border px-2 py-0.5 text-sm">
            {specialStatusLabel(request.status)}
          </span>
          <span className="text-sm text-muted-foreground">
            {request.category_label} ({request.category_rule_ref})
          </span>
          <span
            className="text-sm text-muted-foreground"
            suppressHydrationWarning
          >
            {m.application_submitted_at()}:{" "}
            {formatDateTime(request.submitted_at)}
          </span>
        </div>
        <p className="font-medium">
          {m.special_card_title({ id: request.id.slice(0, 8) })}
        </p>
        <p className="text-sm">{request.purpose}</p>
        {request.object_name != null && (
          <p className="text-sm text-muted-foreground">
            {m.special_object_label()}: {request.object_name}
          </p>
        )}
      </header>

      <SpecialProgress requestId={request.id} />

      {/* FR-1206 (п. 97): заявка и ее документы - то, по чему пишется заключение */}
      <DossierPanel subject={{ kind: "special-request", id: request.id }} />

      {awaitsReview && (
        <form
          className="flex flex-col gap-3 border-t pt-4"
          onSubmit={(event) => {
            event.preventDefault()
            review.mutate()
          }}
        >
          <div className="flex flex-col gap-1.5">
            <Label htmlFor={`conclusion-${request.id}`}>
              {m.special_conclusion_label()}
            </Label>
            <Textarea
              id={`conclusion-${request.id}`}
              required
              rows={3}
              value={conclusion}
              onChange={(event) => setConclusion(event.target.value)}
            />
          </div>
          <div className="flex max-w-sm min-w-64 flex-col gap-1.5">
            <Label htmlFor={`recommendation-${request.id}`}>
              {m.special_recommendation_label()}
            </Label>
            <NativeSelect
              id={`recommendation-${request.id}`}
              value={recommendation}
              onChange={(event) => setRecommendation(event.target.value)}
            >
              <NativeSelectOption value="grant">
                {m.special_decision_grant()}
              </NativeSelectOption>
              <NativeSelectOption value="refuse">
                {m.special_decision_refuse()}
              </NativeSelectOption>
              <NativeSelectOption value="redirect">
                {m.special_decision_redirect()}
              </NativeSelectOption>
            </NativeSelect>
          </div>
          {review.isError && (
            <p role="alert" className="text-sm text-destructive">
              {problemMessage(review.error)}
            </p>
          )}
          <div>
            <Button
              type="submit"
              data-testid="special-review-submit"
              disabled={review.isPending}
            >
              {m.special_review_submit()}
            </Button>
          </div>
        </form>
      )}
    </article>
  )
}
