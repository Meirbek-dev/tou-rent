import { Link } from "@tanstack/react-router"

import { m } from "#/paraglide/messages"

import type { ObligationDto } from "@/lib/obligations"

/**
 * Предмет срока Правил ссылкой; `null` - сослаться не на что.
 *
 * Сроки бывают тендерные и договорные. У вторых `tender_id` пуст, и до
 * появления `contract_id` строка «Представить документы для сверки» висела
 * в списке без единой зацепки: о каком договоре речь - участник выяснял
 * перебором. Кабинет нанимателя - список карточек, поэтому договор
 * открывается якорем на своей карточке (`#contract-<id>`), а не отдельным
 * маршрутом, которого у договора нет.
 *
 * Тендер ведет на публичную карточку намеренно: срок адресован роли, и
 * ссылка в кабинет организатора привела бы участника в чужой кабинет.
 */
export function ObligationLink({
  obligation,
  className,
}: {
  obligation: Pick<ObligationDto, "tender_id" | "tender_title" | "contract_id">
  className?: string | undefined
}) {
  const style = className ?? "text-sm underline-offset-4 hover:underline"

  if (obligation.tender_id != null && obligation.tender_title != null) {
    return (
      <Link
        to="/tenders/$tenderId"
        params={{ tenderId: obligation.tender_id }}
        className={style}
      >
        {obligation.tender_title}
      </Link>
    )
  }

  if (obligation.contract_id != null) {
    return (
      <Link
        to="/app/participant/contracts"
        hash={`contract-${obligation.contract_id}`}
        className={style}
        data-testid="obligation-contract-link"
      >
        {m.deadline_contract_link()}
      </Link>
    )
  }

  return null
}
