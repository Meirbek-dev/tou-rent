import { useState } from "react"
import { Link, createFileRoute, useNavigate } from "@tanstack/react-router"
import {
  useMutation,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { SiteHeader } from "@/components/site-header"
import { Button, buttonVariants } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
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

  return (
    <>
      <SiteHeader />
      <main className="mx-auto flex w-full max-w-md flex-col gap-6 px-6 py-16">
        <h1 className="font-heading text-2xl font-semibold">
          {m.auth_login_title()}
        </h1>
        {oidcError !== undefined && (
          <p role="alert" className="text-sm text-destructive">
            {m.auth_oidc_failed()}
          </p>
        )}
        {providers.oidc && (
          <div className="flex flex-col gap-4">
            <a
              href={providers.oidc.login_url}
              data-testid="login-oidc"
              className={cn(buttonVariants({ variant: "outline" }), "w-full")}
            >
              {m.auth_sign_in_with({ provider: providers.oidc.label })}
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
              required
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
              value={password}
              onChange={(event) => setPassword(event.target.value)}
            />
          </div>
          {login.isError && (
            <p role="alert" className="text-sm text-destructive">
              {problemMessage(login.error)}
            </p>
          )}
          <Button
            type="submit"
            data-testid="login-submit"
            disabled={login.isPending}
          >
            {login.isPending ? m.auth_signing_in() : m.sign_in()}
          </Button>
        </form>
        <p className="text-sm text-muted-foreground">
          {m.auth_no_account()}{" "}
          <Link
            to="/auth/register"
            className="underline underline-offset-4"
            data-testid="go-to-register"
          >
            {m.auth_register_submit()}
          </Link>
        </p>
      </main>
    </>
  )
}
