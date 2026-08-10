import { useEffect, useRef, useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { useAuctionRoom } from "@/hooks/use-auction-room"
import {
  auctionRoomQuery,
  extendAuction,
  finishAuction,
  markAbsent,
  passTurn,
  placeBid,
  startAuction,
} from "@/lib/auctions"
import { problemMessage } from "@/lib/auth"
import { formatDateTime, formatTenge } from "@/lib/format"

import type {
  AuctionDto,
  AuctionRoomDto,
  BidDto,
  CircleParticipantDto,
} from "@/lib/auctions"

// Комната торгов (FR-601–603, 606): лента ставок в реальном времени,
// server-authoritative таймер, панель председателя, итог с победителем
// и вторым местом.
export const Route = createFileRoute("/app/auctions/$auctionId")({
  loader: ({ context, params }) =>
    context.queryClient.ensureQueryData(auctionRoomQuery(params.auctionId)),
  component: AuctionRoomPage,
})

const STATUS_LABELS: Record<string, () => string> = {
  scheduled: m.auction_status_scheduled,
  running: m.auction_status_running,
  finished: m.auction_status_finished,
  cancelled: m.auction_status_cancelled,
}

function AuctionRoomPage() {
  const { auctionId } = Route.useParams()
  const { user } = Route.useRouteContext()
  const queryClient = useQueryClient()
  const { data: room } = useSuspenseQuery(auctionRoomQuery(auctionId))
  const connection = useAuctionRoom(auctionId)

  const auction = room.auction
  const isChair = user.roles.includes("secretary")
  // Очередность по кругу (FR-604): ставку принимают только у того, чей ход
  const myTurn =
    room.my_application_id != null &&
    (room.current_turn_application_id == null ||
      room.current_turn_application_id === room.my_application_id)
  const myState = room.participants.find(
    (participant) => participant.application_id === room.my_application_id
  )?.status
  const canBid =
    room.my_application_id != null &&
    auction.status === "running" &&
    myState !== "passed" &&
    myState !== "absent"

  const refresh = () =>
    queryClient.invalidateQueries({
      queryKey: auctionRoomQuery(auctionId).queryKey,
    })

  return (
    <main className="mx-auto flex w-full max-w-4xl flex-col gap-6 px-6 py-8">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-col gap-1">
          <h1 className="font-heading text-2xl font-semibold">
            {m.auction_room_title()}
          </h1>
          <p className="text-muted-foreground">
            {m.auction_lot({
              seq: auction.lot_seq,
              purpose: auction.lot_purpose,
            })}
          </p>
        </div>
        <span
          data-testid="auction-status"
          className="rounded-full border px-3 py-1 text-sm"
        >
          {STATUS_LABELS[auction.status]?.() ?? auction.status}
        </span>
      </header>

      {/*
        Обрыв связи виден участнику (R-17): без этого экран показывал
        последнее известное состояние - тот же максимум и тот же таймер, -
        и пропущенный ход выглядел как бездействие самого участника.
        aria-live: тот, кто читает экран программой, узнает об этом первым
      */}
      <p
        role="status"
        aria-live="polite"
        data-testid="auction-connection"
        data-state={connection}
        className={
          connection === "online"
            ? "sr-only"
            : "rounded-lg border border-destructive/50 bg-destructive/10 px-4 py-2 text-sm text-destructive"
        }
      >
        {connection === "online"
          ? m.auction_connection_online()
          : connection === "connecting"
            ? m.auction_connection_connecting()
            : m.auction_connection_offline()}
      </p>

      <section className="grid gap-4 rounded-lg border p-4 sm:grid-cols-4">
        <Figure label={m.auction_starting_bid()}>
          {formatTenge(auction.starting_bid)}
        </Figure>
        <Figure label={m.auction_step()}>
          {formatTenge(auction.bid_step)}
        </Figure>
        <Figure label={m.auction_current_max()} testId="current-max">
          {auction.current_max == null
            ? m.auction_no_bids()
            : formatTenge(auction.current_max)}
        </Figure>
        <Figure label={m.auction_time_left()} testId="time-left">
          <Countdown auction={auction} serverTime={room.server_time} />
        </Figure>
      </section>

      {auction.status === "finished" && (
        <Results auction={auction} bids={room.bids} />
      )}

      {room.participants.length > 0 && (
        <Circle
          room={room}
          isChair={isChair}
          auctionId={auctionId}
          onDone={refresh}
        />
      )}

      {canBid && (
        <BidForm
          auctionId={auctionId}
          auction={auction}
          myTurn={myTurn}
          onPlaced={refresh}
        />
      )}
      {room.my_application_id == null && auction.status === "running" && (
        <p className="text-sm text-muted-foreground">
          {m.auction_watcher_hint()}
        </p>
      )}

      {isChair && (
        <ChairPanel auctionId={auctionId} auction={auction} onDone={refresh} />
      )}

      {/*
        Лента объявляется программе чтения с экрана: торги идут в реальном
        времени, и «увидеть новую ставку» - единственный способ понять,
        что происходит. polite, а не assertive: ставки идут часто
      */}
      <section className="flex flex-col gap-2" aria-live="polite">
        <h2 className="font-heading text-lg font-medium">
          {m.auction_feed_title()}
        </h2>
        {room.bids.length === 0 ? (
          <p className="text-sm text-muted-foreground">{m.auction_no_bids()}</p>
        ) : (
          <ol data-testid="bid-feed" className="flex flex-col-reverse gap-1">
            {room.bids.map((bid) => (
              <li
                key={bid.id}
                className="flex flex-wrap items-baseline justify-between gap-2 rounded-md border px-3 py-2 text-sm"
              >
                <span className="font-medium">{bid.applicant_name}</span>
                <span data-testid="bid-amount">{formatTenge(bid.amount)}</span>
                <span
                  className="text-xs text-muted-foreground"
                  suppressHydrationWarning
                >
                  {formatDateTime(bid.placed_at)}
                </span>
              </li>
            ))}
          </ol>
        )}
      </section>
    </main>
  )
}

function Figure({
  label,
  testId,
  children,
}: {
  label: string
  testId?: string
  children: React.ReactNode
}) {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-xs text-muted-foreground">{label}</span>
      <span data-testid={testId} className="text-lg font-semibold">
        {children}
      </span>
    </div>
  )
}

/** Обратный отсчет по часам сервера (FR-602): клиент только отображает. */
function Countdown({
  auction,
  serverTime,
}: {
  auction: AuctionDto
  serverTime: string
}) {
  // Расхождение часов браузера и сервера снимается один раз по снимку
  const [skew] = useState(() => Date.now() - Date.parse(serverTime))
  const [now, setNow] = useState(() => Date.now())

  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1_000)
    return () => clearInterval(timer)
  }, [])

  const endsAt = auction.ends_at
  if (endsAt == null) return <>{m.auction_not_started()}</>
  // У завершенных торгов отсчет останавливается: время окончания уже наступило
  if (auction.status !== "running") return <>-</>

  const left = Math.max(0, Date.parse(endsAt) - (now - skew))
  const minutes = Math.floor(left / 60_000)
  const seconds = Math.floor((left % 60_000) / 1_000)
  return (
    <span suppressHydrationWarning>
      {minutes}:{String(seconds).padStart(2, "0")}
    </span>
  )
}

/** Итог торгов (FR-606, п. 74). */
function Results({ auction, bids }: { auction: AuctionDto; bids: BidDto[] }) {
  // Имя заявителя берется из ленты: победитель и второе место всегда в ней
  const nameOf = (applicationId: string | null | undefined) =>
    bids.find((bid) => bid.application_id === applicationId)?.applicant_name ??
    "-"

  return (
    <section
      data-testid="auction-results"
      className="flex flex-col gap-2 rounded-lg border bg-muted/40 p-4"
    >
      <h2 className="font-heading text-lg font-medium">
        {m.auction_results_title()}
      </h2>
      {auction.winner_amount == null ? (
        <p className="text-sm">{m.auction_no_winner()}</p>
      ) : (
        <p className="text-sm" data-testid="auction-winner">
          {m.auction_winner({
            name: nameOf(auction.winner_application_id),
            amount: formatTenge(auction.winner_amount),
          })}
        </p>
      )}
      {auction.runner_up_amount != null && (
        <p className="text-sm" data-testid="auction-runner-up">
          {m.auction_runner_up({
            name: nameOf(auction.runner_up_application_id),
            amount: formatTenge(auction.runner_up_amount),
          })}
        </p>
      )}
      {auction.finished_early && (
        <p className="text-xs text-muted-foreground">
          {m.auction_finished_early()}
        </p>
      )}
    </section>
  )
}

/**
 * Круг торгов (FR-604–605): очередность, выбывшие и неявившиеся. Секретарь
 * отмечает неявку - первоначальное предложение такого участника оглашается
 * в ленте (п. 70).
 */
function Circle({
  room,
  isChair,
  auctionId,
  onDone,
}: {
  room: AuctionRoomDto
  isChair: boolean
  auctionId: string
  onDone: () => Promise<void>
}) {
  const absent = useMutation({
    mutationFn: (applicationId: string) => markAbsent(auctionId, applicationId),
    onSuccess: onDone,
  })

  return (
    <section className="flex flex-col gap-2">
      <h2 className="font-heading text-lg font-medium">
        {m.auction_circle_title()}
      </h2>
      <ol className="flex flex-col gap-1" data-testid="auction-circle">
        {room.participants.map((participant) => (
          <li
            key={participant.application_id}
            className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-md border px-3 py-2 text-sm"
          >
            <span className="text-muted-foreground">
              {participant.turn_order}
            </span>
            <span className="font-medium">{participant.applicant_name}</span>
            <span className="text-muted-foreground">
              {stateLabel(participant)}
            </span>
            {room.current_turn_application_id ===
              participant.application_id && (
              <span data-testid="current-turn" className="font-medium">
                {m.auction_turn_now()}
              </span>
            )}
            {isChair &&
              participant.status === "active" &&
              room.auction.status !== "finished" && (
                <Button
                  variant="outline"
                  size="sm"
                  className="ml-auto"
                  data-testid="mark-absent"
                  disabled={absent.isPending}
                  onClick={() => absent.mutate(participant.application_id)}
                >
                  {m.auction_mark_absent()}
                </Button>
              )}
          </li>
        ))}
      </ol>
      {absent.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(absent.error)}
        </p>
      )}
    </section>
  )
}

function stateLabel(participant: CircleParticipantDto): string {
  switch (participant.status) {
    case "passed":
      return m.auction_state_passed()
    case "absent":
      return m.auction_state_absent({
        amount: formatTenge(participant.initial_price),
      })
    default:
      return m.auction_state_active()
  }
}

/** Ставка допущенного участника: минимум подсказывает сервер (INV-063). */
function BidForm({
  auctionId,
  auction,
  myTurn,
  onPlaced,
}: {
  auctionId: string
  auction: AuctionDto
  myTurn: boolean
  onPlaced: () => Promise<void>
}) {
  const [amount, setAmount] = useState("")
  // Идентификатор ставки переживает ретрай - повтор не создаст дубля (NFR-05)
  const bidId = useRef(crypto.randomUUID())

  const place = useMutation({
    mutationFn: () =>
      placeBid(
        auctionId,
        amount === "" ? auction.min_next_bid : amount,
        bidId.current
      ),
    onSuccess: async () => {
      bidId.current = crypto.randomUUID()
      setAmount("")
      await onPlaced()
    },
  })

  const pass = useMutation({
    mutationFn: () => passTurn(auctionId),
    onSuccess: onPlaced,
  })

  return (
    <section className="flex flex-col gap-2 rounded-lg border p-4">
      <Label htmlFor="bid-amount">
        {m.auction_min_next({ amount: formatTenge(auction.min_next_bid) })}
      </Label>
      {/* Смена хода - assertive: пропустить свой ход дороже, чем услышать
          объявление не вовремя (FR-604) */}
      <p
        className="text-sm text-muted-foreground"
        data-testid="turn-hint"
        role="status"
        aria-live="assertive"
      >
        {myTurn ? m.auction_your_turn() : m.auction_waiting_turn()}
      </p>
      <div className="flex flex-wrap items-center gap-2">
        <Input
          id="bid-amount"
          name="amount"
          inputMode="decimal"
          className="max-w-48"
          placeholder={auction.min_next_bid}
          value={amount}
          onChange={(event) => setAmount(event.target.value)}
        />
        <Button
          data-testid="place-bid"
          onClick={() => place.mutate()}
          disabled={place.isPending || !myTurn}
        >
          {m.auction_place_bid()}
        </Button>
        <Button
          variant="outline"
          data-testid="pass-turn"
          onClick={() => pass.mutate()}
          disabled={pass.isPending || !myTurn}
        >
          {m.auction_pass()}
        </Button>
      </div>
      {pass.error !== null && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(pass.error)}
        </p>
      )}
      {place.error !== null && (
        <p data-testid="bid-error" className="text-sm text-destructive">
          {problemMessage(place.error)}
        </p>
      )}
    </section>
  )
}

/** Панель председателя/секретаря (FR-602): старт, продление, завершение. */
function ChairPanel({
  auctionId,
  auction,
  onDone,
}: {
  auctionId: string
  auction: AuctionDto
  onDone: () => Promise<void>
}) {
  const [early, setEarly] = useState(false)

  const start = useMutation({
    mutationFn: () => startAuction(auctionId),
    onSuccess: onDone,
  })
  const extend = useMutation({
    mutationFn: () => extendAuction(auctionId),
    onSuccess: onDone,
  })
  const finish = useMutation({
    mutationFn: () => finishAuction(auctionId, early),
    onSuccess: onDone,
  })

  const error = start.error ?? extend.error ?? finish.error

  return (
    <section className="flex flex-col gap-3 rounded-lg border border-dashed p-4">
      <h2 className="font-heading text-lg font-medium">
        {m.auction_chair_panel()}
      </h2>
      <div className="flex flex-wrap items-center gap-2">
        <Button
          data-testid="start-auction"
          onClick={() => start.mutate()}
          disabled={auction.status !== "scheduled" || start.isPending}
        >
          {m.auction_start()}
        </Button>
        <Button
          variant="outline"
          data-testid="extend-auction"
          onClick={() => extend.mutate()}
          disabled={
            auction.status !== "running" ||
            auction.extended_once ||
            extend.isPending
          }
        >
          {m.auction_extend()}
        </Button>
        <Button
          variant="outline"
          data-testid="finish-auction"
          onClick={() => finish.mutate()}
          disabled={auction.status !== "running" || finish.isPending}
        >
          {m.auction_finish()}
        </Button>
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={early}
            onChange={(event) => setEarly(event.target.checked)}
          />
          {m.auction_finish_early()}
        </label>
      </div>
      {auction.extended_once && (
        <p className="text-xs text-muted-foreground">
          {m.auction_extended_used()}
        </p>
      )}
      {error != null && (
        <p data-testid="chair-error" className="text-sm text-destructive">
          {problemMessage(error)}
        </p>
      )}
    </section>
  )
}
