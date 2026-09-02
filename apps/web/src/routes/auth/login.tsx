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
import { meQuery, problemMessage } from "@/lib/auth"
import { cn } from "@/lib/utils"

// FR-1501: email+пароль, сессии. FR-1502 (контур 2, ADR-0003): вход через
// внешнего провайдера - обычная ссылка на api, поток идет редиректами и
// работает без JS (NFR-04); кнопка появляется, только если провайдер настроен.
// Кабинеты - client-only (ssr: false в /app), форма логина SSR-безопасна.
export const Route = createFileRoute("/auth/login")({
  // Причина возврата провайдером - только подсказка на странице; параметр
  // необязателен, иначе ссылки на вход обязаны были бы его передавать
  validateSearch: (search: Record<string, unknown>): { oidc_error?: string } =>
    typeof search["oidc_error"] === "string"
      ? { oidc_error: search["oidc_error"] }
      : {},
  loader: ({ context }) =>
    context.queryClient.ensureQueryData(authProvidersQuery),
  head: () => ({ meta: [{ title: `${m.auth_login_title()} - ToU Rent` }] }),
  component: LoginPage,
})

function LoginPage() {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { oidc_error: oidcError } = Route.useSearch()
  const { data: providers } = useSuspenseQuery(authProvidersQuery)
  const [email, setEmail] = useState("")
  const [password, setPassword] = useState("")

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
      await navigate({ to: "/app" })
    },
  })

  const failed = login.isError

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
