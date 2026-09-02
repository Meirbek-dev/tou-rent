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
import { useNowMs } from "@/hooks/use-now"

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

/**
 * Срок вышел, а статус еще «прием заявок»: перевод статуса делает процедура,
 * и до нее карточка утверждала два противоположных факта разом - зеленое
 * «Прием заявок» рядом с «Прием заявок закрыт» из блока срока. Зеленого
 * приглашения тут больше нет: подпись нейтральная, цвет серый.
 */
const INTAKE_OVER: StatusView = { variant: "neutral", icon: InboxIcon }

function StatusBadge({ view, label }: { view: StatusView; label: string }) {
  const { variant, icon: Icon } = view

  return (
    <Badge variant={variant}>
      <Icon aria-hidden="true" />
      {label}
    </Badge>
  )
}

function DeadlineAwareBadge({
  status,
  deadline,
}: {
  status: TenderStatus
  deadline: string
}) {
  // «Сейчас» появляется после монтирования: серверное время в разметке
  // разошлось бы с браузерным при гидратации (NFR-03, см. useNowMs)
  const nowMs = useNowMs()
  const intakeOver =
    status === "accepting" &&
    nowMs !== null &&
    new Date(deadline).getTime() <= nowMs

  return intakeOver ? (
    <StatusBadge view={INTAKE_OVER} label={m.tender_status_intake_over()} />
  ) : (
    <StatusBadge view={VIEWS[status]} label={tenderStatusLabel(status)} />
  )
}

export function TenderStatusBadge({
  status,
  deadline,
}: {
  status: TenderStatus
  /** Срок приема заявок; без него бейдж читает только статус */
  deadline?: string | null | undefined
}) {
  // Ветка со сроком заводит ежеминутный таймер, поэтому она отдельным
  // компонентом: реестры кабинетов рисуют бейдж десятками строк, и тик,
  // который ничего не меняет, там не нужен ни одной
  return deadline == null ? (
    <StatusBadge view={VIEWS[status]} label={tenderStatusLabel(status)} />
  ) : (
    <DeadlineAwareBadge status={status} deadline={deadline} />
  )
}
