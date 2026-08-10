import { m } from "#/paraglide/messages"
import { Badge } from "@/components/ui/badge"

import type { ApplicationStatus } from "@/lib/participant"

// Исчерпывающие Record: новый статус в контракте ломает typecheck здесь.
const LABELS: Record<ApplicationStatus, () => string> = {
  submitted: m.app_status_submitted,
  withdrawn: m.app_status_withdrawn,
  fee_confirmed: m.app_status_fee_confirmed,
  admitted: m.app_status_admitted,
  rejected: m.app_status_rejected,
}

const VARIANTS: Record<
  ApplicationStatus,
  "default" | "secondary" | "outline" | "destructive"
> = {
  submitted: "default",
  withdrawn: "outline",
  fee_confirmed: "secondary",
  admitted: "secondary",
  rejected: "destructive",
}

export function applicationStatusLabel(status: ApplicationStatus): string {
  return LABELS[status]()
}

export function ApplicationStatusBadge({
  status,
}: {
  status: ApplicationStatus
}) {
  return (
    <Badge variant={VARIANTS[status]}>{applicationStatusLabel(status)}</Badge>
  )
}
