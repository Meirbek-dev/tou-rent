import { useEffect, useRef, useState } from "react"
import { useQueryClient } from "@tanstack/react-query"

import { auctionRoomKey } from "@/lib/auctions"

import { api } from "@/lib/api"

import type {
  AuctionDto,
  AuctionRoomDto,
  BidDto,
  CircleParticipantDto,
} from "@/lib/auctions"

/** События WS-комнаты (RoomEvent контракта http-слоя). */
type RoomEvent =
  | { type: "bid"; bid: BidDto; auction: AuctionDto }
  | { type: "state"; auction: AuctionDto }
  | {
      type: "turn"
      participants: CircleParticipantDto[]
      current_turn_application_id: string | null
    }

// Прод: сокет идет на свой origin - апгрейд проксирует Caddy (арх. § 7).
// Dev: nitro-прокси route rules пробрасывает только обычные запросы, upgrade
// в нем не поддержан, поэтому сокет ходит прямо в api. Порт не влияет на
// same-site, cookie сессии уходит (A-024).
//
// `VITE_API_WS_HOST` учитывается и в прод-сборке, а не только в dev: перед
// сборкой не всегда стоит Caddy. Так собран стенд приемки в пайплайне -
// прод-сборка web на :3000 и api на :8080 без обратного прокси, - и там
// сокет на свой origin упирался в тот же nitro, который апгрейд не умеет.
// Пустая переменная (прод-образ ее не задает) оставляет свой origin.
//
// Доступ через точку, а не через скобки: в прод-сборке Vite подставляет
// значение статически именно по `import.meta.env.ИМЯ`, а обращение по ключу
// остается обращением к объекту, которого в бандле нет. Прежняя запись
// со скобками работала только в dev - там `import.meta.env` настоящий.
const WS_HOST: string | undefined = import.meta.env.VITE_API_WS_HOST

function socketUrl(auctionId: string): string {
  const scheme = window.location.protocol === "https:" ? "wss:" : "ws:"
  const host =
    WS_HOST ?? (import.meta.env.DEV ? "localhost:8080" : window.location.host)
  return `${scheme}//${host}/api/v1/auctions/${auctionId}/ws`
}

/** Состояние подписки на комнату - его видно в интерфейсе. */
export type RoomConnection = "connecting" | "online" | "offline"

/** Задержки переподключения, секунды: растут, но не бесконечно. */
const RETRY_DELAYS_MS = [2_000, 4_000, 8_000, 15_000, 30_000]

/**
 * Подписка на комнату торгов (FR-603): ставки и смены состояния приходят
 * всем присутствующим сразу. Сокет ходит через свой origin (dev - nitro,
 * прод - Caddy), cookie сессии уходит автоматически. Разрыв - переподключение
 * с дочитыванием снимка: порядок и время остаются серверными.
 *
 * Состояние подписки возвращается наружу намеренно. Пока его не было, обрыв
 * выглядел как тишина в комнате: тот же максимум, та же очередь, тот же
 * тикающий таймер - и участник мог пропустить свой ход, не зная, что данные
 * устарели. Это самый ответственный экран системы, и молчать ему нельзя.
 */
export function useAuctionRoom(auctionId: string): RoomConnection {
  const queryClient = useQueryClient()
  const [connection, setConnection] = useState<RoomConnection>("connecting")
  // Счетчик неудач живет в ref: перерисовка не должна его сбрасывать
  const attempt = useRef(0)

  useEffect(() => {
    let socket: WebSocket | null = null
    let retry: ReturnType<typeof setTimeout> | undefined
    let closed = false

    const apply = (event: RoomEvent) => {
      queryClient.setQueryData<AuctionRoomDto>(
        auctionRoomKey(auctionId),
        (room) => {
          if (room === undefined) return room
          if (event.type === "state") {
            return { ...room, auction: event.auction }
          }
          if (event.type === "turn") {
            return {
              ...room,
              participants: event.participants,
              current_turn_application_id: event.current_turn_application_id,
            }
          }
          // Дубль ставки при переподключении не удваивает ленту
          const known = room.bids.some((bid) => bid.id === event.bid.id)
          return {
            ...room,
            auction: event.auction,
            bids: known ? room.bids : [...room.bids, event.bid],
          }
        }
      )
    }

    /** Догрузка пропущенного куска ленты по курсору `seq` (FR-607). */
    const catchUp = async () => {
      const room = queryClient.getQueryData<AuctionRoomDto>(
        auctionRoomKey(auctionId)
      )
      if (room === undefined) return
      const lastSeq = room.bids.at(-1)?.seq
      const { data } = await api.GET("/api/v1/auctions/{id}/bids", {
        params: {
          path: { id: auctionId },
          query: lastSeq === undefined ? {} : { after_seq: lastSeq },
        },
      })
      if (data === undefined || data.length === 0) return
      queryClient.setQueryData<AuctionRoomDto>(
        auctionRoomKey(auctionId),
        (current) => {
          if (current === undefined) return current
          const known = new Set(current.bids.map((bid) => bid.id))
          const missed = data.filter((bid) => !known.has(bid.id))
          return missed.length === 0
            ? current
            : { ...current, bids: [...current.bids, ...missed] }
        }
      )
    }

    const connect = () => {
      setConnection((current) =>
        current === "online" ? current : "connecting"
      )
      socket = new WebSocket(socketUrl(auctionId))
      socket.addEventListener("message", (message) => {
        try {
          apply(JSON.parse(message.data as string) as RoomEvent)
        } catch {
          // Нечитаемый кадр игнорируем: состояние дотянет снимок
        }
      })
      socket.addEventListener("open", () => {
        attempt.current = 0
        setConnection("online")
        // Реконнект без потери ленты (FR-607): дочитываем ставки, пришедшие
        // пока сокет был закрыт, по номеру последней известной
        void catchUp()
      })
      socket.addEventListener("close", () => {
        if (closed) return
        setConnection("offline")
        // Снимок комнаты перезапрашивается сразу: таймер, очередь и максимум
        // могли уйти вперед, пока сокета не было
        void queryClient.invalidateQueries({
          queryKey: auctionRoomKey(auctionId),
        })
        // Задержка растет: сервер, который лежит, не поднимется от того,
        // что в него стучат каждые две секунды
        const delay =
          RETRY_DELAYS_MS[Math.min(attempt.current, RETRY_DELAYS_MS.length - 1)]
        attempt.current += 1
        retry = setTimeout(connect, delay)
      })
    }

    connect()

    return () => {
      closed = true
      if (retry !== undefined) clearTimeout(retry)
      socket?.close()
    }
  }, [auctionId, queryClient])

  return connection
}
