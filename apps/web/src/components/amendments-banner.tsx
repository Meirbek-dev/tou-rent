import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { m } from "#/paraglide/messages"
import { ConfirmAction } from "@/components/confirm-action"
import { QueryBoundary } from "@/components/query-boundary"
import { Button } from "@/components/ui/button"
import { problemMessage } from "@/lib/auth"
import { declineAmendment, tenderAmendmentsQuery } from "@/lib/amendments"
import { formatDateTime } from "@/lib/format"
import { myApplicationsQuery } from "@/lib/participant"
import { notifySuccess } from "@/lib/toast"

/**
 * Баннер изменений тендерной документации (FR-304, п. 27): что изменено,
 * до какого срока продлен прием заявок и печатная форма каждой редакции.
 * Участнику с действующей заявкой - право отказаться с возвратом взноса
 * (FR-1004, п. 26.5).
 *
 * Баннер условный - редакций у большинства тендеров нет. На публичной
 * карточке тендера редакции приходят загрузчиком маршрута
 * (`routes/tenders/$tenderId.tsx`), поэтому там запрос к первой отрисовке
 * уже завершен и заглушка в серверную разметку не попадает; в кабинетах
 * баннер до ответа тоже не занимает места. Видимым остается только отказ
 * запроса: пропавший баннер иначе значил бы «условия не менялись».
 */
export function AmendmentsBanner({
  tenderId,
  applicationId,
}: {
  tenderId: string
  applicationId?: string | undefined
}) {
  const queryClient = useQueryClient()
  const amendments = useQuery(tenderAmendmentsQuery(tenderId))

  const decline = useMutation({
    mutationFn: (id: string) => declineAmendment(id),
    onSuccess: async () => {
      notifySuccess(m.amendments_declined())
      await queryClient.invalidateQueries({
        queryKey: myApplicationsQuery.queryKey,
      })
    },
  })

  return (
    <QueryBoundary query={amendments} skeleton={<></>}>
      {(rows) =>
        rows.length === 0 ? null : (
          <section
            aria-labelledby="amendments"
            className="flex flex-col gap-2 rounded-xl bg-amber-500/10 p-5 ring-1 ring-amber-500/30"
            data-testid="amendments-banner"
          >
            <h3 id="amendments" className="font-heading text-lg font-semibold">
              {m.amendments_title()}
            </h3>
            <ul className="flex flex-col gap-2 text-sm">
              {rows.map((amendment) => (
                <li key={amendment.id} className="flex flex-col gap-0.5">
                  <span className="font-medium">
                    {m.amendments_version({ version: amendment.version })}
                    {" - "}
                    <span className="tabular-nums" suppressHydrationWarning>
                      {formatDateTime(amendment.created_at)}
                    </span>
                  </span>
                  <span>{amendment.summary}</span>
                  <span>
                    {m.amendments_new_deadline()}:{" "}
                    <span
                      className="font-medium tabular-nums"
                      suppressHydrationWarning
                    >
                      {formatDateTime(amendment.new_deadline)}
                    </span>
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
                  {/* Отказ отзывает заявку и запускает возврат взноса
                      (п. 26.5) - назад это не отыгрывается */}
                  <ConfirmAction
                    title={m.amendments_decline_confirm_title()}
                    description={m.amendments_decline_confirm_text()}
                    confirmLabel={m.amendments_decline()}
                    disabled={decline.isPending || decline.isSuccess}
                    onConfirm={() => decline.mutate(applicationId)}
                    trigger={
                      <Button
                        variant="outline"
                        size="sm"
                        data-testid="decline-amendment"
                        disabled={decline.isPending || decline.isSuccess}
                      >
                        {m.amendments_decline()}
                      </Button>
                    }
                  />
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
    </QueryBoundary>
  )
}
