import { queryOptions } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"

// `#/` вместо `@/`: оба псевдонима ведут в `src/` (tsconfig paths), но `#/`
// объявлен полем `imports` в package.json и потому разрешается не только
// сборщиком, а и раннером тестов. Без этого `auth.test.ts` не поднимается -
// vitest берет конфиг корня репозитория, где tsconfigPaths не включен.
import { api } from "#/lib/api"
import { errorCodeMessage } from "#/lib/error-codes"
import { ruleMessage } from "#/lib/rule-messages"

import type { components } from "@tou/api-client"

export type User = components["schemas"]["UserDto"]
export type Problem = components["schemas"]["Problem"]

/** Текущая сессия; null - не аутентифицирован (401 - штатный ответ). */
export const meQuery = queryOptions({
  queryKey: ["auth", "me"],
  queryFn: async (): Promise<User | null> => {
    const { data, response } = await api.GET("/api/v1/auth/me")
    if (response.status === 401) return null
    if (data === undefined) throw new Error("failed to load session")
    return data
  },
  staleTime: 60_000,
})

/**
 * Смена собственного пароля (W-07).
 *
 * Текущий пароль обязателен и проверяется сервером: без него перехваченная
 * сессия превращалась бы в перехваченную навсегда учетную запись. Ответ -
 * 204 без тела, поэтому проверяется только `error`.
 */
export async function changePassword(
  current: string,
  next: string
): Promise<void> {
  const { error } = await api.POST("/api/v1/auth/password", {
    body: { current_password: current, new_password: next },
  })
  if (error !== undefined) throw error as unknown
}

/**
 * Новый код подтверждения на тот же адрес (FR-1501).
 *
 * Ответ сервера всегда 204 - и на неизвестный адрес, и на уже подтвержденный:
 * маршрут открыт до входа, и разные ответы перечисляли бы учетные записи
 * (NFR-07). Поэтому здесь нет ветки «такого адреса нет»: интерфейс говорит
 * ровно то, что знает сервер, - «если учетная запись ждет подтверждения,
 * письмо отправлено».
 */
export async function resendConfirmation(email: string): Promise<void> {
  const { error } = await api.POST("/api/v1/auth/resend-confirmation", {
    body: { email },
  })
  if (error !== undefined) throw error as unknown
}

/** Подтверждение регистрации кодом из письма (FR-1501); 204 без тела. */
export async function confirmRegistration(
  email: string,
  code: string
): Promise<void> {
  const { error } = await api.POST("/api/v1/auth/confirm-registration", {
    body: { email, verification_channel: "email", code },
  })
  if (error !== undefined) throw error as unknown
}

/**
 * Подробность отказа из problem+json (RFC 9457, NFR-08); `null` - ответ
 * не разбирается.
 *
 * Порядок чтения - от самого точного к самому общему: причина отказа по
 * правилу (`rule`), затем текст машинного кода (`code`), и лишь потом то,
 * что прислал сервер словами. Так `title` и `detail` перестали быть
 * объяснением: они составлены по-русски для делопроизводства и на /en/
 * и /kk/ читались как есть - «Неверный email или пароль» английскому
 * участнику (NFR-01).
 *
 * `null` - это обрыв сети, 502 от прокси, HTML-страница ошибки и собственные
 * броски слоя данных (`new Error("failed to load ...")`): у них нет ни кода,
 * ни `detail`, ни `title`, и показывать пользователю нечего.
 */
export function problemDetail(error: unknown): string | null {
  if (error && typeof error === "object") {
    const problem = error as Partial<Problem>
    if (typeof problem.rule === "string") {
      return ruleMessage(problem.rule)
    }

    const detail =
      typeof problem.detail === "string" && problem.detail !== ""
        ? problem.detail
        : null
    const localized =
      typeof problem.code === "string" ? errorCodeMessage(problem.code) : null

    if (localized !== null) {
      // У отказа валидации `detail` называет поле и нарушенное правило
      // словами garde - строка техническая и всегда английская. Она идет
      // второй, после объяснения, а не вместо него.
      return problem.code === "validation_failed" && detail !== null
        ? `${localized} (${detail})`
        : localized
    }
    if (detail !== null) {
      return detail
    }
    if (typeof problem.title === "string" && problem.title !== "") {
      return problem.title
    }
  }
  return null
}

/**
 * Машинный код отказа из problem+json; `undefined` - ответ не разбирается.
 *
 * `problemDetail` отвечает на вопрос «что показать», а форме нужен другой:
 * «какое поле сервер отверг». Один и тот же `invalid_credentials` на входе
 * значит «email или пароль», а на смене пароля - ровно «текущий пароль»,
 * и различает их только код.
 */
export function problemCode(error: unknown): string | undefined {
  if (error !== null && typeof error === "object") {
    const code = (error as Partial<Problem>).code
    if (typeof code === "string") return code
  }
  return undefined
}

/**
 * Адрес возврата после входа (FR-1501); `undefined` - значение не годится.
 *
 * Путь приезжает в query-параметре, то есть из чужой ссылки: без проверки
 * `?redirect=https://evil.example` превратил бы страницу входа в открытый
 * перенаправитель. Годится ровно один вид значения - путь этого же портала:
 * начинается с одного «/», а не с «//» и не с «/\» - оба браузер читает как
 * адрес другого хоста. Управляющие символы браузер из адреса молча
 * выбрасывает, и `/<TAB>/evil.example` после этого снова протокольно-
 * относительный, поэтому строка с ними отвергается целиком. Сам вход
 * исключен отдельно: возврат на `/auth/...` замкнул бы петлю.
 */
export function safeRedirectPath(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined
  if (!value.startsWith("/")) return undefined
  if (value.startsWith("//") || value.startsWith("/\\")) return undefined
  if (value.startsWith("/auth/")) return undefined
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index)
    if (code < 0x20 || code === 0x7f) return undefined
  }
  return value
}

/**
 * Человекочитаемое сообщение об отказе - всегда строка интерфейса.
 *
 * Последней веткой здесь стоял литерал `"unknown_error"`: не ключ Paraglide,
 * а служебное слово, которое доезжало до `<FormAlert>` на любой неразобранной
 * ошибке. Отфильтрован он был ровно в одном месте из тридцати восьми -
 * сравнением строк. Теперь неизвестное объясняется словами в трех локалях,
 * а «есть ли что показать» спрашивают у `problemDetail`, а не у текста.
 */
export function problemMessage(error: unknown): string {
  return problemDetail(error) ?? m.error_unknown()
}

/** Кабинеты по ролям (ТЗ § 8, INV-POL-01). Ключи - snake_case ролей домена. */
export const CABINET_PATHS: Record<string, string> = {
  participant: "/app/participant",
  organizer: "/app/organizer",
  secretary: "/app/secretary",
  commission: "/app/commission",
  finance: "/app/finance",
  board: "/app/board",
  admin: "/app/admin",
}

/**
 * Кабинеты пользователя вместе с адресами (ТЗ § 8).
 *
 * Возвращается пара, а не одна роль: иначе на каждом месте вызова словарь
 * читается дважды - сперва `role in CABINET_PATHS`, затем `CABINET_PATHS[role]`,
 * - и связь между проверкой и чтением остается на совести пишущего. При
 * noUncheckedIndexedAccess второе чтение к тому же дает `string | undefined`,
 * которого `<Link to>` не принимает. Здесь проверка и есть чтение.
 */
export function userCabinets(
  roles: readonly string[]
): { role: string; path: string }[] {
  return roles.flatMap((role) => {
    const path = CABINET_PATHS[role]
    return path === undefined ? [] : [{ role, path }]
  })
}

/** Подпись кабинета роли: одна на весь интерфейс (ТЗ § 8). */
const CABINET_LABELS: Record<string, () => string> = {
  participant: m.cabinet_participant,
  organizer: m.cabinet_organizer,
  secretary: m.cabinet_secretary,
  commission: m.cabinet_commission,
  finance: m.cabinet_finance,
  board: m.cabinet_board,
  admin: m.cabinet_admin,
}

export function cabinetLabel(role: string): string {
  return CABINET_LABELS[role]?.() ?? role
}
