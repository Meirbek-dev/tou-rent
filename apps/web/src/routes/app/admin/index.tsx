import { useMemo, useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { MyDeadlines } from "@/components/my-deadlines"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import {
  GRANTABLE_ROLES,
  addCoefficientVersion,
  addHoliday,
  coefficientsQuery,
  grantRole,
  mrpQuery,
  removeHoliday,
  revokeRole,
  setMrp,
  usersQuery,
} from "@/lib/admin"
import { problemMessage } from "@/lib/auth"
import { holidaysQuery } from "@/lib/obligations"

import type { GrantableRole, UserDto } from "@/lib/admin"

// Кабинет департамента цифрового развития (М15, ТЗ § 3): пользователи и роли
// (FR-1503, FR-1902) и справочники расчета - МРП, коэффициенты Прил. 4,
// производственный календарь (FR-1901, FR-202, FR-1701).
//
// FR-202 держится не запретом на правку, а тем, что в лоте лежит снимок
// расчета, а коэффициенты версионируются по дате вступления в силу: админ
// добавляет версию, а не переписывает историю.
export const Route = createFileRoute("/app/admin/")({
  component: AdminHome,
})

function AdminHome() {
  return (
    <div className="flex flex-col gap-10">
      <MyDeadlines />
      <UsersPanel />
      <MrpPanel />
      <CoefficientsPanel />
      <HolidaysPanel />
    </div>
  )
}

/** FR-1902: список пользователей, назначение и отзыв роли (в аудит - триггером). */
function UsersPanel() {
  const queryClient = useQueryClient()
  const { data: page } = useQuery(usersQuery)
  const [role, setRole] = useState<Record<string, GrantableRole>>({})

  const refresh = () => queryClient.invalidateQueries({ queryKey: ["admin"] })

  const grant = useMutation({
    mutationFn: ({ userId, next }: { userId: string; next: GrantableRole }) =>
      grantRole(userId, next),
    onSuccess: refresh,
  })
  const revoke = useMutation({
    mutationFn: ({ userId, next }: { userId: string; next: string }) =>
      revokeRole(userId, next),
    onSuccess: refresh,
  })

  const chosen = (user: UserDto): GrantableRole =>
    role[user.id] ?? GRANTABLE_ROLES[0]

  return (
    <section aria-labelledby="admin-users" className="flex flex-col gap-3">
      <h2 id="admin-users" className="font-heading text-lg font-semibold">
        {m.admin_users_title()}
      </h2>
      <p className="text-sm text-muted-foreground">{m.admin_users_hint()}</p>
      {page === undefined || page.items.length === 0 ? (
        <p className="text-sm text-muted-foreground">{m.admin_users_empty()}</p>
      ) : (
        <div className="overflow-x-auto">
          <Table data-testid="admin-users">
            <TableHeader>
              <TableRow>
                <TableHead scope="col">{m.admin_user()}</TableHead>
                <TableHead scope="col">{m.admin_roles()}</TableHead>
                <TableHead scope="col">{m.admin_grant_role()}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {page.items.map((user) => (
                <TableRow key={user.id}>
                  <TableCell>
                    <span className="font-medium">{user.full_name}</span>
                    <br />
                    <span className="text-sm text-muted-foreground">
                      {user.email}
                    </span>
                  </TableCell>
                  <TableCell>
                    <div className="flex flex-wrap items-center gap-1.5">
                      {user.roles.length === 0 ? (
                        <span className="text-sm text-muted-foreground">
                          {m.admin_no_roles()}
                        </span>
                      ) : (
                        user.roles.map((granted) => (
                          <Badge
                            key={granted}
                            variant="outline"
                            className="gap-1"
                          >
                            {roleLabel(granted)}
                            <button
                              type="button"
                              aria-label={`${m.admin_revoke_role()}: ${roleLabel(granted)}`}
                              className="text-muted-foreground hover:text-destructive"
                              onClick={() =>
                                revoke.mutate({
                                  userId: user.id,
                                  next: granted,
                                })
                              }
                            >
                              ×
                            </button>
                          </Badge>
                        ))
                      )}
                    </div>
                  </TableCell>
                  <TableCell>
                    <div className="flex flex-wrap items-center gap-2">
                      <NativeSelect
                        aria-label={m.admin_grant_role()}
                        value={chosen(user)}
                        onChange={(event) =>
                          setRole((current) => ({
                            ...current,
                            [user.id]: event.target.value as GrantableRole,
                          }))
                        }
                      >
                        {GRANTABLE_ROLES.map((option) => (
                          <NativeSelectOption key={option} value={option}>
                            {roleLabel(option)}
                          </NativeSelectOption>
                        ))}
                      </NativeSelect>
                      <Button
                        variant="outline"
                        size="sm"
                        data-testid={`grant-role-${user.id}`}
                        disabled={grant.isPending}
                        onClick={() =>
                          grant.mutate({ userId: user.id, next: chosen(user) })
                        }
                      >
                        {m.admin_grant_role()}
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}
      {(grant.isError || revoke.isError) && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(grant.error ?? revoke.error)}
        </p>
      )}
    </section>
  )
}

/** Роль в интерфейсе: значения enum домена, перевод - через Paraglide. */
function roleLabel(role: string): string {
  const labels: Record<string, string> = {
    participant: m.role_participant(),
    organizer: m.role_organizer(),
    secretary: m.role_secretary(),
    commission: m.role_commission(),
    board: m.role_board(),
    finance: m.role_finance(),
    admin: m.role_admin(),
  }
  return labels[role] ?? role
}

/** FR-1901: величина МРП на год - база ставки Прил. 4 (Рбс = 1,5 МРП за м²/год). */
function MrpPanel() {
  const queryClient = useQueryClient()
  const { data: years } = useQuery(mrpQuery)
  const [year, setYear] = useState(String(new Date().getFullYear()))
  const [amount, setAmount] = useState("")

  const save = useMutation({
    mutationFn: () => setMrp(Number(year), amount),
    onSuccess: async () => {
      setAmount("")
      await queryClient.invalidateQueries({ queryKey: ["refdata", "mrp"] })
    },
  })

  return (
    <section aria-labelledby="admin-mrp" className="flex flex-col gap-3">
      <h2 id="admin-mrp" className="font-heading text-lg font-semibold">
        {m.admin_mrp_title()}
      </h2>
      <p className="text-sm text-muted-foreground">{m.admin_mrp_hint()}</p>
      {years !== undefined && years.length > 0 && (
        <ul className="flex flex-wrap gap-2" data-testid="admin-mrp-list">
          {years.map((entry) => (
            <li
              key={entry.year}
              className="rounded-lg border px-3 py-1.5 text-sm"
            >
              {entry.year}: {entry.amount} ₸
            </li>
          ))}
        </ul>
      )}
      <form
        className="flex flex-wrap items-end gap-3"
        onSubmit={(event) => {
          event.preventDefault()
          save.mutate()
        }}
      >
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="mrp-year">{m.admin_mrp_year()}</Label>
          <Input
            id="mrp-year"
            type="number"
            min={2000}
            max={2100}
            className="w-32"
            required
            value={year}
            onChange={(event) => setYear(event.target.value)}
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="mrp-amount">{m.admin_mrp_amount()}</Label>
          <Input
            id="mrp-amount"
            inputMode="decimal"
            className="w-40"
            required
            data-testid="mrp-amount"
            value={amount}
            onChange={(event) => setAmount(event.target.value)}
          />
        </div>
        <Button
          type="submit"
          variant="outline"
          data-testid="save-mrp"
          disabled={save.isPending}
        >
          {m.admin_save()}
        </Button>
      </form>
      {save.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(save.error)}
        </p>
      )}
    </section>
  )
}

/**
 * FR-202: новая версия множителя Прил. 4 с датой вступления в силу.
 *
 * Перечень множителей и их опций задан Правилами и закрыт внешним ключом
 * (`refdata.rate_options`), поэтому опция выбирается из списка, а не вводится
 * текстом: админ версионирует значение, а не заводит новую опцию. Список
 * собирается из уже существующих версий - у каждой опции каталога есть
 * действующая или прошлая версия.
 */
function CoefficientsPanel() {
  const queryClient = useQueryClient()
  const { data: versions } = useQuery(coefficientsQuery)
  const [pair, setPair] = useState("")
  const [form, setForm] = useState({ value: "", effective_from: "" })

  const pairs = useMemo(() => {
    const known = new Map<
      string,
      {
        key: string
        coefficient: string
        option_code: string
        label_ru: string
      }
    >()
    for (const version of versions ?? []) {
      const key = `${version.coefficient}|${version.option_code}`
      if (!known.has(key)) {
        known.set(key, {
          key,
          coefficient: version.coefficient,
          option_code: version.option_code,
          label_ru: version.label_ru,
        })
      }
    }
    return [...known.values()]
  }, [versions])

  const add = useMutation({
    mutationFn: () => {
      const chosen = pairs.find((option) => option.key === pair)
      if (chosen === undefined) throw new Error("option not chosen")
      return addCoefficientVersion({
        coefficient: chosen.coefficient,
        option_code: chosen.option_code,
        label_ru: chosen.label_ru,
        label_kk: null,
        label_en: null,
        ...form,
      })
    },
    onSuccess: async () => {
      setForm({ value: "", effective_from: "" })
      await queryClient.invalidateQueries({
        queryKey: ["refdata", "coefficients"],
      })
    },
  })

  const field = (key: keyof typeof form) => ({
    value: form[key],
    onChange: (event: { target: { value: string } }) =>
      setForm((current) => ({ ...current, [key]: event.target.value })),
  })

  return (
    <section
      aria-labelledby="admin-coefficients"
      className="flex flex-col gap-3"
    >
      <h2
        id="admin-coefficients"
        className="font-heading text-lg font-semibold"
      >
        {m.admin_coefficients_title()}
      </h2>
      <p className="text-sm text-muted-foreground">
        {m.admin_coefficients_hint()}
      </p>
      {versions !== undefined && versions.length > 0 && (
        <div className="max-h-96 overflow-auto rounded-lg border">
          <Table data-testid="admin-coefficients">
            <TableHeader>
              <TableRow>
                <TableHead scope="col">{m.admin_coefficient()}</TableHead>
                <TableHead scope="col">{m.admin_option()}</TableHead>
                <TableHead scope="col">{m.admin_value()}</TableHead>
                <TableHead scope="col">{m.admin_effective_from()}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {versions.map((version) => (
                <TableRow key={version.id}>
                  <TableCell className="font-mono text-sm">
                    {version.coefficient}
                  </TableCell>
                  <TableCell>
                    {version.label_ru}
                    <span className="ml-1 font-mono text-xs text-muted-foreground">
                      {version.option_code}
                    </span>
                  </TableCell>
                  <TableCell>{version.value}</TableCell>
                  <TableCell>
                    {version.effective_from}
                    {version.current && (
                      <Badge variant="secondary" className="ml-2">
                        {m.admin_current_version()}
                      </Badge>
                    )}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}
      <form
        className="flex flex-wrap items-end gap-3"
        onSubmit={(event) => {
          event.preventDefault()
          add.mutate()
        }}
      >
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="coefficient-pair">{m.admin_option()}</Label>
          <NativeSelect
            id="coefficient-pair"
            className="w-80"
            required
            data-testid="coefficient-pair"
            value={pair}
            onChange={(event) => setPair(event.target.value)}
          >
            <NativeSelectOption value="">
              {m.admin_pick_option()}
            </NativeSelectOption>
            {pairs.map((option) => (
              <NativeSelectOption key={option.key} value={option.key}>
                {option.coefficient} · {option.label_ru} ({option.option_code})
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="coefficient-value">{m.admin_value()}</Label>
          <Input
            id="coefficient-value"
            className="w-28"
            inputMode="decimal"
            required
            data-testid="coefficient-value"
            {...field("value")}
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="coefficient-from">{m.admin_effective_from()}</Label>
          <Input
            id="coefficient-from"
            type="date"
            required
            {...field("effective_from")}
          />
        </div>
        <Button
          type="submit"
          variant="outline"
          data-testid="add-coefficient"
          disabled={add.isPending}
        >
          {m.admin_add_version()}
        </Button>
      </form>
      {add.isError && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(add.error)}
        </p>
      )}
    </section>
  )
}

/** FR-1701: производственный календарь РК - основа всех «рабочих дней» Правил. */
function HolidaysPanel() {
  const queryClient = useQueryClient()
  const { data: holidays } = useQuery(holidaysQuery)
  const [day, setDay] = useState("")
  const [label, setLabel] = useState("")

  const refresh = () =>
    queryClient.invalidateQueries({ queryKey: ["refdata", "holidays"] })

  const add = useMutation({
    mutationFn: () => addHoliday(day, label),
    onSuccess: async () => {
      setDay("")
      setLabel("")
      await refresh()
    },
  })
  const remove = useMutation({
    mutationFn: (value: string) => removeHoliday(value),
    onSuccess: refresh,
  })

  return (
    <section aria-labelledby="admin-holidays" className="flex flex-col gap-3">
      <h2 id="admin-holidays" className="font-heading text-lg font-semibold">
        {m.admin_holidays_title()}
      </h2>
      <p className="text-sm text-muted-foreground">{m.admin_holidays_hint()}</p>
      {holidays !== undefined && holidays.length > 0 && (
        <ul className="flex flex-wrap gap-2" data-testid="admin-holidays">
          {holidays.map((holiday) => (
            <li
              key={holiday.day}
              className="flex items-center gap-2 rounded-lg border px-3 py-1.5 text-sm"
            >
              <span>
                {holiday.day} - {holiday.label_ru}
              </span>
              <button
                type="button"
                aria-label={`${m.admin_remove()}: ${holiday.day}`}
                className="text-muted-foreground hover:text-destructive"
                onClick={() => remove.mutate(holiday.day)}
              >
                ×
              </button>
            </li>
          ))}
        </ul>
      )}
      <form
        className="flex flex-wrap items-end gap-3"
        onSubmit={(event) => {
          event.preventDefault()
          add.mutate()
        }}
      >
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="holiday-day">{m.admin_holiday_day()}</Label>
          <Input
            id="holiday-day"
            type="date"
            required
            value={day}
            onChange={(event) => setDay(event.target.value)}
          />
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="holiday-label">{m.admin_label()}</Label>
          <Input
            id="holiday-label"
            className="w-64"
            required
            data-testid="holiday-label"
            value={label}
            onChange={(event) => setLabel(event.target.value)}
          />
        </div>
        <Button
          type="submit"
          variant="outline"
          data-testid="add-holiday"
          disabled={add.isPending}
        >
          {m.admin_add()}
        </Button>
      </form>
      {(add.isError || remove.isError) && (
        <p role="alert" className="text-sm text-destructive">
          {problemMessage(add.error ?? remove.error)}
        </p>
      )}
    </section>
  )
}
