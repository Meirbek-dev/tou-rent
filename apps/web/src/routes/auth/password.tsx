import { useState } from "react"
import { Link, createFileRoute } from "@tanstack/react-router"
import { useMutation } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { SiteHeader } from "@/components/site-header"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { changePassword, problemMessage } from "@/lib/auth"
import {
  PASSWORD_MIN_LENGTH,
  fieldError,
  passwordSchema,
} from "@/lib/validation"

// W-07: смена собственного пароля. Страница живет рядом со входом, а не
// в кабинете: сюда приходят и после сброса админом, когда роли у человека
// может не быть вовсе, и с чужого устройства, где кабинет открывать незачем.
// Данных страница не грузит и потому безопасна для SSR; отсутствие сессии
// видно по ответу api - 401 с обычным problem+json.
export const Route = createFileRoute("/auth/password")({
  head: () => ({
    meta: [{ title: `${m.auth_password_change_title()} - ToU Rent` }],
  }),
  component: ChangePasswordPage,
})

function ChangePasswordPage() {
  const [current, setCurrent] = useState("")
  const [next, setNext] = useState("")
  const [repeat, setRepeat] = useState("")
  const [submitAttempted, setSubmitAttempted] = useState(false)

  // Те же границы, что и у `garde` в `auth.rs`; повтор - только на клиенте:
  // серверу второе поле не нужно, а опечатка в новом пароле означала бы
  // потерю доступа сразу после успешной смены
  const nextError = fieldError(passwordSchema, next)
  const repeatError =
    repeat !== next ? m.auth_password_repeat_mismatch() : undefined
  const hasErrors = nextError !== undefined || repeatError !== undefined

  const shown = (value: string, error: string | undefined) =>
    submitAttempted || value !== "" ? error : undefined

  const change = useMutation({
    mutationFn: () => changePassword(current, next),
    onSuccess: () => {
      setCurrent("")
      setNext("")
      setRepeat("")
      setSubmitAttempted(false)
    },
  })

  return (
    <>
      <SiteHeader />
      <main className="mx-auto flex w-full max-w-md flex-col gap-6 px-6 py-16">
        <h1 className="font-heading text-2xl font-semibold">
          {m.auth_password_change_title()}
        </h1>
        <p className="text-sm text-muted-foreground">
          {m.auth_password_change_hint()}
        </p>
        <form
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault()
            setSubmitAttempted(true)
            if (hasErrors) return
            change.mutate()
          }}
        >
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="password-current">
              {m.auth_password_current()}
            </Label>
            <Input
              id="password-current"
              type="password"
              autoComplete="current-password"
              required
              value={current}
              onChange={(event) => setCurrent(event.target.value)}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="password-new">{m.auth_password_new()}</Label>
            <Input
              id="password-new"
              type="password"
              autoComplete="new-password"
              required
              minLength={PASSWORD_MIN_LENGTH}
              aria-describedby="password-new-hint"
              aria-invalid={shown(next, nextError) !== undefined}
              value={next}
              onChange={(event) => setNext(event.target.value)}
            />
            <p id="password-new-hint" className="text-sm text-muted-foreground">
              {m.auth_password_rule({ min: PASSWORD_MIN_LENGTH })}
            </p>
            <FieldError
              id="password-new-error"
              message={shown(next, nextError)}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="password-repeat">{m.auth_password_repeat()}</Label>
            <Input
              id="password-repeat"
              type="password"
              autoComplete="new-password"
              required
              aria-invalid={shown(repeat, repeatError) !== undefined}
              value={repeat}
              onChange={(event) => setRepeat(event.target.value)}
            />
            <FieldError
              id="password-repeat-error"
              message={shown(repeat, repeatError)}
            />
          </div>
          {change.isError && (
            <p role="alert" className="text-sm text-destructive">
              {problemMessage(change.error)}
            </p>
          )}
          {change.isSuccess && (
            <p role="status" className="text-sm">
              {m.auth_password_changed()}
            </p>
          )}
          <Button
            type="submit"
            data-testid="change-password-submit"
            disabled={change.isPending}
          >
            {change.isPending
              ? m.auth_password_changing()
              : m.auth_password_change_submit()}
          </Button>
        </form>
        <p className="text-sm text-muted-foreground">
          <Link to="/auth/login" className="underline underline-offset-4">
            {m.sign_in()}
          </Link>
        </p>
      </main>
    </>
  )
}

/** Ошибка поля рядом с самим полем: `role="alert"` озвучивает ее сразу. */
function FieldError({
  id,
  message,
}: {
  id: string
  message: string | undefined
}) {
  if (message === undefined) return null
  return (
    <p id={id} role="alert" className="text-sm text-destructive">
      {message}
    </p>
  )
}
