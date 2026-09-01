import { Link } from "@tanstack/react-router"

import { m } from "#/paraglide/messages"
import { DeadlineBlock } from "@/components/deadline-block"
import { TenderStatusBadge } from "@/components/tender-status-badge"

import type { TenderDto } from "@/lib/api"

/**
 * Строка реестра объявлений.
 *
 * Две колонки: содержимое и срок подачи справа. Кликабельна вся карточка
 * (растянутая ссылка `after:inset-0`), но в дереве доступности ссылка
 * ровно одна - заголовок.
 *
 * `headingLevel` - потому что уровень заголовка задает страница, а не
 * компонент: на /tenders заголовок страницы h1, и строка обязана быть h2,
 * иначе в оглавлении появляется дыра.
 */
export function TenderListItem({
  tender,
  headingLevel = 3,
}: {
  tender: TenderDto
  headingLevel?: 2 | 3
}) {
  const Heading = headingLevel === 2 ? "h2" : "h3"

  return (
    <li className="relative overflow-hidden rounded-xl border bg-card shadow-xs transition-[border-color,box-shadow] focus-within:ring-2 focus-within:ring-ring focus-within:ring-offset-1 focus-within:ring-offset-background hover:border-border hover:shadow-sm">
      <article className="grid grid-cols-[minmax(0,1fr)] sm:grid-cols-[minmax(0,1fr)_auto]">
        <div className="flex min-w-0 flex-col gap-2 p-4 sm:p-5">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
            <TenderStatusBadge
              status={tender.status}
              deadline={tender.submission_deadline}
            />
            <span className="text-sm text-muted-foreground">
              {m.tenders_lots_count({ count: tender.lots.length })}
            </span>
          </div>
          {/* Наименование тендера - свободный текст: в нем встречается
              длинный неразрывный кусок (номер, идентификатор, ссылка).
              Без переноса он уезжает за карточку, а `overflow-hidden`
              у нее срезает хвост - на узком экране заголовок обрывался */}
          <Heading className="text-lg leading-snug font-semibold break-words">
            <Link
              to="/tenders/$tenderId"
              params={{ tenderId: tender.id }}
              className="underline-offset-4 outline-none after:absolute after:inset-0 hover:underline"
            >
              {tender.title}
            </Link>
          </Heading>
        </div>

        <div className="border-t px-4 pt-3 pb-4 sm:col-start-2 sm:border-t-0 sm:border-l sm:px-5 sm:py-5 sm:text-right">
          <DeadlineBlock
            value={tender.submission_deadline}
            className="sm:min-w-[9rem] sm:items-end"
          />
        </div>
      </article>
    </li>
  )
}
