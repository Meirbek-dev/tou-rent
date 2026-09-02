import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { CheckCheckIcon } from "lucide-react"

import { m } from "#/paraglide/messages"
import { QueryBoundary } from "@/components/query-boundary"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { problemMessage } from "@/lib/auth"
import { formatDateTime } from "@/lib/format"
import {
  pendingPublicationsQuery,
  publicRecordsQuery,
  publishRecord,
} from "@/lib/public-records"
import { serverLabel } from "@/lib/server-label"
import { notifySuccess } from "@/lib/toast"

import type { PendingPublication } from "@/lib/public-records"

/**
 * Публикации особого порядка (FR-1403, п. 90, 92, 97): результат
 * рассмотрения заявки, обоснование ставки договора и акт приемки инвестиций
 * выкладываются на портал уполномоченным подразделением за пять рабочих дней
 * (п. 97). Публичный доступ длится шесть месяцев, дальше материал снимается
 * джобом и остается в досье решения (INV-076, FR-1206).
 */
export function PublicationsPanel() {
  const pending = useQuery(pendingPublicationsQuery)

  return (
    <section aria-labelledby="publications" className="flex flex-col gap-3">
      <h2 id="publications" className="font-heading text-lg font-semibold">
        {m.publications_title()}
      </h2>
      <p className="text-sm text-muted-foreground">{m.publications_hint()}</p>
      <QueryBoundary
        query={pending}
        skeleton={
          <div className="flex flex-col gap-3" aria-hidden="true">
            <Skeleton className="h-28 w-full rounded-lg" />
            <Skeleton className="h-28 w-full rounded-lg" />
          </div>
        }
        empty={{
          when: (page) => page.items.length === 0,
          icon: CheckCheckIcon,
          title: m.publications_empty_title(),
          description: m.publications_empty(),
        }}
      >
        {(page) => (
          <>
            {/* Список ждущих публикации разгружается работой: поднятый признак
                означает не вторую страницу, а то, что работы больше, чем видно */}
            {page.truncated && (
              <p
                role="status"
                className="rounded-lg bg-amber-500/10 px-3 py-2 text-sm text-amber-700 ring-1 ring-amber-500/30 dark:text-amber-400"
                data-testid="publications-truncated"
              >
                {m.list_truncated({ count: page.items.length })}
              </p>
            )}
            <ul
              className="flex flex-col gap-3"
              data-testid="publications-pending"
            >
              {page.items.map((item) => (
                <li key={`${item.kind}-${item.source_id}`}>
                  <PendingCard item={item} />
                </li>
              ))}
            </ul>
          </>
        )}
      </QueryBoundary>
    </section>
  )
}

function PendingCard({ item }: { item: PendingPublication }) {
  const queryClient = useQueryClient()

  const publish = useMutation({
    mutationFn: () => publishRecord(item.kind, item.source_id),
    onSuccess: async () => {
      notifySuccess(m.publications_published())
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: pendingPublicationsQuery.queryKey,
        }),
        queryClient.invalidateQueries({
          queryKey: publicRecordsQuery.queryKey,
        }),
        // Публикация ложится в досье решения триггером БД (FR-1206)
        queryClient.invalidateQueries({ queryKey: ["dossier"] }),
      ])
    },
  })

  return (
    <article className="flex flex-col gap-2 rounded-lg border p-4">
      <div className="flex flex-wrap items-center gap-3">
        <Badge variant="neutral">{serverLabel(item, "kind_title")}</Badge>
        <span
          className="text-sm text-muted-foreground"
          suppressHydrationWarning
        >
          {formatDateTime(item.occurred_at)}
        </span>
      </div>
      <p className="font-medium">{item.title}</p>
      {publish.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(publish.error)}
        </p>
      )}
      <div className="flex flex-wrap items-center gap-3">
        <Button
          type="button"
          size="sm"
          data-testid="publish-record"
          disabled={!item.ready || publish.isPending}
          onClick={() => publish.mutate()}
        >
          {m.publications_publish()}
        </Button>
        {/* Публикуется сформированный документ либо замороженный расчет */}
        {!item.ready && (
          <span className="text-sm text-muted-foreground">
            {m.publications_not_ready()}
          </span>
        )}
      </div>
    </article>
  )
}
