import { useQuery } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { buttonVariants } from "@/components/ui/button"
import { formatDate, formatDateTime } from "@/lib/format"
import { dossierQuery } from "@/lib/publications"
import { cn } from "@/lib/utils"

import type { DossierSubject } from "@/lib/publications"

/**
 * Досье (FR-1602, FR-1206, п. 16, 97): состав собирается автоматически из
 * событий процесса, выгрузка - архивом с манифестом. Предмета два - тендер
 * и решение особого порядка; механизм у них общий, различаются заголовок
 * и срок хранения материалов (INV-042).
 */
export function DossierPanel({ subject }: { subject: DossierSubject }) {
  const { data: items } = useQuery(dossierQuery(subject))
  if (items === undefined || items.length === 0) return null

  const tender = subject.kind === "tender"
  const base = tender
    ? `/api/v1/tenders/${subject.id}`
    : `/api/v1/special-requests/${subject.id}`

  return (
    <section
      aria-labelledby={`dossier-${subject.id}`}
      className="flex flex-col gap-3"
      data-testid="dossier-panel"
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h3
          id={`dossier-${subject.id}`}
          className="font-heading text-lg font-semibold"
        >
          {tender ? m.dossier_title() : m.dossier_decision_title()}
        </h3>
        <a
          href={`${base}/dossier.zip`}
          className={cn(buttonVariants({ variant: "outline", size: "sm" }))}
          data-testid="dossier-archive"
        >
          {m.dossier_download()}
        </a>
      </div>
      <p className="text-sm text-muted-foreground">
        {tender ? m.dossier_hint() : m.dossier_decision_hint()}
      </p>
      <ul className="flex flex-col gap-1 text-sm" data-testid="dossier-items">
        {items.map((item) => (
          <li key={item.id} className="flex flex-wrap items-center gap-x-3">
            <span className="text-muted-foreground">{item.kind_title_ru}</span>
            <span>{item.title ?? "-"}</span>
            <span className="text-muted-foreground" suppressHydrationWarning>
              {formatDateTime(item.occurred_at)}
            </span>
            {item.has_file && (
              <span className="text-muted-foreground">{m.dossier_file()}</span>
            )}
            {/* INV-042: WORM-хранение - 5 лет тендерные материалы, 3 года решения */}
            <span className="text-muted-foreground" suppressHydrationWarning>
              {m.dossier_retention({
                date: formatDate(item.retain_until) ?? item.retain_until,
              })}
            </span>
          </li>
        ))}
      </ul>
    </section>
  )
}
