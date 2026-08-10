import { useState } from "react"
import { Link } from "@tanstack/react-router"
import {
  useMutation,
  useQueries,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { Button, buttonVariants } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { tenderQuery } from "@/lib/api"
import {
  generateResultsProtocol,
  lotAuctionQuery,
  resultsProtocolQuery,
  scheduleAuction,
  setRecordingUrl,
} from "@/lib/auctions"
import { problemMessage } from "@/lib/auth"
import { formatTenge } from "@/lib/format"
import { cn } from "@/lib/utils"

import type { LotDto } from "@/lib/api"
import type { AuctionDto } from "@/lib/auctions"

/** Строка состояния комнаты: до старта - стартовая ставка, в торгах -
 * минимум следующей, после - сумма победителя (FR-606). */
function summary(auction: AuctionDto): string {
  if (auction.status === "finished") {
    return auction.winner_amount == null
      ? m.auction_no_winner()
      : `${m.auction_status_finished()} · ${formatTenge(auction.winner_amount)}`
  }
  if (auction.status === "running") {
    return m.auction_min_next({ amount: formatTenge(auction.min_next_bid) })
  }
  return `${m.auction_starting_bid()}: ${formatTenge(auction.starting_bid)}`
}

/**
 * Онлайн-торги по лотам тендера в кабинете секретаря (FR-601): открытие
 * комнаты фиксирует стартовую ставку (INV-062) и шаг 5 % (п. 63), дальше -
 * переход в комнату, где идет лента и таймер. Здесь же - протокол итогов
 * (FR-701), доступный после завершения торгов по всем лотам.
 */
export function AuctionLotsPanel({
  tenderId,
  lots,
}: {
  tenderId: string
  lots: LotDto[]
}) {
  const queryClient = useQueryClient()
  const auctions = useQueries({
    queries: lots.map((lot) => lotAuctionQuery(lot.id)),
  })
  const { data: protocol } = useQuery(resultsProtocolQuery(tenderId))

  const open = useMutation({
    mutationFn: (lotId: string) => scheduleAuction(lotId),
    onSuccess: async (_data, lotId) => {
      await queryClient.invalidateQueries({
        queryKey: lotAuctionQuery(lotId).queryKey,
      })
    },
  })

  const generateProtocol = useMutation({
    mutationFn: () => generateResultsProtocol(tenderId),
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: resultsProtocolQuery(tenderId).queryKey,
      })
    },
  })

  return (
    <section aria-labelledby="auctions">
      <h3 id="auctions" className="mb-3 font-heading text-lg font-semibold">
        {m.auction_panel_title()}
      </h3>
      <ul className="flex flex-col gap-2">
        {lots.map((lot, index) => {
          const auction = auctions[index]?.data ?? null
          return (
            <li
              key={lot.id}
              className="flex flex-wrap items-center justify-between gap-3 rounded-lg border px-4 py-3"
            >
              <div className="flex flex-col gap-0.5">
                <span className="font-medium">
                  {m.auction_lot({ seq: lot.seq, purpose: lot.purpose })}
                </span>
                <span className="text-sm text-muted-foreground">
                  {auction === null
                    ? m.auction_not_scheduled()
                    : summary(auction)}
                </span>
              </div>
              {auction === null ? (
                <Button
                  variant="outline"
                  data-testid="open-auction"
                  onClick={() => open.mutate(lot.id)}
                  disabled={open.isPending}
                >
                  {m.auction_open_room()}
                </Button>
              ) : (
                <Link
                  to="/app/auctions/$auctionId"
                  params={{ auctionId: auction.id }}
                  className="text-sm underline-offset-4 hover:underline"
                >
                  {m.auction_go_to_room()} →
                </Link>
              )}
            </li>
          )
        })}
      </ul>
      {open.isError && (
        <p role="alert" className="mt-2 text-sm text-destructive">
          {problemMessage(open.error)}
        </p>
      )}

      <div className="mt-4 flex flex-col gap-2">
        <div className="flex flex-wrap items-center gap-3">
          {protocol == null ? (
            <Button
              variant="outline"
              data-testid="generate-results-protocol"
              onClick={() => generateProtocol.mutate()}
              disabled={generateProtocol.isPending}
            >
              {m.results_protocol_generate()}
            </Button>
          ) : (
            <a
              href={`/api/v1/tenders/${tenderId}/results-protocol.pdf`}
              target="_blank"
              rel="noreferrer"
              data-testid="results-protocol-pdf"
              className={cn(buttonVariants({ variant: "outline" }))}
            >
              {m.results_protocol_pdf({ number: protocol.number ?? "" })}
            </a>
          )}
        </div>
        <p className="text-sm text-muted-foreground">
          {m.results_protocol_hint()}
        </p>
        {generateProtocol.isError && (
          <p role="alert" className="text-sm text-destructive">
            {problemMessage(generateProtocol.error)}
          </p>
        )}
      </div>

      <RecordingForm tenderId={tenderId} />
    </section>
  )
}

/**
 * Ссылка на запись торгов (FR-306, п. 72). Появляется, когда итоги подведены:
 * до этого записи не существует, и сервер такую правку отклоняет.
 */
function RecordingForm({ tenderId }: { tenderId: string }) {
  const queryClient = useQueryClient()
  const { data: tender } = useQuery(tenderQuery(tenderId))
  const [url, setUrl] = useState<string | null>(null)

  const save = useMutation({
    mutationFn: (value: string) => setRecordingUrl(tenderId, value),
    onSuccess: async () => {
      setUrl(null)
      await queryClient.invalidateQueries({
        queryKey: tenderQuery(tenderId).queryKey,
      })
    },
  })

  if (tender == null) return null
  const summedUp =
    tender.status === "summed_up" || tender.status === "contracted"
  if (!summedUp) return null

  const value = url ?? tender.zoom_recording_url ?? ""

  return (
    <form
      className="mt-4 flex flex-col gap-2 border-t pt-4"
      onSubmit={(event) => {
        event.preventDefault()
        save.mutate(value)
      }}
    >
      <Label htmlFor="recording-url">{m.auction_recording_label()}</Label>
      <div className="flex flex-wrap items-center gap-2">
        <Input
          id="recording-url"
          name="recording_url"
          type="url"
          inputMode="url"
          className="max-w-md"
          placeholder="https://…"
          data-testid="recording-url"
          value={value}
          onChange={(event) => setUrl(event.target.value)}
        />
        <Button
          type="submit"
          variant="outline"
          data-testid="save-recording"
          disabled={save.isPending}
        >
          {m.auction_recording_save()}
        </Button>
        {tender.zoom_recording_url != null && (
          <a
            href={tender.zoom_recording_url}
            target="_blank"
            rel="noreferrer"
            className="text-sm underline-offset-4 hover:underline"
          >
            {m.auction_recording_open()} →
          </a>
        )}
      </div>
      <p className="text-sm text-muted-foreground">
        {m.auction_recording_hint()}
      </p>
      {save.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(save.error)}
        </p>
      )}
    </form>
  )
}
