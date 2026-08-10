import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { Button } from "@/components/ui/button"
import { problemMessage } from "@/lib/auth"
import { declineAmendment, tenderAmendmentsQuery } from "@/lib/amendments"
import { formatDateTime } from "@/lib/format"
import { myApplicationsQuery } from "@/lib/participant"

/**
 * Баннер изменений тендерной документации (FR-304, п. 27): что изменено,
 * до какого срока продлен прием заявок и печатная форма каждой редакции.
 * Участнику с действующей заявкой - право отказаться с возвратом взноса
 * (FR-1004, п. 26.5).
 */
export function AmendmentsBanner({
  tenderId,
  applicationId,
}: {
  tenderId: string
  applicationId?: string
}) {
  const queryClient = useQueryClient()
  const { data: amendments } = useQuery(tenderAmendmentsQuery(tenderId))

  const decline = useMutation({
    mutationFn: () => declineAmendment(applicationId as string),
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: myApplicationsQuery.queryKey }),
  })

  if (amendments === undefined || amendments.length === 0) return null

  return (
    <section
      aria-labelledby="amendments"
      className="flex flex-col gap-2 rounded-lg border border-amber-500/60 bg-amber-500/10 p-4"
      data-testid="amendments-banner"
    >
      <h3 id="amendments" className="font-heading text-lg font-semibold">
        {m.amendments_title()}
      </h3>
      <ul className="flex flex-col gap-2 text-sm">
        {amendments.map((amendment) => (
          <li key={amendment.id} className="flex flex-col gap-0.5">
            <span className="font-medium">
              {m.amendments_version({ version: amendment.version })}
              {" - "}
              <span suppressHydrationWarning>
                {formatDateTime(amendment.created_at)}
              </span>
            </span>
            <span>{amendment.summary}</span>
            <span className="text-muted-foreground" suppressHydrationWarning>
              {m.amendments_new_deadline()}:{" "}
              {formatDateTime(amendment.new_deadline)}
            </span>
            {amendment.has_doc && (
              <a
                href={`/api/v1/tender-amendments/${amendment.id}/announcement.pdf`}
                className="underline-offset-4 hover:underline"
              >
                {m.amendments_pdf()}
              </a>
            )}
          </li>
        ))}
      </ul>

      {applicationId !== undefined && (
        <div className="flex flex-col gap-1">
          <p className="text-sm text-muted-foreground">
            {m.amendments_decline_hint()}
          </p>
          <div>
            <Button
              variant="outline"
              size="sm"
              data-testid="decline-amendment"
              disabled={decline.isPending || decline.isSuccess}
              onClick={() => decline.mutate()}
            >
              {m.amendments_decline()}
            </Button>
          </div>
          {decline.isError && (
            <p role="alert" className="text-sm text-destructive">
              {problemMessage(decline.error)}
            </p>
          )}
        </div>
      )}
    </section>
  )
}
