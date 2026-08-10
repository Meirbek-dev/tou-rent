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

const VARIANTS: Record<
  ObjectStatus,
  "default" | "secondary" | "outline" | "destructive"
> = {
  free: "default",
  in_tender: "secondary",
  leased: "outline",
}

/** Статус объекта вычисляется из тендеров и договоров (FR-103). */
export function objectStatusLabel(status: ObjectStatus): string {
  return LABELS[status]()
}

export function ObjectStatusBadge({ status }: { status: ObjectStatus }) {
  return <Badge variant={VARIANTS[status]}>{objectStatusLabel(status)}</Badge>
}
