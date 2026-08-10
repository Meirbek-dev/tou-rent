import { Link } from "@tanstack/react-router"
import { m } from "#/paraglide/messages"
import { TenderStatusBadge } from "@/components/tender-status-badge"
import { formatDateTime } from "@/lib/format"

import type { TenderDto } from "@/lib/api"

/** Строка реестра/главной: заголовок-ссылка, статус, дедлайн, число лотов. */
export function TenderListItem({ tender }: { tender: TenderDto }) {
  const deadline = formatDateTime(tender.submission_deadline)
  return (
    <li className="rounded-lg border p-4 transition-colors hover:bg-muted/50">
      <article className="flex flex-col gap-2">
        <div className="flex flex-wrap items-center gap-3">
          <TenderStatusBadge status={tender.status} />
          <span className="text-sm text-muted-foreground">
            {m.tenders_lots_count({ count: tender.lots.length })}
          </span>
        </div>
        <h3 className="font-heading text-lg font-semibold">
          <Link
            to="/tenders/$tenderId"
            params={{ tenderId: tender.id }}
            className="underline-offset-4 hover:underline"
          >
            {tender.title}
          </Link>
        </h3>
        {/* Intl-вывод может отличаться между SSR и браузерами с урезанным ICU */}
        {deadline !== null && (
          <p className="text-sm text-muted-foreground" suppressHydrationWarning>
            {m.tender_deadline()}: {deadline}
          </p>
        )}
      </article>
    </li>
  )
}
