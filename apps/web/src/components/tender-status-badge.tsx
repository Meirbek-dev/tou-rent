import {
  BanIcon,
  CircleCheckIcon,
  CircleXIcon,
  ClipboardCheckIcon,
  FilePenIcon,
  GavelIcon,
  InboxIcon,
  ListChecksIcon,
  MegaphoneIcon,
  type LucideIcon,
} from "lucide-react"

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

type StatusView = {
  variant: "info" | "success" | "warning" | "neutral" | "destructive"
  icon: LucideIcon
}

/**
 * Цвет и значок статуса. Зеленый - «можно подать заявку», желтый - «идет
 * процедура», серый - завершенные состояния: цвет нигде не остается
 * единственным носителем смысла, подпись и значок дублируют его (SC 1.4.1).
 */
const VIEWS: Record<TenderStatus, StatusView> = {
  draft: { variant: "neutral", icon: FilePenIcon },
  announced: { variant: "info", icon: MegaphoneIcon },
  repeat_announced: { variant: "info", icon: MegaphoneIcon },
  accepting: { variant: "success", icon: InboxIcon },
  qualification: { variant: "warning", icon: ClipboardCheckIcon },
  trading: { variant: "warning", icon: GavelIcon },
  summed_up: { variant: "info", icon: ListChecksIcon },
  contracted: { variant: "neutral", icon: CircleCheckIcon },
  failed: { variant: "destructive", icon: CircleXIcon },
  cancelled: { variant: "neutral", icon: BanIcon },
}

export function tenderStatusLabel(status: TenderStatus): string {
  return LABELS[status]()
}

export function TenderStatusBadge({ status }: { status: TenderStatus }) {
  const { variant, icon: Icon } = VIEWS[status]

  return (
    <Badge variant={variant}>
      <Icon aria-hidden="true" />
      {tenderStatusLabel(status)}
    </Badge>
  )
}
