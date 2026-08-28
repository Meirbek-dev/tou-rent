import { useEffect, useRef, useState } from "react"
import { Link, createFileRoute } from "@tanstack/react-router"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { ConfirmAction } from "@/components/confirm-action"
import { PageHeader } from "@/components/page-header"
import { PageShell } from "@/components/page-shell"
import { Badge } from "@/components/ui/badge"
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
import { notifySuccess } from "@/lib/toast"
import { cn } from "@/lib/utils"
import { ArrowLeftIcon } from "lucide-react"

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
  head: () => ({ meta: [{ title: `${m.auction_room_title()} - ToU Rent` }] }),
  component: AuctionRoomPage,
})

/**
 * Состояния комнаты (`core.auction_status`). Союза в контракте нет - `status`
 * приходит строкой, - поэтому перечень объявлен здесь: он же дает компилятору
 * повод сломаться, когда состояние добавят, а подписи или тон забудут.
 */
const AUCTION_STATUSES = [
  "scheduled",
  "running",
  "finished",
  "cancelled",
] as const

type AuctionStatus = (typeof AUCTION_STATUSES)[number]

const STATUS_LABELS: Record<AuctionStatus, () => string> = {
  scheduled: m.auction_status_scheduled,
  running: m.auction_status_running,
  finished: m.auction_status_finished,
  cancelled: m.auction_status_cancelled,
}

const STATUS_TONES: Record<
  AuctionStatus,
  "success" | "neutral" | "info" | "destructive"
> = {
  scheduled: "neutral",
  running: "success",
  finished: "info",
  cancelled: "destructive",
}

function isKnownStatus(status: string): status is AuctionStatus {
  return (AUCTION_STATUSES as readonly string[]).includes(status)
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

  // Тендера в снимке комнаты нет (`AuctionDto` несет только лот), поэтому
  // назад ведем в свою заявку - она у торгующегося всегда есть, - а зрителя
  // в кабинет. Адрес объявлен `string`: у `Link` строковый адрес допустим,
  // а шаблонный литерал в союз маршрутов не попадает
  const backTo: string =
    room.my_application_id == null
      ? "/app"
      : `/app/participant/applications/${room.my_application_id}`

  return (
    <PageShell>
      <PageHeader
        // Комната открывается ссылкой из заявки или из кабинета и не имеет
        // своей крошки в шапке каркаса: без обратной ссылки выйти отсюда
        // можно было только кнопкой браузера
        breadcrumb={
          <Link
            to={backTo}
            className="inline-flex w-fit items-center gap-1.5 text-sm text-muted-foreground underline-offset-4 hover:underline"
          >
            <ArrowLeftIcon aria-hidden="true" className="size-4" />
            {room.my_application_id == null
              ? m.back_to_cabinet()
              : m.auction_back_to_application()}
          </Link>
        }
        title={m.auction_room_title()}
        description={m.auction_lot({
          seq: auction.lot_seq,
          purpose: auction.lot_purpose,
        })}
        badge={
          <Badge
            data-testid="auction-status"
            variant={
              isKnownStatus(auction.status)
                ? STATUS_TONES[auction.status]
                : "neutral"
            }
          >
            {isKnownStatus(auction.status)
              ? STATUS_LABELS[auction.status]()
              : auction.status}
          </Badge>
        }
      />

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
            : "rounded-lg border border-destructive/25 bg-destructive/10 px-4 py-2 text-sm text-destructive"
        }
      >
        {connection === "online"
          ? m.auction_connection_online()
          : connection === "connecting"
            ? m.auction_connection_connecting()
            : m.auction_connection_offline()}
      </p>

      <Countdown auction={auction} serverTime={room.server_time} />

      <section className="grid gap-4 rounded-lg border p-4 sm:grid-cols-3">
        <Figure label={m.auction_starting_bid()}>
          {formatTenge(auction.starting_bid)}
        </Figure>
        <Figure label={m.auction_step()}>
          {auction.bid_step_percent}% · {formatTenge(auction.bid_step)}
        </Figure>
        <Figure label={m.auction_current_max()} testId="current-max">
          {auction.current_max == null
            ? m.auction_no_bids()
            : formatTenge(auction.current_max)}
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
                <span data-testid="bid-amount" className="tabular-nums">
                  {formatTenge(bid.amount)}
                </span>
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
    </PageShell>
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
      <span data-testid={testId} className="text-lg font-semibold tabular-nums">
        {children}
      </span>
    </div>
  )
}

/** Меньше минуты до конца - счет идет на ходы, а не на минуты. */
const URGENT_MS = 60_000

/**
 * Обратный отсчет по часам сервера (FR-602): клиент только отображает.
 *
 * Это единственное число экрана, которое меняется само и по которому
 * принимают решение прямо сейчас, - поэтому оно крупное и стоит отдельным
 * блоком. Сноской в ряду из четырех подписей его на ходу не прочитать.
 */
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
  const running = auction.status === "running" && endsAt != null
  // У завершенных торгов отсчет останавливается: время окончания уже наступило
  const left = running ? Math.max(0, Date.parse(endsAt) - (now - skew)) : null
  const urgent = left !== null && left <= URGENT_MS

  const value =
    left === null
      ? endsAt == null
        ? m.auction_not_started()
        : "-"
      : `${Math.floor(left / 60_000)}:${String(
          Math.floor((left % 60_000) / 1_000)
        ).padStart(2, "0")}`

  return (
    <section
      aria-labelledby="auction-countdown"
      className={cn(
        "flex flex-col items-center gap-1 rounded-xl border px-6 py-6",
        urgent
          ? "border-amber-500/40 bg-amber-500/10"
          : "border-border bg-muted/50"
      )}
    >
      <h2
        id="auction-countdown"
        className="text-xs font-medium tracking-wide text-muted-foreground uppercase"
      >
        {m.auction_time_left()}
      </h2>
      <span
        data-testid="time-left"
        suppressHydrationWarning
        className={cn(
          "text-4xl leading-none font-semibold tabular-nums sm:text-6xl",
          urgent && "text-amber-700 dark:text-amber-400"
        )}
      >
        {value}
      </span>
    </section>
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
    onSuccess: async () => {
      notifySuccess(m.auction_absent_marked())
      await onDone()
    },
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
            <span className="text-muted-foreground tabular-nums">
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
                // Неявка необратима: оглашается первоначальное предложение,
                // и вернуть участника в круг уже нечем (п. 70)
                <ConfirmAction
                  title={m.auction_absent_confirm_title()}
                  description={m.auction_absent_confirm_description({
                    name: participant.applicant_name,
                    amount: formatTenge(participant.initial_price),
                  })}
                  confirmLabel={m.auction_mark_absent()}
                  onConfirm={() => absent.mutate(participant.application_id)}
                  trigger={
                    <Button
                      variant="outline"
                      size="sm"
                      className="ml-auto"
                      data-testid="mark-absent"
                      disabled={absent.isPending}
                    >
                      {m.auction_mark_absent()}
                    </Button>
                  }
                />
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
      notifySuccess(m.auction_bid_placed())
      await onPlaced()
    },
  })

  const pass = useMutation({
    mutationFn: () => passTurn(auctionId),
    onSuccess: async () => {
      notifySuccess(m.auction_passed())
      await onPlaced()
    },
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
    onSuccess: async () => {
      notifySuccess(m.auction_started())
      await onDone()
    },
  })
  const extend = useMutation({
    mutationFn: () => extendAuction(auctionId),
    onSuccess: async () => {
      notifySuccess(m.auction_extended())
      await onDone()
    },
  })
  const finish = useMutation({
    mutationFn: () => finishAuction(auctionId, early),
    onSuccess: async () => {
      notifySuccess(m.auction_finished())
      await onDone()
    },
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
        {/* Завершение подводит итог: победитель и второе место фиксируются,
            ставок больше не принимают - отыграть это нечем (FR-606) */}
        <ConfirmAction
          title={m.auction_finish_confirm_title()}
          description={
            early
              ? m.auction_finish_confirm_early()
              : m.auction_finish_confirm_description()
          }
          confirmLabel={m.auction_finish()}
          onConfirm={() => finish.mutate()}
          trigger={
            <Button
              variant="outline"
              data-testid="finish-auction"
              disabled={auction.status !== "running" || finish.isPending}
            >
              {m.auction_finish()}
            </Button>
          }
        />
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
