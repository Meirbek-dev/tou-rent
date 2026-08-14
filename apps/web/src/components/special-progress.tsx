import { useId } from "react"

import { useQuery } from "@tanstack/react-query"
import { HourglassIcon } from "lucide-react"

import { m } from "#/paraglide/messages"
import { QueryBoundary } from "@/components/query-boundary"
import { buttonVariants } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { formatDateTime } from "@/lib/format"
import { decisionLabel, specialProgressQuery } from "@/lib/special"
import { cn } from "@/lib/utils"

// FR-1202 (п. 89–90): ход рассмотрения заявки особого порядка - заключение
// уполномоченного подразделения и решение Правления с обоснованием.
export function SpecialProgress({ requestId }: { requestId: string }) {
  const progress = useQuery(specialProgressQuery(requestId))
  // Раздел выводится по заявке в списке, а не по странице: постоянный id
  // связывал бы все карточки списка с одним и тем же заголовком
  const headingId = useId()

  return (
    <section aria-labelledby={headingId} className="flex flex-col gap-4">
      <h2 id={headingId} className="font-heading text-lg font-semibold">
        {m.special_progress_title()}
      </h2>
      <QueryBoundary
        query={progress}
        skeleton={
          <div className="flex flex-col gap-2" aria-hidden="true">
            <Skeleton className="h-24 w-full rounded-lg" />
          </div>
        }
        empty={{
          when: (data) => data.review == null && data.decision == null,
          icon: HourglassIcon,
          title: m.special_progress_empty_title(),
          description: m.special_progress_empty(),
        }}
      >
        {({ review, decision }) => (
          <>
            {review != null && (
              <article className="flex flex-col gap-2 rounded-lg border p-4">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <h3 className="font-medium">{m.special_review_heading()}</h3>
                  <span
                    className="text-sm text-muted-foreground"
                    suppressHydrationWarning
                  >
                    {formatDateTime(review.created_at)}
                  </span>
                </div>
                <p className="text-sm">{review.conclusion}</p>
                <p className="text-sm text-muted-foreground">
                  {m.special_recommendation_label()}:{" "}
                  {decisionLabel(review.recommendation)}
                  {review.reviewer_name != null && ` · ${review.reviewer_name}`}
                </p>
              </article>
            )}

            {decision != null && (
              <article className="flex flex-col gap-2 rounded-lg border p-4">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <h3 className="font-medium">
                    {m.special_decision_heading()}
                  </h3>
                  <span
                    className="text-sm text-muted-foreground"
                    suppressHydrationWarning
                  >
                    {formatDateTime(decision.decided_at)}
                  </span>
                </div>
                <p className="font-medium">
                  {decisionLabel(decision.decision)}
                </p>
                <p className="text-sm">{decision.rationale}</p>
                {decision.decided_by_name != null && (
                  <p className="text-sm text-muted-foreground">
                    {decision.decided_by_name}
                  </p>
                )}
                {decision.has_pdf && (
                  <a
                    href={`/api/v1/special-requests/${requestId}/decision.pdf`}
                    className={cn(
                      buttonVariants({ variant: "outline" }),
                      "self-start"
                    )}
                  >
                    {m.special_decision_pdf()}
                  </a>
                )}
              </article>
            )}
          </>
        )}
      </QueryBoundary>
    </section>
  )
}
