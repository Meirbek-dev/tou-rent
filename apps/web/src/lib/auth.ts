import { queryOptions } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"

// `#/` вместо `@/`: оба псевдонима ведут в `src/` (tsconfig paths), но `#/`
// объявлен полем `imports` в package.json и потому разрешается не только
// сборщиком, а и раннером тестов. Без этого `auth.test.ts` не поднимается -
// vitest берет конфиг корня репозитория, где tsconfigPaths не включен.
import { api } from "#/lib/api"
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
 * Подробность отказа из problem+json (RFC 9457, NFR-08); `null` - ответ
 * не разбирается.
 *
 * Причина отказа по правилу читается первой и переводится по каталогу:
 * раньше первым был `detail`, а туда попадал текст `RAISE EXCEPTION` из
 * триггера БД - единственная строка интерфейса, существовавшая только
 * по-русски (NFR-01). Теперь сервер шлет машинную причину, `detail`
 * у отказов по правилу пуст, и остальным ошибкам он по-прежнему принадлежит.
 *
 * `null` - это обрыв сети, 502 от прокси, HTML-страница ошибки и собственные
 * броски слоя данных (`new Error("failed to load ...")`): у них нет ни
 * `detail`, ни `title`, и показывать пользователю нечего.
 */
export function problemDetail(error: unknown): string | null {
  if (error && typeof error === "object") {
    const problem = error as Partial<Problem>
    if (typeof problem.rule === "string") {
      return ruleMessage(problem.rule)
    }
    if (typeof problem.detail === "string" && problem.detail !== "") {
      return problem.detail
    }
    if (typeof problem.title === "string" && problem.title !== "") {
      return problem.title
    }
  }
  return null
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
