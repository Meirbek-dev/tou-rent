import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { Button } from "@/components/ui/button"
import { problemMessage } from "@/lib/auth"
import { formatDateTime } from "@/lib/format"
import {
  pendingPublicationsQuery,
  publicRecordsQuery,
  publishRecord,
} from "@/lib/public-records"

import type { PendingPublication } from "@/lib/public-records"

/**
 * Публикации особого порядка (FR-1403, п. 90, 92, 97): результат
 * рассмотрения заявки, обоснование ставки договора и акт приемки инвестиций
 * выкладываются на портал уполномоченным подразделением за пять рабочих дней
 * (п. 97). Публичный доступ длится шесть месяцев, дальше материал снимается
 * джобом и остается в досье решения (INV-076, FR-1206).
 */
export function PublicationsPanel() {
  const { data: pending } = useQuery(pendingPublicationsQuery)
  if (pending === undefined) return null

  return (
    <section aria-labelledby="publications" className="flex flex-col gap-3">
      <h2 id="publications" className="font-heading text-lg font-semibold">
        {m.publications_title()}
      </h2>
      <p className="text-sm text-muted-foreground">{m.publications_hint()}</p>
      {pending.length === 0 ? (
        <p className="text-muted-foreground">{m.publications_empty()}</p>
      ) : (
        <ul className="flex flex-col gap-3" data-testid="publications-pending">
          {pending.map((item) => (
            <li key={`${item.kind}-${item.source_id}`}>
              <PendingCard item={item} />
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}

function PendingCard({ item }: { item: PendingPublication }) {
  const queryClient = useQueryClient()

  const publish = useMutation({
    mutationFn: () => publishRecord(item.kind, item.source_id),
    onSuccess: async () => {
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
        <span className="rounded-md border px-2 py-0.5 text-sm">
          {item.kind_title_ru}
        </span>
        <span className="text-sm text-muted-foreground">{item.rule_ref}</span>
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
