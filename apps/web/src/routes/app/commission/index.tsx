import { Link, createFileRoute } from "@tanstack/react-router"
import { useSuspenseQuery } from "@tanstack/react-query"
import { GavelIcon } from "lucide-react"
import { m } from "#/paraglide/messages"
import { EmptyState } from "@/components/empty-state"
import { PageHeader } from "@/components/page-header"
import { Panel } from "@/components/panel"
import { StatCard } from "@/components/stat-card"
import { TenderStatusBadge } from "@/components/tender-status-badge"
import { memberRoleLabel } from "@/lib/commission"
import { activeCommissionQuery } from "@/lib/commission"
import { formatDateTime } from "@/lib/format"
import { organizerTendersQuery } from "@/lib/organizer"

// Член комиссии: состав с кворумом и тендеры, по которым идет допуск.
export const Route = createFileRoute("/app/commission/")({
  loader: async ({ context }) => {
    await Promise.all([
      context.queryClient.ensureQueryData(organizerTendersQuery),
      context.queryClient.ensureQueryData(activeCommissionQuery),
    ])
  },
  head: () => ({ meta: [{ title: `${m.cabinet_commission()} - ToU Rent` }] }),
  component: CommissionHome,
})

/** Работа комиссии идет от приема заявок до итогов торгов. */
const COMMISSION_STATUSES = ["accepting", "qualification", "trading"]

function CommissionHome() {
  const { data: page } = useSuspenseQuery(organizerTendersQuery)
  const { data: commission } = useSuspenseQuery(activeCommissionQuery)
  const tenders = page.items.filter((tender) =>
    COMMISSION_STATUSES.includes(tender.status)
  )
  // Голосование идет на стадии допуска (FR-1103): именно эти тендеры ждут
  // голоса члена комиссии. Отдельного маршрута «мои неподанные голоса» у
  // сервера нет (см. @/lib/queues), и выдумывать число нельзя - считается
  // то, что есть: тендеры на допуске
  const awaitingVote = tenders.filter(
    (tender) => tender.status === "qualification"
  )

  return (
    <div className="flex flex-col gap-6">
      <PageHeader
        title={m.cabinet_commission()}
        description={m.commission_dash_subtitle()}
      />

      <div className="grid grid-cols-2 gap-3 lg:grid-cols-3">
        <StatCard
          label={m.commission_stat_awaiting_vote()}
          value={awaitingVote.length}
          urgency={awaitingVote.length > 0 ? "soon" : "normal"}
        />
        <StatCard label={m.commission_stat_in_work()} value={tenders.length} />
        <StatCard
          label={m.meeting_quorum()}
          value={commission?.quorum_required ?? "-"}
          hint={
            commission === null
              ? undefined
              : m.commission_voting_total({ count: commission.voting_total })
          }
        />
      </div>

      <Panel title={m.commission_tenders_title()} titleAs="h2">
        {tenders.length === 0 ? (
          <EmptyState
            icon={GavelIcon}
            title={m.commission_tenders_empty()}
            description={m.commission_tenders_empty_hint()}
          />
        ) : (
          <ul className="flex flex-col gap-2">
            {tenders.map((tender) => (
              <li
                key={tender.id}
                className="grid overflow-hidden rounded-lg border transition-colors hover:bg-muted/50"
              >
                <div className="flex flex-wrap items-center gap-x-4 gap-y-2 p-3">
                  <TenderStatusBadge status={tender.status} />
                  <h3 className="min-w-0 flex-1 font-medium">
                    <Link
                      to="/app/commission/tenders/$tenderId"
                      params={{ tenderId: tender.id }}
                      className="underline-offset-4 hover:underline"
                      data-testid={`commission-tender-${tender.id}`}
                    >
                      {tender.title}
                    </Link>
                  </h3>
                  <span className="flex shrink-0 flex-col gap-0.5">
                    <span className="text-xs text-muted-foreground">
                      {m.meeting_scheduled_at()}
                    </span>
                    <span
                      className="text-sm tabular-nums"
                      suppressHydrationWarning
                    >
                      {formatDateTime(tender.opening_at) ?? m.tender_date_tbd()}
                    </span>
                  </span>
                </div>
              </li>
            ))}
          </ul>
        )}
      </Panel>

      <Panel title={m.commission_composition_title()} titleAs="h2">
        {commission === null ? (
          <p className="text-sm text-muted-foreground">{m.commission_none()}</p>
        ) : (
          <div className="flex flex-col gap-3">
            <div className="flex flex-wrap items-center gap-3">
              <span className="font-medium">{commission.name}</span>
              <span className="text-sm text-muted-foreground">
                {m.commission_voting_total({ count: commission.voting_total })}
              </span>
              <span className="text-sm text-muted-foreground">
                {m.commission_quorum_required({
                  count: commission.quorum_required,
                })}
              </span>
              {!commission.approved && (
                <span className="text-sm text-destructive">
                  {m.commission_not_approved()}
                </span>
              )}
            </div>
            <ul className="grid grid-cols-1 gap-1 text-sm sm:grid-cols-2">
              {commission.members.map((member) => (
                <li key={member.member_id} className="flex gap-2">
                  <span>{member.full_name}</span>
                  <span className="text-muted-foreground">
                    {memberRoleLabel(member.member_role)}
                  </span>
                </li>
              ))}
            </ul>
          </div>
        )}
      </Panel>
    </div>
  )
}
