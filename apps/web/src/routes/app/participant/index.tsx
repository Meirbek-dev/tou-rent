import { Link, createFileRoute } from "@tanstack/react-router"
import { useSuspenseQuery } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { PageHeader } from "@/components/page-header"
import { Panel } from "@/components/panel"
import { ApplicationStatusBadge } from "@/components/application-status-badge"
import { EmptyState } from "@/components/empty-state"
import { LandInvestorPanel } from "@/components/land-panels"
import { MyDeadlines } from "@/components/my-deadlines"
import { MyProtocols } from "@/components/my-protocols"
import { TenderStatusBadge } from "@/components/tender-status-badge"
import { buttonVariants } from "@/components/ui/button"
import { DeadlineBlock } from "@/components/deadline-block"
import { useNowMs } from "@/hooks/use-now"
import { localizedTenderTitle, tendersPageQuery } from "@/lib/api"
import { formatDateTime, formatTenge } from "@/lib/format"
import { intakeClosed } from "@/lib/intake"
import { applicationSubject, myApplicationsQuery } from "@/lib/participant"
import {
  mySpecialRequestsQuery,
  requestCategoryLabel,
  specialStatusLabel,
} from "@/lib/special"
import { cn } from "@/lib/utils"
import { ClipboardListIcon, InboxIcon } from "lucide-react"

import type { ApplicationStatus } from "@/lib/participant"

// Кабинет участника: мои заявки + тендеры с открытым приемом (FR-401).
export const Route = createFileRoute("/app/participant/")({
  loader: async ({ context }) => {
    await Promise.all([
      context.queryClient.ensureQueryData(myApplicationsQuery),
      context.queryClient.ensureQueryData(tendersPageQuery()),
      context.queryClient.ensureQueryData(mySpecialRequestsQuery),
    ])
  },
  head: () => ({ meta: [{ title: `${m.cabinet_participant()} - ToU Rent` }] }),
  component: ParticipantHome,
})

/**
 * Чего заявка ждет дальше. Подпись есть только у тех состояний, в которых
 * ход за кем-то другим: «отклонена» и «отозвана» договаривать нечего - об
 * этом уже сказал значок статуса.
 */
const NEXT_STEP: Partial<Record<ApplicationStatus, () => string>> = {
  submitted: m.app_next_submitted,
  fee_confirmed: m.app_next_fee_confirmed,
  admitted: m.app_next_admitted,
}

/**
 * Порядок заявок: сперва те, по которым процедура идет, потом закрытые.
 * История - внизу, потому что читают ее реже, чем следят за ходом дела.
 */
const STATUS_ORDER: Record<ApplicationStatus, number> = {
  admitted: 0,
  fee_confirmed: 1,
  submitted: 2,
  rejected: 3,
  withdrawn: 4,
}

function ParticipantHome() {
  const { data: applications } = useSuspenseQuery(myApplicationsQuery)
  const { data: tendersPage } = useSuspenseQuery(tendersPageQuery())
  const { data: specialRequests } = useSuspenseQuery(mySpecialRequestsQuery)
  // Прием закрывает срок, а статус переводит процедура: пока она не прошла,
  // тендер остается `accepting` с вышедшим сроком (FR-401, п. 45). Витрина
  // звала участника подавать заявку по таким тендерам и тут же отвечала ему
  // «Прием заявок закрыт» - дважды в одной строке, кнопкой и блоком срока
  const nowMs = useNowMs()
  const accepting = tendersPage.items.filter((t) => t.status === "accepting")
  const open = accepting.filter(
    (t) => !intakeClosed(t.status, t.submission_deadline, nowMs)
  )
  const awaitingOpening = accepting.filter((t) =>
    intakeClosed(t.status, t.submission_deadline, nowMs)
  )

  const ordered = applications.toSorted(
    (left, right) =>
      STATUS_ORDER[left.status] - STATUS_ORDER[right.status] ||
      right.submitted_at.localeCompare(left.submitted_at)
  )

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        title={m.cabinet_participant()}
        description={m.participant_dash_subtitle()}
        actions={
          <Link
            to="/app/participant/contracts"
            className={cn(buttonVariants({ variant: "outline" }))}
          >
            {m.my_contracts_title()}
          </Link>
        }
      />

      {/* Сроки Правил - до заявок: просроченный срок стоит дороже, чем
          порядок дел, и до сих пор участник видел его только значком в шапке
          или письмом-эскалацией - на дашборде блока не было вовсе */}
      <MyDeadlines />

      {/* Свои заявки: рабочая очередь участника */}
      <Panel title={m.my_applications_title()} titleAs="h2">
        {ordered.length === 0 ? (
          <EmptyState
            icon={ClipboardListIcon}
            title={m.my_applications_empty()}
            description={m.my_applications_empty_hint()}
          />
        ) : (
          <ul className="flex flex-col gap-2">
            {ordered.map((application) => (
              <li
                key={application.id}
                className="flex flex-wrap items-center gap-x-3 gap-y-1.5 rounded-lg border p-3"
              >
                <ApplicationStatusBadge status={application.status} />
                <Link
                  to="/app/participant/applications/$applicationId"
                  params={{ applicationId: application.id }}
                  className="font-medium underline-offset-4 hover:underline"
                >
                  {applicationSubject(application)}
                </Link>
                {NEXT_STEP[application.status] !== undefined && (
                  <span className="text-sm text-muted-foreground">
                    {NEXT_STEP[application.status]?.()}
                  </span>
                )}
                <span className="ml-auto flex flex-wrap items-baseline gap-x-3">
                  {application.price_amount != null && (
                    <span
                      className="text-sm tabular-nums"
                      suppressHydrationWarning
                    >
                      {formatTenge(application.price_amount)}
                    </span>
                  )}
                  <span
                    className="text-xs text-muted-foreground tabular-nums"
                    suppressHydrationWarning
                  >
                    {formatDateTime(application.submitted_at)}
                  </span>
                </span>
              </li>
            ))}
          </ul>
        )}
      </Panel>

      <MyProtocols />

      {/* FR-1801 (п. 104–105): заявка инвестора на земельный участок */}
      <LandInvestorPanel />

      {/* FR-1201 (п. 87–88): особый порядок - заявка вне тендера */}
      <Panel
        title={m.special_requests_title()}
        titleAs="h2"
        actions={
          <Link
            to="/app/participant/special/new"
            className={cn(buttonVariants({ variant: "outline", size: "sm" }))}
          >
            {m.special_new_cta()}
          </Link>
        }
      >
        {specialRequests.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {m.special_requests_empty()}
          </p>
        ) : (
          <ul className="flex flex-col gap-2">
            {specialRequests.map((request) => (
              <li
                key={request.id}
                className="flex flex-wrap items-center gap-x-3 gap-y-1.5 rounded-lg border p-3"
              >
                <span className="rounded-md border px-2 py-0.5 text-sm">
                  {specialStatusLabel(request.status)}
                </span>
                <Link
                  to="/app/participant/special/$requestId"
                  params={{ requestId: request.id }}
                  className="font-medium underline-offset-4 hover:underline"
                >
                  {m.special_card_title({ id: request.id.slice(0, 8) })}
                </Link>
                <span className="text-sm text-muted-foreground">
                  {requestCategoryLabel(request)}
                </span>
                <span
                  className="ml-auto text-xs text-muted-foreground tabular-nums"
                  suppressHydrationWarning
                >
                  {formatDateTime(request.submitted_at)}
                </span>
              </li>
            ))}
          </ul>
        )}
      </Panel>

      {/* Открытый прием - последним: это витрина, а не работа по своим делам */}
      <Panel title={m.open_tenders_title()} titleAs="h2">
        {open.length === 0 ? (
          <EmptyState
            icon={InboxIcon}
            title={m.open_tenders_empty()}
            description={m.open_tenders_empty_hint()}
            action={
              <Link
                to="/tenders"
                className={cn(buttonVariants({ variant: "outline" }))}
              >
                {m.nav_tenders()}
              </Link>
            }
          />
        ) : (
          <ul className="flex flex-col gap-3" data-testid="open-tenders">
            {open.map((tender) => (
              <li
                key={tender.id}
                className="flex flex-wrap items-center justify-between gap-3 rounded-lg border p-4"
              >
                <div className="flex min-w-0 flex-col gap-1.5">
                  <TenderStatusBadge
                    status={tender.status}
                    deadline={tender.submission_deadline}
                  />
                  <p className="font-heading text-lg font-semibold">
                    {localizedTenderTitle(tender)}
                  </p>
                </div>
                <DeadlineBlock value={tender.submission_deadline} />
                <Link
                  to="/app/participant/apply/$tenderId"
                  params={{ tenderId: tender.id }}
                  className={cn(buttonVariants())}
                >
                  {m.apply_cta()}
                </Link>
              </li>
            ))}
          </ul>
        )}
      </Panel>

      {/* Срок вышел, вскрытия еще не было: подавать нечего, но и прятать
          такой тендер незачем - участник видит, чего ждет. Кнопки здесь нет
          вовсе: «Прием заявок закрыт» уже сказано блоком срока */}
      {awaitingOpening.length > 0 && (
        <Panel title={m.participant_intake_over_title()} titleAs="h2">
          <ul className="flex flex-col gap-3" data-testid="intake-over-tenders">
            {awaitingOpening.map((tender) => (
              <li
                key={tender.id}
                className="flex flex-wrap items-center justify-between gap-3 rounded-lg border p-4"
              >
                <div className="flex min-w-0 flex-col gap-1.5">
                  <TenderStatusBadge
                    status={tender.status}
                    deadline={tender.submission_deadline}
                  />
                  <p className="font-heading text-lg font-semibold">
                    {localizedTenderTitle(tender)}
                  </p>
                </div>
                <DeadlineBlock value={tender.submission_deadline} />
              </li>
            ))}
          </ul>
        </Panel>
      )}
    </div>
  )
}
