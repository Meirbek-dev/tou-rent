import { useEffect, useRef, useState } from "react"
import { Link, createFileRoute, useNavigate } from "@tanstack/react-router"
import { useMutation } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { AppLogo } from "@/components/app-logo"
import { FormAlert } from "@/components/form-alert"
import { SiteHeader } from "@/components/site-header"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import {
  InputOTP,
  InputOTPGroup,
  InputOTPSeparator,
  InputOTPSlot,
} from "@/components/ui/input-otp"
import { Spinner } from "@/components/ui/spinner"
import {
  confirmRegistration,
  problemMessage,
  resendConfirmation,
} from "@/lib/auth"
import { REGEXP_ONLY_DIGITS } from "input-otp"

/** Длина кода подтверждения: восемь цифр (см. `tou_http::verification`). */
const CODE_LENGTH = 8

type ConfirmSearch = { email?: string; code?: string }

/**
 * Код из адреса строкой; `undefined` - на код не похоже.
 *
 * Разбор параметров у роутера - `JSON.parse` на каждом значении, и «94939755»
 * доезжает до компонента числом, а «05486974» - числом 5486974 без старшего
 * нуля. Поэтому значение приводится к строке и дополняется нулями слева до
 * восьми разрядов: код всегда восьмизначный (`tou_http::verification`).
 */
function codeFromSearch(value: unknown): string | undefined {
  if (typeof value !== "string" && typeof value !== "number") return undefined
  const text = String(value)
  if (!/^\d{1,8}$/u.test(text)) return undefined
  return text.padStart(CODE_LENGTH, "0")
}

/**
 * Подтверждение регистрации (FR-1501) как отдельный адрес.
 *
 * Раньше шаг кода жил состоянием компонента регистрации: перезагрузка
 * страницы или закрытая вкладка теряли его вместе с адресом, куда ушел код,
 * и учетная запись оставалась навсегда неподтвержденной - вход отвечал
 * отказом, повторная регистрация упиралась в занятый ИИН. Здесь шаг стал
 * маршрутом: адрес едет в параметре, страницу можно открыть заново,
 * переслать себе и запросить новый код.
 *
 * `code` в параметрах поддержан ради ссылки из письма: если он пришел
 * готовым, форма отправляется сама.
 */
export const Route = createFileRoute("/auth/confirm")({
  // Параметры приезжают из чужой ссылки: строкой берется только то, что
  // похоже на адрес и на код, остальное молча отбрасывается
  validateSearch: (search: Record<string, unknown>): ConfirmSearch => {
    const email = search["email"]
    const code = codeFromSearch(search["code"])
    return {
      ...(typeof email === "string" && email !== "" && email.length <= 254
        ? { email }
        : {}),
      ...(code === undefined ? {} : { code }),
    }
  },
  head: () => ({ meta: [{ title: `${m.auth_confirm_title()} - ToU Rent` }] }),
  component: ConfirmPage,
})

function ConfirmPage() {
  const navigate = useNavigate()
  const { email: emailFromLink, code: codeFromLink } = Route.useSearch()
  const [email, setEmail] = useState(emailFromLink ?? "")
  const [code, setCode] = useState(codeFromLink ?? "")
  const [resent, setResent] = useState(false)
  // Автоотправка кода из ссылки - ровно один раз: без замка StrictMode
  // тратит две попытки из пяти, после которых код гасится совсем
  const autoSubmitted = useRef(false)

  const confirm = useMutation({
    mutationFn: () => confirmRegistration(email.trim(), code),
    onSuccess: async () => {
      // Пароля у этой страницы нет (и держать его в адресе нельзя), поэтому
      // сессия открывается обычным входом - с уже подставленным адресом
      await navigate({
        to: "/auth/login",
        search: {
          confirmed: true,
          ...(email.trim() === "" ? {} : { email: email.trim() }),
        },
        replace: true,
      })
    },
  })

  const resend = useMutation({
    mutationFn: () => resendConfirmation(email.trim()),
    onSuccess: () => setResent(true),
  })

  const ready = email.trim() !== "" && code.length === CODE_LENGTH

  useEffect(() => {
    if (autoSubmitted.current) return
    if (codeFromLink === undefined || codeFromLink.length !== CODE_LENGTH)
      return
    if (emailFromLink === undefined) return
    autoSubmitted.current = true
    confirm.mutate()
  }, [codeFromLink, emailFromLink, confirm])

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
                {m.auth_confirm_title()}
              </h1>
              <p className="text-sm text-muted-foreground">
                {m.auth_confirm_hint()}
              </p>
            </div>
          </div>

          <form
            className="flex flex-col gap-5"
            onSubmit={(event) => {
              event.preventDefault()
              if (ready) confirm.mutate()
            }}
          >
            <Field>
              <FieldLabel htmlFor="confirm-email">{m.auth_email()}</FieldLabel>
              <Input
                id="confirm-email"
                name="email"
                type="email"
                autoComplete="email"
                inputMode="email"
                spellCheck={false}
                autoCapitalize="none"
                required
                value={email}
                onChange={(event) => {
                  setEmail(event.target.value)
                  setResent(false)
                }}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="confirm-code">
                {m.auth_verification_code()}
              </FieldLabel>
              <FieldDescription>
                {m.auth_verification_hint({ destination: email })}
              </FieldDescription>
              <InputOTP
                id="confirm-code"
                name="code"
                autoComplete="one-time-code"
                inputMode="numeric"
                maxLength={CODE_LENGTH}
                pattern={REGEXP_ONLY_DIGITS}
                value={code}
                onChange={setCode}
              >
                <InputOTPGroup>
                  {[0, 1, 2, 3].map((index) => (
                    <InputOTPSlot key={index} index={index} />
                  ))}
                </InputOTPGroup>
                <InputOTPSeparator />
                <InputOTPGroup>
                  {[4, 5, 6, 7].map((index) => (
                    <InputOTPSlot key={index} index={index} />
                  ))}
                </InputOTPGroup>
              </InputOTP>
            </Field>

            {confirm.isError && (
              <FormAlert>{problemMessage(confirm.error)}</FormAlert>
            )}
            {resend.isError && (
              <FormAlert>{problemMessage(resend.error)}</FormAlert>
            )}
            {/* Сервер отвечает одинаково и на живой, и на неизвестный адрес
                (NFR-07), поэтому подпись не обещает, что письмо ушло именно
                вам, - она говорит, что обращение принято */}
            {resent && !resend.isError && (
              <FormAlert tone="success">
                {m.auth_confirm_resent({ email: email.trim() })}
              </FormAlert>
            )}

            <Button
              type="submit"
              data-testid="confirm-submit"
              disabled={confirm.isPending || !ready}
            >
              {confirm.isPending && <Spinner data-icon="inline-start" />}
              {confirm.isPending ? m.auth_verifying() : m.auth_verify_submit()}
            </Button>
            <Button
              type="button"
              variant="outline"
              data-testid="confirm-resend"
              disabled={resend.isPending || email.trim() === ""}
              onClick={() => resend.mutate()}
            >
              {resend.isPending && <Spinner data-icon="inline-start" />}
              {resend.isPending
                ? m.auth_confirm_resending()
                : m.auth_confirm_resend()}
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
