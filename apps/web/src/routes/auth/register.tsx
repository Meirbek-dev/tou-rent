import { useState } from "react"
import { Link, createFileRoute, useNavigate } from "@tanstack/react-router"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { getLocale } from "#/paraglide/runtime"
import { m } from "#/paraglide/messages"
import { AppLogo } from "@/components/app-logo"
import { FormAlert } from "@/components/form-alert"
import { SiteHeader } from "@/components/site-header"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Spinner } from "@/components/ui/spinner"
import { api } from "@/lib/api"
import { meQuery, problemMessage } from "@/lib/auth"
import {
  FULL_NAME_MAX_LENGTH,
  PASSWORD_MIN_LENGTH,
  emailSchema,
  fieldError,
  fullNameSchema,
  passwordSchema,
} from "@/lib/validation"

// FR-1501, FR-1504: регистрация внешнего участника - email, пароль и имя.
// Подтверждение email в контуре 1 автоматическое (ссылка уходит в лог api),
// поэтому сразу после регистрации выполняется вход и участник попадает
// в кабинет. Локаль берется из текущей - интерфейс уже выбран посетителем.
export const Route = createFileRoute("/auth/register")({
  head: () => ({ meta: [{ title: `${m.auth_register_title()} - ToU Rent` }] }),
  component: RegisterPage,
})

function RegisterPage() {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [fullName, setFullName] = useState("")
  const [email, setEmail] = useState("")
  const [password, setPassword] = useState("")
  const [submitAttempted, setSubmitAttempted] = useState(false)

  // Те же границы, что и у `garde` в `auth.rs`: короткий пароль до сих пор
  // отвергался только сервером - после отправки и с общей ошибкой формы
  const nameError = fieldError(fullNameSchema, fullName.trim())
  const emailError = fieldError(emailSchema, email.trim())
  const passwordError = fieldError(passwordSchema, password)
  const hasErrors =
    nameError !== undefined ||
    emailError !== undefined ||
    passwordError !== undefined

  const shown = (value: string, error: string | undefined) =>
    submitAttempted || value !== "" ? error : undefined

  const register = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST("/api/v1/auth/register", {
        body: {
          email: email.trim(),
          password,
          full_name: fullName.trim(),
          locale: getLocale(),
        },
      })
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("register failed")
      }
      // Регистрация не создает сессию - вход выполняется теми же данными
      const login = await api.POST("/api/v1/auth/login", {
        body: { email: email.trim(), password },
      })
      if (login.error !== undefined || login.data === undefined) {
        throw login.error ?? new Error("login after register failed")
      }
      return login.data
    },
    onSuccess: async (user) => {
      queryClient.setQueryData(meQuery.queryKey, user)
      await navigate({ to: "/app" })
    },
  })

  const nameShown = shown(fullName, nameError)
  const emailShown = shown(email, emailError)
  const passwordShown = shown(password, passwordError)

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
                {m.auth_register_title()}
              </h1>
              <p className="text-sm text-muted-foreground">
                {m.auth_register_hint()}
              </p>
            </div>
          </div>

          <form
            className="flex flex-col gap-4"
            onSubmit={(event) => {
              event.preventDefault()
              setSubmitAttempted(true)
              if (hasErrors) return
              register.mutate()
            }}
          >
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="register-name">{m.auth_full_name()}</Label>
              <Input
                id="register-name"
                name="full_name"
                autoComplete="name"
                required
                maxLength={FULL_NAME_MAX_LENGTH}
                aria-invalid={nameShown !== undefined}
                {...(nameShown !== undefined && {
                  "aria-describedby": "register-name-error",
                })}
                value={fullName}
                onChange={(event) => setFullName(event.target.value)}
              />
              <FieldError id="register-name-error" message={nameShown} />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="register-email">{m.auth_email()}</Label>
              <Input
                id="register-email"
                name="email"
                type="email"
                autoComplete="email"
                inputMode="email"
                spellCheck={false}
                autoCapitalize="none"
                required
                aria-invalid={emailShown !== undefined}
                {...(emailShown !== undefined && {
                  "aria-describedby": "register-email-error",
                })}
                value={email}
                onChange={(event) => setEmail(event.target.value)}
              />
              <FieldError id="register-email-error" message={emailShown} />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="register-password">{m.auth_password()}</Label>
              <Input
                id="register-password"
                name="password"
                type="password"
                autoComplete="new-password"
                required
                minLength={PASSWORD_MIN_LENGTH}
                aria-describedby={
                  passwordShown === undefined
                    ? "register-password-hint"
                    : "register-password-hint register-password-error"
                }
                aria-invalid={passwordShown !== undefined}
                value={password}
                onChange={(event) => setPassword(event.target.value)}
              />
              <p
                id="register-password-hint"
                className="text-sm text-muted-foreground"
              >
                {m.auth_password_rule({ min: PASSWORD_MIN_LENGTH })}
              </p>
              <FieldError
                id="register-password-error"
                message={passwordShown}
              />
            </div>
            {register.isError && (
              <FormAlert>{problemMessage(register.error)}</FormAlert>
            )}
            <Button
              type="submit"
              data-testid="register-submit"
              disabled={register.isPending}
            >
              {register.isPending && <Spinner />}
              {register.isPending
                ? m.auth_registering()
                : m.auth_register_submit()}
            </Button>
          </form>
        </div>

        <p className="text-sm text-muted-foreground">
          {m.auth_have_account()}{" "}
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
