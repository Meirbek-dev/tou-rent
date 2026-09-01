import { useState } from "react"
import { Link, createFileRoute, useNavigate } from "@tanstack/react-router"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { getLocale } from "#/paraglide/runtime"
import { m } from "#/paraglide/messages"
import { AppLogo } from "@/components/app-logo"
import { FormAlert } from "@/components/form-alert"
import { SiteHeader } from "@/components/site-header"
import { Button } from "@/components/ui/button"
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import {
  InputOTP,
  InputOTPGroup,
  InputOTPSeparator,
  InputOTPSlot,
} from "@/components/ui/input-otp"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { Spinner } from "@/components/ui/spinner"
import { api } from "@/lib/api"
import { meQuery, problemMessage } from "@/lib/auth"
import {
  FULL_NAME_MAX_LENGTH,
  PASSWORD_MIN_LENGTH,
  emailSchema,
  fieldError,
  fullNameSchema,
  idNumberSchema,
  passwordSchema,
  phoneSchema,
} from "@/lib/validation"
import { REGEXP_ONLY_DIGITS } from "input-otp"

// FR-1501, FR-1504: регистрация по БИН/ИИН с обязательным подтверждением
// Email. Сессия создается только после успешной проверки кода.
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
  const [applicantKind, setApplicantKind] = useState<
    "individual" | "legal_entity"
  >("legal_entity")
  const [idNumber, setIdNumber] = useState("")
  const [phone, setPhone] = useState("")
  const [verificationCode, setVerificationCode] = useState("")
  const [pendingVerification, setPendingVerification] = useState(false)
  const [submitAttempted, setSubmitAttempted] = useState(false)

  // Те же границы, что и у `garde` в `auth.rs`: короткий пароль до сих пор
  // отвергался только сервером - после отправки и с общей ошибкой формы
  const nameError = fieldError(fullNameSchema, fullName.trim())
  const emailError = fieldError(emailSchema, email.trim())
  const passwordError = fieldError(passwordSchema, password)
  const idNumberError = fieldError(idNumberSchema, idNumber.trim())
  const phoneError = fieldError(phoneSchema, phone.trim())
  const hasErrors =
    nameError !== undefined ||
    emailError !== undefined ||
    passwordError !== undefined ||
    idNumberError !== undefined ||
    phoneError !== undefined

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
          applicant_kind: applicantKind,
          id_number: idNumber.trim(),
          phone: phone.trim(),
          verification_channel: "email",
        },
      })
      if (error !== undefined || data === undefined) {
        throw error ?? new Error("register failed")
      }
      return data
    },
    onSuccess: () => setPendingVerification(true),
  })

  const confirm = useMutation({
    mutationFn: async () => {
      const verification = await api.POST("/api/v1/auth/confirm-registration", {
        body: {
          email: email.trim(),
          verification_channel: "email",
          code: verificationCode,
        },
      })
      if (verification.error !== undefined) {
        throw verification.error
      }

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
  const idNumberShown = shown(idNumber, idNumberError)
  const phoneShown = shown(phone, phoneError)

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

          {pendingVerification ? (
            <form
              className="flex flex-col gap-5"
              onSubmit={(event) => {
                event.preventDefault()
                if (verificationCode.length === 8) confirm.mutate()
              }}
            >
              <FieldGroup>
                <Field>
                  <FieldLabel htmlFor="register-code">
                    {m.auth_verification_code()}
                  </FieldLabel>
                  <FieldDescription>
                    {m.auth_verification_hint({ destination: email })}
                  </FieldDescription>
                  <InputOTP
                    id="register-code"
                    name="code"
                    autoComplete="one-time-code"
                    inputMode="numeric"
                    maxLength={8}
                    pattern={REGEXP_ONLY_DIGITS}
                    value={verificationCode}
                    onChange={setVerificationCode}
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
              </FieldGroup>
              {confirm.isError && (
                <FormAlert>{problemMessage(confirm.error)}</FormAlert>
              )}
              <Button
                type="submit"
                disabled={confirm.isPending || verificationCode.length !== 8}
              >
                {confirm.isPending && <Spinner data-icon="inline-start" />}
                {confirm.isPending
                  ? m.auth_verifying()
                  : m.auth_verify_submit()}
              </Button>
            </form>
          ) : (
            <form
              className="flex flex-col gap-5"
              onSubmit={(event) => {
                event.preventDefault()
                setSubmitAttempted(true)
                if (hasErrors) return
                register.mutate()
              }}
            >
              <FieldGroup>
                <Field data-invalid={nameShown !== undefined}>
                  <FieldLabel htmlFor="register-name">
                    {m.auth_full_name()}
                  </FieldLabel>
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
                  <FieldError id="register-name-error">{nameShown}</FieldError>
                </Field>
                <Field>
                  <FieldLabel htmlFor="register-kind">
                    {m.applicant_kind_label()}
                  </FieldLabel>
                  <NativeSelect
                    id="register-kind"
                    name="applicant_kind"
                    value={applicantKind}
                    onChange={(event) =>
                      setApplicantKind(
                        event.target.value as "individual" | "legal_entity"
                      )
                    }
                  >
                    <NativeSelectOption value="legal_entity">
                      {m.applicant_kind_legal()}
                    </NativeSelectOption>
                    <NativeSelectOption value="individual">
                      {m.applicant_kind_individual()}
                    </NativeSelectOption>
                  </NativeSelect>
                </Field>
                <Field data-invalid={idNumberShown !== undefined}>
                  <FieldLabel htmlFor="register-id-number">
                    {m.applicant_id_number_label()}
                  </FieldLabel>
                  <Input
                    id="register-id-number"
                    name="id_number"
                    autoComplete="off"
                    inputMode="numeric"
                    maxLength={12}
                    required
                    aria-invalid={idNumberShown !== undefined}
                    value={idNumber}
                    onChange={(event) => setIdNumber(event.target.value)}
                  />
                  <FieldError>{idNumberShown}</FieldError>
                </Field>
                <Field data-invalid={emailShown !== undefined}>
                  <FieldLabel htmlFor="register-email">
                    {m.auth_email()}
                  </FieldLabel>
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
                  <FieldError id="register-email-error">
                    {emailShown}
                  </FieldError>
                </Field>
                <Field data-invalid={phoneShown !== undefined}>
                  <FieldLabel htmlFor="register-phone">
                    {m.applicant_phone_label()}
                  </FieldLabel>
                  <Input
                    id="register-phone"
                    name="phone"
                    type="tel"
                    autoComplete="tel"
                    inputMode="tel"
                    required
                    aria-invalid={phoneShown !== undefined}
                    value={phone}
                    onChange={(event) => setPhone(event.target.value)}
                  />
                  <FieldError>{phoneShown}</FieldError>
                </Field>
                <Field data-invalid={passwordShown !== undefined}>
                  <FieldLabel htmlFor="register-password">
                    {m.auth_password()}
                  </FieldLabel>
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
                  <FieldDescription id="register-password-hint">
                    {m.auth_password_rule({ min: PASSWORD_MIN_LENGTH })}
                  </FieldDescription>
                  <FieldError id="register-password-error">
                    {passwordShown}
                  </FieldError>
                </Field>
              </FieldGroup>
              {register.isError && (
                <FormAlert>{problemMessage(register.error)}</FormAlert>
              )}
              <Button
                type="submit"
                data-testid="register-submit"
                disabled={register.isPending}
              >
                {register.isPending && <Spinner data-icon="inline-start" />}
                {register.isPending
                  ? m.auth_registering()
                  : m.auth_register_submit()}
              </Button>
            </form>
          )}
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
