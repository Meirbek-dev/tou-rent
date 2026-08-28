import { useState } from "react"
import { Link } from "@tanstack/react-router"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
  BanIcon,
  CircleCheckIcon,
  ClockIcon,
  GavelIcon,
  PackageIcon,
} from "lucide-react"

import { m } from "#/paraglide/messages"
import { EmptyState } from "@/components/empty-state"
import { FormAlert } from "@/components/form-alert"
import { Panel } from "@/components/panel"
import { QueryBoundary } from "@/components/query-boundary"
import { Badge } from "@/components/ui/badge"
import { Button, buttonVariants } from "@/components/ui/button"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Skeleton } from "@/components/ui/skeleton"
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
import { notifySuccess } from "@/lib/toast"
import { cn } from "@/lib/utils"

import type { LucideIcon } from "lucide-react"
import type { LotDto } from "@/lib/api"
import type { AuctionDto } from "@/lib/auctions"

type StatusView = {
  variant: "info" | "warning" | "neutral"
  icon: LucideIcon
  label: () => string
}

/**
 * Состояние комнаты - бейдж, а не оттенок текста: цвет здесь несет смысл
 * ровно тот же, что в реестре тендеров (желтый - идет прямо сейчас, синий -
 * назначено, серый - позади). Статус в контракте - строка, поэтому неизвестное
 * значение не рисуется вовсе: показать сырое `running` пользователю нельзя.
 */
const STATUS_VIEWS: Record<string, StatusView> = {
  scheduled: {
    variant: "info",
    icon: ClockIcon,
    label: m.auction_status_scheduled,
  },
  running: {
    variant: "warning",
    icon: GavelIcon,
    label: m.auction_status_running,
  },
  finished: {
    variant: "neutral",
    icon: CircleCheckIcon,
    label: m.auction_status_finished,
  },
  cancelled: {
    variant: "neutral",
    icon: BanIcon,
    label: m.auction_status_cancelled,
  },
}

function AuctionStatusBadge({ status }: { status: string }) {
  const view = STATUS_VIEWS[status]
  if (view === undefined) return null

  return (
    <Badge variant={view.variant}>
      <view.icon aria-hidden="true" />
      {view.label()}
    </Badge>
  )
}

/** Деньги комнаты: до старта - стартовая ставка, в торгах - минимум
 * следующей, после - сумма победителя (FR-606). */
function summary(auction: AuctionDto): string {
  if (auction.status === "finished") {
    return auction.winner_amount == null
      ? m.auction_no_winner()
      : formatTenge(auction.winner_amount)
  }
  if (auction.status === "running") {
    return m.auction_min_next({ amount: formatTenge(auction.min_next_bid) })
  }
  return `${m.auction_starting_bid()}: ${formatTenge(auction.starting_bid)}`
}

/**
 * Онлайн-торги по лотам тендера в кабинете секретаря (FR-601): открытие
 * комнаты фиксирует стартовую ставку (INV-062) и выбранный шаг ≥5 % (Q-019),
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
  return (
    <Panel
      title={m.auction_panel_title()}
      contentClassName="flex flex-col gap-4"
    >
      {lots.length === 0 ? (
        <EmptyState icon={PackageIcon} title={m.auction_lot_none()} />
      ) : (
        <ul className="flex flex-col gap-2">
          {lots.map((lot) => (
            <li key={lot.id}>
              <LotRow lot={lot} />
            </li>
          ))}
        </ul>
      )}

      <ResultsProtocolSection tenderId={tenderId} />
      <RecordingForm tenderId={tenderId} />
    </Panel>
  )
}

/**
 * Строка лота: пока состояние комнаты не загружено, здесь нет ни надписи
 * «комната не открыта», ни кнопки, которая ее открывает - иначе одно
 * нажатие по недогруженному экрану фиксировало бы стартовую ставку.
 */
function LotRow({ lot }: { lot: LotDto }) {
  const queryClient = useQueryClient()
  const auction = useQuery(lotAuctionQuery(lot.id))
  const [stepPercent, setStepPercent] = useState("5")
  const parsedStep = Number(stepPercent)
  const stepInvalid = !Number.isFinite(parsedStep) || parsedStep < 5

  const open = useMutation({
    mutationFn: () => scheduleAuction(lot.id, stepPercent),
    onSuccess: async () => {
      notifySuccess(m.auction_lot_opened_toast())
      await queryClient.invalidateQueries({
        queryKey: lotAuctionQuery(lot.id).queryKey,
      })
    },
  })

  return (
    <article className="flex flex-col gap-2 rounded-lg border px-4 py-3">
      <span className="font-medium">
        {m.auction_lot({ seq: lot.seq, purpose: lot.purpose })}
      </span>
      <QueryBoundary
        query={auction}
        skeleton={<Skeleton className="h-9 w-full rounded-lg" />}
      >
        {(data) => (
          <div className="flex flex-wrap items-center justify-between gap-3">
            {data == null ? (
              <span className="text-sm text-muted-foreground">
                {m.auction_not_scheduled()}
              </span>
            ) : (
              <div className="flex flex-wrap items-center gap-2">
                <AuctionStatusBadge status={data.status} />
                <span className="text-sm text-muted-foreground">
                  {summary(data)}
                </span>
              </div>
            )}
            {data == null ? (
              <form
                className="flex flex-wrap items-end gap-2"
                onSubmit={(event) => {
                  event.preventDefault()
                  if (!stepInvalid) open.mutate()
                }}
              >
                <Field data-invalid={stepInvalid} className="w-40">
                  <FieldLabel htmlFor={`bid-step-${lot.id}`}>
                    {m.auction_step_percent_label()}
                  </FieldLabel>
                  <Input
                    id={`bid-step-${lot.id}`}
                    type="number"
                    inputMode="decimal"
                    min="5"
                    step="any"
                    value={stepPercent}
                    aria-invalid={stepInvalid}
                    onChange={(event) => setStepPercent(event.target.value)}
                  />
                  <FieldDescription>
                    {m.auction_step_percent_hint()}
                  </FieldDescription>
                </Field>
                <Button
                  type="submit"
                  variant="outline"
                  data-testid="open-auction"
                  disabled={open.isPending || stepInvalid}
                >
                  {m.auction_open_room()}
                </Button>
              </form>
            ) : (
              <Link
                to="/app/auctions/$auctionId"
                params={{ auctionId: data.id }}
                className="text-sm underline-offset-4 hover:underline"
              >
                {m.auction_go_to_room()} →
              </Link>
            )}
          </div>
        )}
      </QueryBoundary>
      {open.isError && <FormAlert>{problemMessage(open.error)}</FormAlert>}
    </article>
  )
}

/** Протокол итогов тендера (FR-701): пока неизвестно, сформирован ли он,
 * не показываем ни кнопку формирования, ни ссылку на PDF. */
function ResultsProtocolSection({ tenderId }: { tenderId: string }) {
  const queryClient = useQueryClient()
  const protocol = useQuery(resultsProtocolQuery(tenderId))

  const generateProtocol = useMutation({
    mutationFn: () => generateResultsProtocol(tenderId),
    onSuccess: async () => {
      notifySuccess(m.auction_lot_protocol_toast())
      await queryClient.invalidateQueries({
        queryKey: resultsProtocolQuery(tenderId).queryKey,
      })
    },
  })

  return (
    <div className="flex flex-col gap-2 border-t pt-4">
      <QueryBoundary
        query={protocol}
        skeleton={<Skeleton className="h-9 w-64 max-w-full rounded-lg" />}
      >
        {(data) =>
          data == null ? (
            <div>
              <Button
                variant="outline"
                data-testid="generate-results-protocol"
                onClick={() => generateProtocol.mutate()}
                disabled={generateProtocol.isPending}
              >
                {m.results_protocol_generate()}
              </Button>
            </div>
          ) : (
            <div>
              <a
                href={`/api/v1/tenders/${tenderId}/results-protocol.pdf`}
                target="_blank"
                rel="noreferrer"
                data-testid="results-protocol-pdf"
                className={cn(buttonVariants({ variant: "outline" }))}
              >
                {m.results_protocol_pdf({ number: data.number ?? "" })}
              </a>
            </div>
          )
        }
      </QueryBoundary>
      <p className="text-sm text-muted-foreground">
        {m.results_protocol_hint()}
      </p>
      {generateProtocol.isError && (
        <FormAlert>{problemMessage(generateProtocol.error)}</FormAlert>
      )}
    </div>
  )
}

/**
 * Ссылка на запись торгов (FR-306, п. 72). Появляется, когда итоги подведены:
 * до этого записи не существует, и сервер такую правку отклоняет.
 */
function RecordingForm({ tenderId }: { tenderId: string }) {
  const queryClient = useQueryClient()
  const tender = useQuery(tenderQuery(tenderId))
  const [url, setUrl] = useState<string | null>(null)

  const save = useMutation({
    mutationFn: (value: string) => setRecordingUrl(tenderId, value),
    onSuccess: async () => {
      setUrl(null)
      notifySuccess(m.auction_lot_recording_saved())
      await queryClient.invalidateQueries({
        queryKey: tenderQuery(tenderId).queryKey,
      })
    },
  })

  return (
    <QueryBoundary
      query={tender}
      skeleton={<Skeleton className="h-9 w-full rounded-lg" />}
    >
      {(data) => {
        // Правка допустима только после подведения итогов (п. 72)
        if (
          data == null ||
          (data.status !== "summed_up" && data.status !== "contracted")
        ) {
          return null
        }
        const value = url ?? data.zoom_recording_url ?? ""

        return (
          <form
            className="flex flex-col gap-2 border-t pt-4"
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
              {data.zoom_recording_url != null && (
                <a
                  href={data.zoom_recording_url}
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
              <FormAlert>{problemMessage(save.error)}</FormAlert>
            )}
          </form>
        )
      }}
    </QueryBoundary>
  )
}
