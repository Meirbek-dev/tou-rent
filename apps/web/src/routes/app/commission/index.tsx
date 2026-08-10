import { Link, createFileRoute } from "@tanstack/react-router"
import { useSuspenseQuery } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { MyDeadlines } from "@/components/my-deadlines"
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

  return (
    <div className="flex flex-col gap-8">
      <MyDeadlines />

      <section aria-labelledby="composition" className="flex flex-col gap-3">
        <h2 id="composition" className="font-heading text-lg font-semibold">
          {m.commission_composition_title()}
        </h2>
        {commission === null ? (
          <p className="text-muted-foreground">{m.commission_none()}</p>
        ) : (
          <div className="flex flex-col gap-3 rounded-lg border p-4">
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
      </section>

      <section aria-labelledby="tenders" className="flex flex-col gap-4">
        <h2 id="tenders" className="font-heading text-lg font-semibold">
          {m.commission_tenders_title()}
        </h2>
        {tenders.length === 0 ? (
          <p className="text-muted-foreground">
            {m.commission_tenders_empty()}
          </p>
        ) : (
          <ul className="flex flex-col gap-3">
            {tenders.map((tender) => (
              <li
                key={tender.id}
                className="rounded-lg border p-4 transition-colors hover:bg-muted/50"
              >
                <div className="flex flex-wrap items-center gap-3">
                  <TenderStatusBadge status={tender.status} />
                  {tender.opening_at != null && (
                    <span
                      className="text-sm text-muted-foreground"
                      suppressHydrationWarning
                    >
                      {m.meeting_scheduled_at()}:{" "}
                      {formatDateTime(tender.opening_at)}
                    </span>
                  )}
                </div>
                <h3 className="mt-2 font-heading text-lg font-semibold">
                  <Link
                    to="/app/commission/tenders/$tenderId"
                    params={{ tenderId: tender.id }}
                    className="underline-offset-4 hover:underline"
                    data-testid={`commission-tender-${tender.id}`}
                  >
                    {tender.title}
                  </Link>
                </h3>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  )
}

/** Роль в комиссии - предметный термин п. 11, переводится ключами i18n. */
