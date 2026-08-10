import { useEffect } from "react"
import { useQueryClient } from "@tanstack/react-query"

// SSE-подписка центра уведомлений (FR-1301): событие обновляет колокольчик
// и историю ≤1 с (критерий Т10). EventSource переподключается сам; браузер
// ходит через свой origin (dev - nitro-прокси, прод - Caddy), cookie сессии
// уходит автоматически.
export function useNotificationStream() {
  const queryClient = useQueryClient()

  useEffect(() => {
    const source = new EventSource("/api/v1/notifications/stream")
    const refresh = () => {
      void queryClient.invalidateQueries({ queryKey: ["notifications"] })
    }
    source.addEventListener("notification", refresh)
    return () => {
      source.removeEventListener("notification", refresh)
      source.close()
    }
  }, [queryClient])
}
