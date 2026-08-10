import { useState } from "react"
import { Link, createFileRoute, useNavigate } from "@tanstack/react-router"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { getLocale } from "#/paraglide/runtime"
import { m } from "#/paraglide/messages"
import { SiteHeader } from "@/components/site-header"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { api } from "@/lib/api"
import { meQuery, problemMessage } from "@/lib/auth"

// FR-1501, FR-1504: регистрация внешнего участника - email, пароль и имя.
// Подтверждение email в контуре 1 автоматическое (ссылка уходит в лог api),
// поэтому сразу после регистрации выполняется вход и участник попадает
// в кабинет. Локаль берется из текущей - интерфейс уже выбран посетителем.
export const Route = createFileRoute("/auth/register")({
  head: () => ({ meta: [{ title: `${m.auth_register_title()} - ToU Rent` }] }),
  component: RegisterPage,
})

/** Парольная политика api: не короче 12 символов (garde, TODO-ENGINEER). */
const PASSWORD_MIN = 12

function RegisterPage() {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [fullName, setFullName] = useState("")
  const [email, setEmail] = useState("")
  const [password, setPassword] = useState("")

  const register = useMutation({
    mutationFn: async () => {
      const { data, error } = await api.POST("/api/v1/auth/register", {
        body: {
          email,
          password,
          full_name: fullName,
          locale: getLocale(),
        },
      })
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("register failed")
      }
      // Регистрация не создает сессию - вход выполняется теми же данными
      const login = await api.POST("/api/v1/auth/login", {
        body: { email, password },
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

  return (
    <>
      <SiteHeader />
      <main className="mx-auto flex w-full max-w-md flex-col gap-6 px-6 py-16">
        <h1 className="font-heading text-2xl font-semibold">
          {m.auth_register_title()}
        </h1>
        <p className="text-sm text-muted-foreground">
          {m.auth_register_hint()}
        </p>
        <form
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault()
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
              maxLength={200}
              value={fullName}
              onChange={(event) => setFullName(event.target.value)}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="register-email">{m.auth_email()}</Label>
            <Input
              id="register-email"
              name="email"
              type="email"
              autoComplete="email"
              required
              value={email}
              onChange={(event) => setEmail(event.target.value)}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="register-password">{m.auth_password()}</Label>
            <Input
              id="register-password"
              name="password"
              type="password"
              autoComplete="new-password"
              required
              minLength={PASSWORD_MIN}
              aria-describedby="register-password-hint"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
            />
            <p
              id="register-password-hint"
              className="text-sm text-muted-foreground"
            >
              {m.auth_password_rule({ min: PASSWORD_MIN })}
            </p>
          </div>
          {register.isError && (
            <p role="alert" className="text-sm text-destructive">
              {problemMessage(register.error)}
            </p>
          )}
          <Button
            type="submit"
            data-testid="register-submit"
            disabled={register.isPending}
          >
            {register.isPending
              ? m.auth_registering()
              : m.auth_register_submit()}
          </Button>
        </form>
        <p className="text-sm text-muted-foreground">
          {m.auth_have_account()}{" "}
          <Link to="/auth/login" className="underline underline-offset-4">
            {m.sign_in()}
          </Link>
        </p>
      </main>
    </>
  )
}
