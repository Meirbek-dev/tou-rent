import {
  CircleCheckIcon,
  CircleXIcon,
  SendIcon,
  Undo2Icon,
  WalletIcon,
  type LucideIcon,
} from "lucide-react"

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

type StatusView = {
  variant: "info" | "success" | "warning" | "neutral" | "destructive"
  icon: LucideIcon
}

/** Тот же словарь тонов, что и у тендера: цвет значит одно и то же. */
const VIEWS: Record<ApplicationStatus, StatusView> = {
  submitted: { variant: "info", icon: SendIcon },
  withdrawn: { variant: "neutral", icon: Undo2Icon },
  fee_confirmed: { variant: "success", icon: WalletIcon },
  admitted: { variant: "success", icon: CircleCheckIcon },
  rejected: { variant: "destructive", icon: CircleXIcon },
}

export function applicationStatusLabel(status: ApplicationStatus): string {
  return LABELS[status]()
}

export function ApplicationStatusBadge({
  status,
}: {
  status: ApplicationStatus
}) {
  const { variant, icon: Icon } = VIEWS[status]

  return (
    <Badge variant={variant}>
      <Icon aria-hidden="true" />
      {applicationStatusLabel(status)}
    </Badge>
  )
}
