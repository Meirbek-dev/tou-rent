import {
  DoorOpenIcon,
  GavelIcon,
  KeyRoundIcon,
  type LucideIcon,
} from "lucide-react"

import { m } from "#/paraglide/messages"
import { Badge } from "@/components/ui/badge"

import type { ObjectStatus } from "@/lib/api"

// Исчерпывающие Record по ObjectStatus: новый статус в контракте сломает
// typecheck здесь, а не отрисуется молча без подписи.
const LABELS: Record<ObjectStatus, () => string> = {
  free: m.object_status_free,
  in_tender: m.object_status_in_tender,
  leased: m.object_status_leased,
}

type StatusView = {
  variant: "info" | "success" | "warning" | "neutral" | "destructive"
  icon: LucideIcon
}

const VIEWS: Record<ObjectStatus, StatusView> = {
  free: { variant: "success", icon: DoorOpenIcon },
  in_tender: { variant: "warning", icon: GavelIcon },
  leased: { variant: "neutral", icon: KeyRoundIcon },
}

/** Статус объекта вычисляется из тендеров и договоров (FR-103). */
export function objectStatusLabel(status: ObjectStatus): string {
  return LABELS[status]()
}

export function ObjectStatusBadge({ status }: { status: ObjectStatus }) {
  const { variant, icon: Icon } = VIEWS[status]

  return (
    <Badge variant={variant}>
      <Icon aria-hidden="true" />
      {objectStatusLabel(status)}
    </Badge>
  )
}
