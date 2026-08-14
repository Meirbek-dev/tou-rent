import { Link, createFileRoute } from "@tanstack/react-router"
import { useSuspenseQuery } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { PageHeader } from "@/components/page-header"
import { QueueCard } from "@/components/queue-card"
import { StatCard } from "@/components/stat-card"
import { buttonVariants } from "@/components/ui/button"
import { useNowMs } from "@/hooks/use-now"
import { objectsQuery, organizerTendersQuery } from "@/lib/organizer"
import { deadlineUrgency } from "@/lib/relative-time"
import { cn } from "@/lib/utils"

import type { QueueItem } from "@/components/queue-card"
import type { DeadlineUrgency } from "@/lib/relative-time"

/** Сколько тендеров показывать в очереди «истекает срок». */
const CLOSING_SOON = 3

/** Статусы, при которых срок подачи еще имеет смысл (Правила п. 21). */
const OPEN_STATUSES = ["announced", "repeat_announced", "accepting"]

// Обзор кабинета организатора: сколько объектов и тендеров в каждом
// состоянии и у каких тендеров горит срок приема заявок.
//
// Числа считаются по уже загруженным страницам реестров - отдельных
// агрегирующих маршрутов у сервера нет, и выдумывать их ради плитки нельзя.
// Поэтому счет, упершийся в границу страницы, показывается с «+»: «100+»
// и «ровно 100» - разные утверждения (тот же прием, что на витрине портала).
export const Route = createFileRoute("/app/organizer/")({
  loader: async ({ context }) => {
    await Promise.all([
      context.queryClient.ensureQueryData(objectsQuery),
      context.queryClient.ensureQueryData(organizerTendersQuery),
    ])
  },
  head: () => ({ meta: [{ title: `${m.cabinet_organizer()} - ToU Rent` }] }),
  component: OrganizerHome,
})

function OrganizerHome() {
  const { data: objects } = useSuspenseQuery(objectsQuery)
  const { data: tenders } = useSuspenseQuery(organizerTendersQuery)
  const nowMs = useNowMs()

  const freeObjects = objects.items.filter(
    (object) => object.status === "free"
  ).length
  const objectsInTender = objects.items.filter(
    (object) => object.status === "in_tender"
  ).length
  const drafts = tenders.items.filter(
    (tender) => tender.status === "draft"
  ).length
  const accepting = tenders.items.filter(
    (tender) => tender.status === "accepting"
  ).length

  // Ближайший срок - первым; сортировка по строке ISO не зависит от «сейчас»
  // и потому одинакова на сервере и в браузере
  const closing = tenders.items
    .filter(
      (tender) =>
        tender.submission_deadline != null &&
        OPEN_STATUSES.includes(tender.status)
    )
    .toSorted((left, right) =>
      (left.submission_deadline ?? "").localeCompare(
        right.submission_deadline ?? ""
      )
    )

  const closingItems: QueueItem[] = closing.map((tender) => ({
    id: tender.id,
    label: tender.title,
    to: `/app/organizer/tenders/${tender.id}`,
    at: tender.submission_deadline,
  }))

  // Тон плитки приема заявок берется от самого горящего срока
  const urgency: DeadlineUrgency =
    nowMs === null
      ? "normal"
      : (closing
          .map((tender) =>
            deadlineUrgency(tender.submission_deadline ?? "", nowMs)
          )
          .find((value) => value !== "normal") ?? "normal")

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        title={m.cabinet_organizer()}
        description={m.org_dash_subtitle()}
        actions={
          <>
            <Link
              to="/app/organizer/objects"
              className={cn(buttonVariants({ variant: "outline" }))}
            >
              {m.org_nav_objects()}
            </Link>
            <Link
              to="/app/organizer/tenders/new"
              className={cn(buttonVariants())}
            >
              {m.tender_create_cta()}
            </Link>
          </>
        }
      />

      <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <StatCard
          label={m.org_dash_objects_free()}
          value={pageCount(objects.next_after, freeObjects)}
          to="/app/organizer/objects"
        />
        <StatCard
          label={m.org_dash_objects_in_tender()}
          value={pageCount(objects.next_after, objectsInTender)}
          to="/app/organizer/objects"
        />
        <StatCard
          label={m.org_dash_tenders_accepting()}
          value={pageCount(tenders.next_after, accepting)}
          urgency={urgency}
          to="/app/organizer/tenders"
        />
        <StatCard
          label={m.org_dash_tenders_draft()}
          value={pageCount(tenders.next_after, drafts)}
          to="/app/organizer/tenders"
        />
      </div>

      <QueueCard
        title={m.org_dash_closing_title()}
        count={closing.length}
        items={closingItems.slice(0, CLOSING_SOON)}
        empty={m.org_dash_closing_empty()}
        seeAll={{
          to: "/app/organizer/tenders",
          label: m.org_tenders_title(),
        }}
      />
    </div>
  )
}

/**
 * Счет по странице реестра: «+» означает, что записей больше, чем
 * прочитано. Молчать об этом нельзя - иначе плитка утверждает точное число,
 * которого не знает.
 */
function pageCount(next: string | null | undefined, shown: number): string {
  return next == null ? String(shown) : `${shown}+`
}
