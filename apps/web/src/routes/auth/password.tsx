import { useState } from "react"
import { Link, createFileRoute } from "@tanstack/react-router"
import { useMutation } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { AppLogo } from "@/components/app-logo"
import { FormAlert } from "@/components/form-alert"
import { SiteHeader } from "@/components/site-header"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Spinner } from "@/components/ui/spinner"
import { changePassword, problemMessage } from "@/lib/auth"
import {
  PASSWORD_MIN_LENGTH,
  fieldError,
  passwordSchema,
} from "@/lib/validation"

// W-07: смена собственного пароля. Страница живет рядом со входом, а не
// в кабинете: сюда приходят и после сброса админом, когда роли у человека
// может не быть вовсе, и с чужого устройства, где кабинет открывать незачем
// (это же сказано посетителю строкой `auth_password_who`).
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

  const nextShown = shown(next, nextError)
  const repeatShown = shown(repeat, repeatError)

  return (
    <>
      <SiteHeader />
      <main
        id="main"
        className="mx-auto flex w-full max-w-[26rem] flex-col gap-6 px-4 py-14 sm:px-6"
      >
        <div className="flex flex-col gap-6 rounded-xl border bg-card p-6 shadow-xs sm:p-8">
          <div className="flex flex-col gap-4">
            <AppLogo />
            <div className="flex flex-col gap-2">
              <h1 className="text-2xl font-semibold tracking-tight">
                {m.auth_password_change_title()}
              </h1>
              <p className="text-sm text-muted-foreground">
                {m.auth_password_who()}
              </p>
              <p className="text-sm text-muted-foreground">
                {m.auth_password_change_hint()}
              </p>
            </div>
          </div>

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
                aria-invalid={change.isError}
                {...(change.isError && {
                  "aria-describedby": "password-change-error",
                })}
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
                aria-describedby={
                  nextShown === undefined
                    ? "password-new-hint"
                    : "password-new-hint password-new-error"
                }
                aria-invalid={nextShown !== undefined}
                value={next}
                onChange={(event) => setNext(event.target.value)}
              />
              <p
                id="password-new-hint"
                className="text-sm text-muted-foreground"
              >
                {m.auth_password_rule({ min: PASSWORD_MIN_LENGTH })}
              </p>
              <FieldError id="password-new-error" message={nextShown} />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="password-repeat">
                {m.auth_password_repeat()}
              </Label>
              <Input
                id="password-repeat"
                type="password"
                autoComplete="new-password"
                required
                aria-invalid={repeatShown !== undefined}
                {...(repeatShown !== undefined && {
                  "aria-describedby": "password-repeat-error",
                })}
                value={repeat}
                onChange={(event) => setRepeat(event.target.value)}
              />
              <FieldError id="password-repeat-error" message={repeatShown} />
            </div>
            {change.isError && (
              <FormAlert id="password-change-error">
                {problemMessage(change.error)}
              </FormAlert>
            )}
            {change.isSuccess && (
              <FormAlert tone="success">{m.auth_password_changed()}</FormAlert>
            )}
            <Button
              type="submit"
              data-testid="change-password-submit"
              disabled={change.isPending}
            >
              {change.isPending && <Spinner />}
              {change.isPending
                ? m.auth_password_changing()
                : m.auth_password_change_submit()}
            </Button>
          </form>
        </div>

        <p className="text-sm text-muted-foreground">
          <Link
            to="/auth/login"
            className="text-primary underline underline-offset-4"
          >
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
