import { useQuery } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { protocolKindLabel } from "@/components/protocols-panel"
import { formatDateTime } from "@/lib/format"
import { myProtocolsQuery } from "@/lib/publications"

/**
 * Копии протоколов в кабинете участника (FR-703, п. 56): по всем тендерам,
 * где участник подавал заявку, - независимо от публичного срока (п. 76).
 */
export function MyProtocols() {
  const { data: protocols } = useQuery(myProtocolsQuery)
  if (protocols === undefined || protocols.length === 0) return null

  return (
    <section
      aria-labelledby="my-protocols"
      className="flex flex-col gap-3"
      data-testid="my-protocols"
    >
      <h2 id="my-protocols" className="font-heading text-lg font-semibold">
        {m.my_protocols_title()}
      </h2>
      <ul className="flex flex-col gap-2 text-sm">
        {protocols.map((protocol) => (
          <li
            key={protocol.id}
            className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-lg border p-3"
          >
            <span className="font-medium">{protocol.tender_title}</span>
            <span>
              {protocolKindLabel(protocol.kind)}
              {protocol.number != null && ` №${protocol.number}`}
            </span>
            <span className="text-muted-foreground" suppressHydrationWarning>
              {formatDateTime(protocol.generated_at)}
            </span>
            {protocol.has_pdf && (
              <a
                href={`/api/v1/protocols/${protocol.id}/pdf`}
                className="underline-offset-4 hover:underline"
              >
                {m.protocols_pdf()}
              </a>
            )}
          </li>
        ))}
      </ul>
    </section>
  )
}
