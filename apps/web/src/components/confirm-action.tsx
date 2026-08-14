import { useState } from "react"

import { m } from "#/paraglide/messages"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog"

import type { ReactElement } from "react"

/**
 * Подтверждение необратимого действия (отмена тендера, возврат депозита,
 * отзыв заявки).
 *
 * Такие кнопки в кабинетах до сих пор срабатывали с первого щелчка: отмена
 * тендера - решение, которое Правила назад не отыгрывают, а стоило оно
 * промаха мышью. Диалог именно `alertdialog`, а не `dialog`: он перехватывает
 * фокус и требует ответа, и это ровно то поведение, которого здесь ждут.
 *
 * Состояние открытия внутреннее: `AlertDialogAction` в этой сборке - обычная
 * кнопка, сама диалог не закрывает, и снаружи это забывали бы делать.
 */
export function ConfirmAction({
  title,
  description,
  confirmLabel,
  cancelLabel,
  variant = "destructive-solid",
  onConfirm,
  disabled = false,
  trigger,
}: {
  title: string
  description: string
  confirmLabel: string
  cancelLabel?: string | undefined
  /** Подтверждающая кнопка: по умолчанию - разрушительное действие */
  variant?: "default" | "destructive" | "destructive-solid"
  onConfirm: () => void
  disabled?: boolean
  /** Кнопка, открывающая диалог */
  trigger: ReactElement
}) {
  const [open, setOpen] = useState(false)

  return (
    <AlertDialog open={open} onOpenChange={setOpen}>
      <AlertDialogTrigger disabled={disabled} render={trigger} />
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          <AlertDialogDescription>{description}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>
            {cancelLabel ?? m.confirm_cancel()}
          </AlertDialogCancel>
          <AlertDialogAction
            variant={variant}
            onClick={() => {
              setOpen(false)
              onConfirm()
            }}
          >
            {confirmLabel}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}
