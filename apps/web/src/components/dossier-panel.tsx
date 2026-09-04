import { useQuery } from "@tanstack/react-query"
import { FolderOpenIcon } from "lucide-react"

import { m } from "#/paraglide/messages"
import { QueryBoundary } from "@/components/query-boundary"
import { buttonVariants } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { formatDate, formatDateTime } from "@/lib/format"
import { dossierQuery } from "@/lib/publications"
import { serverLabel } from "@/lib/server-label"
import { cn } from "@/lib/utils"

import type { DossierSubject } from "@/lib/publications"

/**
 * Досье (FR-1602, FR-1206, п. 16, 97): состав собирается автоматически из
 * событий процесса, выгрузка - архивом с манифестом. Предмета два - тендер
 * и решение особого порядка; механизм у них общий, различаются заголовок
 * и срок хранения материалов (INV-042).
 */
export function DossierPanel({
  subject,
  anonymizeApplicationTitles = false,
}: {
  subject: DossierSubject
  anonymizeApplicationTitles?: boolean
}) {
  const dossier = useQuery(dossierQuery(subject))

  const tender = subject.kind === "tender"
  const base = tender
    ? `/api/v1/tenders/${subject.id}`
    : `/api/v1/special-requests/${subject.id}`

  // Выгрузка предлагается только тогда, когда известно, что выгружать:
  // ссылка на архив пустого (или еще не загруженного) досье - обещание,
  // за которым приходит пустой zip
  const downloadable = dossier.data !== undefined && dossier.data.length > 0

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
        {downloadable && (
          <a
            href={`${base}/dossier.zip`}
            className={cn(buttonVariants({ variant: "outline", size: "sm" }))}
            data-testid="dossier-archive"
          >
            {m.dossier_download()}
          </a>
        )}
      </div>
      <p className="text-sm text-muted-foreground">
        {tender ? m.dossier_hint() : m.dossier_decision_hint()}
      </p>
      <QueryBoundary
        query={dossier}
        skeleton={
          <div className="flex flex-col gap-1.5" aria-hidden="true">
            <Skeleton className="h-5 w-full rounded-md" />
            <Skeleton className="h-5 w-11/12 rounded-md" />
            <Skeleton className="h-5 w-4/5 rounded-md" />
          </div>
        }
        empty={{
          when: (items) => items.length === 0,
          icon: FolderOpenIcon,
          title: m.dossier_empty_title(),
          description: m.dossier_empty(),
        }}
      >
        {(items) => {
          const applicationNumbers = new Map(
            items
              .filter((item) => item.kind === "application")
              .map((item, index) => [item.id, index + 1])
          )

          return (
            <ul
              className="flex flex-col gap-1 text-sm"
              data-testid="dossier-items"
            >
              {items.map((item) => {
                const applicationNumber = applicationNumbers.get(item.id)
                const title =
                  anonymizeApplicationTitles && applicationNumber !== undefined
                    ? m.participant_number({ number: applicationNumber })
                    : (item.title ?? "-")

                return (
                  <li
                    key={item.id}
                    className="flex flex-wrap items-center gap-x-3"
                  >
                    <span className="text-muted-foreground">
                      {serverLabel(item, "kind_title")}
                    </span>
                    <span>{title}</span>
                    <span
                      className="text-muted-foreground"
                      suppressHydrationWarning
                    >
                      {formatDateTime(item.occurred_at)}
                    </span>
                    {item.has_file && (
                      <span className="text-muted-foreground">
                        {m.dossier_file()}
                      </span>
                    )}
                    {/* INV-042: WORM-хранение - 5 лет тендерные материалы, 3 года решения */}
                    <span
                      className="text-muted-foreground"
                      suppressHydrationWarning
                    >
                      {m.dossier_retention({
                        date:
                          formatDate(item.retain_until) ?? item.retain_until,
                      })}
                    </span>
                  </li>
                )
              })}
            </ul>
          )
        }}
      </QueryBoundary>
    </section>
  )
}
