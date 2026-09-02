import { useEffect, useState } from "react"

/** Как часто пересчитывать «сейчас»: подписи сроков меняются в часах и сутках. */
const TICK_MS = 60_000

/**
 * «Сейчас» в миллисекундах - и только после монтирования.
 *
 * До первого кадра значение `null` намеренно: `Date.now()` в разметке SSR
 * разошелся бы с браузерным при гидратации, а кэш отдал бы протухший момент
 * следующему посетителю (NFR-03). Поэтому все, что считается от «сейчас» -
 * относительные подписи и тон срочности, - дорисовывается, а не рендерится
 * на сервере.
 *
 * Сами вычисления остаются в чистых функциях `@/lib/relative-time`: хук
 * отвечает лишь за момент отсчета.
 */
export function useNowMs(): number | null {
  const [nowMs, setNowMs] = useState<number | null>(null)

  useEffect(() => {
    let timer: ReturnType<typeof setInterval> | undefined
    const frame = requestAnimationFrame(() => {
      setNowMs(Date.now())
      timer = setInterval(() => setNowMs(Date.now()), TICK_MS)
    })
    return () => {
      cancelAnimationFrame(frame)
      if (timer !== undefined) clearInterval(timer)
    }
  }, [])

  return nowMs
}
