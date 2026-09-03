import { useState } from "react"
import { Link, createFileRoute, useNavigate } from "@tanstack/react-router"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { AppLogo } from "@/components/app-logo"
import { FormAlert } from "@/components/form-alert"
import { SiteHeader } from "@/components/site-header"
import { Button, buttonVariants } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Spinner } from "@/components/ui/spinner"
import { api, authProvidersQuery } from "@/lib/api"
import {
  meQuery,
  problemCode,
  problemMessage,
  resendConfirmation,
  safeRedirectPath,
} from "@/lib/auth"
import { cn } from "@/lib/utils"

// FR-1501: email+пароль, сессии. FR-1502 (контур 2, ADR-0003): вход через
// внешнего провайдера - обычная ссылка на api, поток идет редиректами и
// работает без JS (NFR-04); кнопка появляется, только если провайдер настроен.
// Кабинеты - client-only (ssr: false в /app), форма логина SSR-безопасна.
type LoginSearch = {
  oidc_error?: string
  redirect?: string
  /** Пришли с подтверждения почты: сказать об этом и не молчать */
  confirmed?: boolean
  /** Адрес, который уже назвали на предыдущем шаге - подставляется в поле */
  email?: string
}

export const Route = createFileRoute("/auth/login")({
  // Причина возврата провайдером - только подсказка на странице; параметр
  // необязателен, иначе ссылки на вход обязаны были бы его передавать.
  // `redirect` - адрес, за которым пришли: его ставит охрана `/app`, а
  // проверяет `safeRedirectPath` - параметр приезжает из чужой ссылки.
  validateSearch: (search: Record<string, unknown>): LoginSearch => {
    const target = safeRedirectPath(search["redirect"])
    const email = search["email"]
    return {
      ...(typeof search["oidc_error"] === "string"
        ? { oidc_error: search["oidc_error"] }
        : {}),
      ...(target === undefined ? {} : { redirect: target }),
      ...(search["confirmed"] === true || search["confirmed"] === "true"
        ? { confirmed: true }
        : {}),
      ...(typeof email === "string" && email !== "" && email.length <= 254
        ? { email }
        : {}),
    }
  },
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(authProvidersQuery),
  head: () => ({ meta: [{ title: `${m.auth_login_title()} - ToU Rent` }] }),
  component: LoginPage,
})

function LoginPage() {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const {
    oidc_error: oidcError,
    redirect: redirectTo,
    confirmed,
    email: emailFromLink,
  } = Route.useSearch()
  const { data: providers } = useSuspenseQuery(authProvidersQuery)
  const [email, setEmail] = useState(emailFromLink ?? "")
  const [password, setPassword] = useState("")
  const [resent, setResent] = useState(false)

  const login = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST("/api/v1/auth/login", {
        body: { email, password },
      })
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("login failed")
      }
      return data
    },
    onSuccess: async (user) => {
      queryClient.setQueryData(meQuery.queryKey, user)
      // Возврат туда, откуда прислали: глубокая ссылка на кабинет переживает
      // вход, а без параметра открывается кабинет по роли (`/app`)
      await (redirectTo === undefined
        ? navigate({ to: "/app" })
        : navigate({ href: redirectTo, replace: true }))
    },
  })

  // Учетная запись есть, но канал не подтвержден (403 `email_not_confirmed`).
  // Это не «неверный пароль»: повторять ввод бессмысленно, и до появления
  // этой ветки незавершенная регистрация была тупиком - вход отвечал отказом,
  // а повторная регистрация упиралась в занятый ИИН
  const unconfirmed = problemCode(login.error) === "email_not_confirmed"
  const failed = login.isError && !unconfirmed

  const resend = useMutation({
    mutationFn: () => resendConfirmation(email.trim()),
    onSuccess: () => setResent(true),
  })

  return (
    <>
      <SiteHeader />
      <main
        id="main"
        className="mx-auto flex w-full max-w-[26rem] flex-col gap-6 px-4 py-14 sm:px-6"
      >
        <div className="flex flex-col gap-6 rounded-xl border bg-card p-6 shadow-xs sm:p-8">
          <div className="flex flex-col gap-4">
            <AppLogo variant="auth" />
            <div className="flex flex-col gap-2">
              <h1 className="text-2xl font-semibold tracking-tight">
                {m.auth_login_title()}
              </h1>
              <p className="text-sm text-muted-foreground">
                {m.auth_login_hint()}
              </p>
            </div>
          </div>

          {oidcError !== undefined && (
            <FormAlert>{m.auth_oidc_failed()}</FormAlert>
          )}

          {/* Пришли с подтверждения почты: сказать, что шаг пройден, - иначе
              страница входа выглядит так же, как после отказа */}
          {confirmed === true && (
            <FormAlert tone="success">{m.auth_confirm_done()}</FormAlert>
          )}

          {providers.oidc && (
            <div className="flex flex-col gap-4">
              <a
                href={providers.oidc.login_url}
                data-testid="login-oidc"
                className={cn(buttonVariants({ variant: "outline" }), "w-full")}
              >
                {/* Подпись провайдера приходит с сервера одной строкой
                    по-русски и в винительном падеже («учетную запись
                    университета»), а подставленная в локализованный шаблон
                    давала «Sign in with учетную запись университета» на en
                    и «учетную запись университета арқылы кіру» на kk. Текст
                    кнопки теперь целиком свой в трех локалях, а `oidc.label`
                    в интерфейс не подставляется: падеж и язык серверной
                    строки портал не контролирует (I18-3) */}
                {m.auth_sign_in_university()}
              </a>
              <p className="text-center text-sm text-muted-foreground">
                {m.auth_or_password()}
              </p>
            </div>
          )}

          <form
            className="flex flex-col gap-4"
            onSubmit={(event) => {
              event.preventDefault()
              login.mutate()
            }}
          >
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="login-email">{m.auth_email()}</Label>
              <Input
                id="login-email"
                type="email"
                autoComplete="email"
                inputMode="email"
                spellCheck={false}
                autoCapitalize="none"
                required
                aria-invalid={failed}
                {...(failed && { "aria-describedby": "login-error" })}
                value={email}
                onChange={(event) => setEmail(event.target.value)}
              />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="login-password">{m.auth_password()}</Label>
              <Input
                id="login-password"
                type="password"
                autoComplete="current-password"
                required
                aria-invalid={failed}
                {...(failed && { "aria-describedby": "login-error" })}
                value={password}
                onChange={(event) => setPassword(event.target.value)}
              />
            </div>
            {failed && (
              <FormAlert id="login-error">
                {problemMessage(login.error)}
              </FormAlert>
            )}
            {unconfirmed && (
              <div
                data-testid="login-unconfirmed"
                className="flex flex-col gap-2 rounded-lg bg-amber-500/10 px-3 py-3 text-sm ring-1 ring-amber-500/30"
              >
                <p role="alert" className="text-amber-700 dark:text-amber-400">
                  {m.error_email_not_confirmed()}
                </p>
                {resent && !resend.isError && (
                  <p
                    role="status"
                    className="text-emerald-700 dark:text-emerald-400"
                  >
                    {m.auth_confirm_resent({ email: email.trim() })}
                  </p>
                )}
                {resend.isError && (
                  <p role="alert" className="text-destructive">
                    {problemMessage(resend.error)}
                  </p>
                )}
                <div className="flex flex-wrap items-center gap-3">
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    data-testid="login-resend"
                    disabled={resend.isPending || email.trim() === ""}
                    onClick={() => resend.mutate()}
                  >
                    {resend.isPending && <Spinner data-icon="inline-start" />}
                    {resend.isPending
                      ? m.auth_confirm_resending()
                      : m.auth_confirm_resend()}
                  </Button>
                  <Link
                    to="/auth/confirm"
                    search={{ email: email.trim() }}
                    className="text-primary underline underline-offset-4"
                    data-testid="login-enter-code"
                  >
                    {m.auth_confirm_enter_code()}
                  </Link>
                </div>
              </div>
            )}
            <Button
              type="submit"
              data-testid="login-submit"
              disabled={login.isPending}
            >
              {login.isPending && <Spinner />}
              {login.isPending ? m.auth_signing_in() : m.sign_in()}
            </Button>
          </form>
        </div>

        <div className="flex flex-col gap-2 text-sm text-muted-foreground">
          <p>
            {m.auth_no_account()}{" "}
            <Link
              to="/auth/register"
              className="text-primary underline underline-offset-4"
              data-testid="go-to-register"
            >
              {m.auth_register_submit()}
            </Link>
          </p>
          {/*
            W-07: канала восстановления по почте в контуре 1 нет (T41), и
            обещать «ссылку на восстановление» было бы враньем. Здесь сказано,
            как вернуть доступ на самом деле - через администратора, - и дана
            ссылка на смену пароля для тех, кто его помнит.
          */}
          <p>
            {m.auth_forgot_password()}{" "}
            <Link
              to="/auth/password"
              className="text-primary underline underline-offset-4"
              data-testid="go-to-change-password"
            >
              {m.auth_password_change_title()}
            </Link>
          </p>
        </div>
      </main>
    </>
  )
}
