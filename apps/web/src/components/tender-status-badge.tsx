import { m } from "#/paraglide/messages"
import { Badge } from "@/components/ui/badge"

import type { TenderStatus } from "@/lib/api"

// Исчерпывающие Record по TenderStatus: новый статус в контракте
// сломает typecheck здесь, а не молча отрисуется без подписи.
const LABELS: Record<TenderStatus, () => string> = {
  draft: m.tender_status_draft,
  announced: m.tender_status_announced,
  accepting: m.tender_status_accepting,
  qualification: m.tender_status_qualification,
  trading: m.tender_status_trading,
  summed_up: m.tender_status_summed_up,
  contracted: m.tender_status_contracted,
  failed: m.tender_status_failed,
  repeat_announced: m.tender_status_repeat_announced,
  cancelled: m.tender_status_cancelled,
}

const VARIANTS: Record<
  TenderStatus,
  "default" | "secondary" | "outline" | "destructive"
> = {
  draft: "outline",
  announced: "default",
  accepting: "default",
  qualification: "secondary",
  trading: "default",
  summed_up: "secondary",
  contracted: "secondary",
  failed: "destructive",
  repeat_announced: "default",
  cancelled: "destructive",
}

export function tenderStatusLabel(status: TenderStatus): string {
  return LABELS[status]()
}

export function TenderStatusBadge({ status }: { status: TenderStatus }) {
  return <Badge variant={VARIANTS[status]}>{tenderStatusLabel(status)}</Badge>
}
