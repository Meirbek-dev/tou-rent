import { useMemo, useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { m } from "#/paraglide/messages"
import { getLocale } from "#/paraglide/runtime"
import { ConfirmAction } from "@/components/confirm-action"
import { PageHeader } from "@/components/page-header"
import { Panel } from "@/components/panel"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Switch } from "@/components/ui/switch"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { Textarea } from "@/components/ui/textarea"
import {
  GRANTABLE_ROLES,
  PURGE_CONFIRMATION,
  addCoefficientVersion,
  addHoliday,
  adminSiteAnnouncementQuery,
  auditChainQuery,
  coefficientsQuery,
  dataOverviewQuery,
  deactivateDemoAccounts,
  grantRole,
  mrpQuery,
  purgeData,
  purgeRecord,
  recordsQuery,
  removeHoliday,
  resetPassword,
  revokeRole,
  setMrp,
  setUserActive,
  saveSiteAnnouncement,
  usersQuery,
} from "@/lib/admin"
import { meQuery, problemMessage } from "@/lib/auth"
import { formatDateTime, formatTenge } from "@/lib/format"
import { holidaysQuery } from "@/lib/obligations"
import { notifyError, notifySuccess } from "@/lib/toast"
import { tabSearch } from "@/lib/tabs"

import type {
  AdminDataKind,
  AdminDataOverviewDto,
  AdminPurgeScope,
  AdminRecordDto,
  GrantableRole,
  SiteAnnouncementDto,
  UserDto,
} from "@/lib/admin"

// Кабинет департамента цифрового развития (М15, ТЗ § 3): пользователи и роли
// (FR-1503, FR-1902) и справочники расчета - МРП, коэффициенты Прил. 4,
// производственный календарь (FR-1901, FR-202, FR-1701).
//
// FR-202 держится не запретом на правку, а тем, что в лоте лежит снимок
// расчета, а коэффициенты версионируются по дате вступления в силу: админ
// добавляет версию, а не переписывает историю.
/** Разделы кабинета: люди отдельно, справочники расчета - отдельно,
 * очистка данных стенда - в самом конце, подальше от повседневного. */
const TABS = [
  "users",
  "announcement",
  "mrp",
  "coefficients",
  "holidays",
  "data",
] as const

export const Route = createFileRoute("/app/admin/")({
  validateSearch: tabSearch(TABS),
  head: () => ({ meta: [{ title: `${m.cabinet_admin()} - ToU Rent` }] }),
  component: AdminHome,
})

function AdminHome() {
  const tab = Route.useSearch().tab ?? "users"
  const navigate = Route.useNavigate()

  return (
    <div className="flex flex-col gap-6">
      {/* Имя кабинета - заголовок страницы: из макета он ушел вместе
          с прежней шапкой (каркас называет кабинет группой боковой
          навигации) */}
      <PageHeader title={m.cabinet_admin()} />
      {/* Состояние цепочки аудита - выше вкладок и вне их: разрыв ставит
          под вопрос доказательную базу всей системы, и увидеть его нужно
          независимо от того, какой справочник сейчас открыт */}
      <AuditChainPanel />
      <Tabs
        value={tab}
        onValueChange={(value) => {
          void navigate({
            search: { tab: value as (typeof TABS)[number] },
            replace: true,
          })
        }}
        className="gap-6"
      >
        <TabsList className="max-w-full overflow-x-auto">
          <TabsTrigger value="users">{m.admin_users_title()}</TabsTrigger>
          <TabsTrigger value="announcement">
            {m.admin_announcement_tab()}
          </TabsTrigger>
          <TabsTrigger value="mrp">{m.admin_mrp_title()}</TabsTrigger>
          <TabsTrigger value="coefficients">
            {m.admin_coefficients_title()}
          </TabsTrigger>
          <TabsTrigger value="holidays">{m.admin_holidays_title()}</TabsTrigger>
          <TabsTrigger value="data">{m.admin_data_tab()}</TabsTrigger>
        </TabsList>
        {/* Одна панель на выбранную вкладку: справочники ниже сами ходят
            в сеть, и держать разметку всех четырех ради вкладки, которую
            сейчас не смотрят, незачем */}
        <TabsContent value={tab}>
          {tab === "users" && <UsersPanel />}
          {tab === "announcement" && <SiteAnnouncementPanel />}
          {tab === "mrp" && <MrpPanel />}
          {tab === "coefficients" && <CoefficientsPanel />}
          {tab === "holidays" && <HolidaysPanel />}
          {tab === "data" && <DataPanel />}
        </TabsContent>
      </Tabs>
    </div>
  )
}

/** Вид данных вкладки: область очистки, подпись и счетчик в обзоре. */
type DataKindRow = {
  scope: AdminDataKind
  label: () => string
  count: (counts: AdminDataOverviewDto["counts"]) => number
}

/**
 * Виды данных в порядке от корней графа процедур к листьям: так же они
 * идут в таблице обзора и в селекторе записей. Подпись - та же, что у
 * счетчика: у одного вида одно имя во всех местах вкладки.
 */
const DATA_KINDS: DataKindRow[] = [
  {
    scope: "objects",
    label: m.admin_data_count_objects,
    count: (c) => c.objects,
  },
  {
    scope: "tenders",
    label: m.admin_data_count_tenders,
    count: (c) => c.tenders,
  },
  { scope: "lots", label: m.admin_data_count_lots, count: (c) => c.lots },
  {
    scope: "applications",
    label: m.admin_data_count_applications,
    count: (c) => c.applications,
  },
  {
    scope: "protocols",
    label: m.admin_data_count_protocols,
    count: (c) => c.protocols,
  },
  {
    scope: "auctions",
    label: m.admin_data_count_auctions,
    count: (c) => c.auctions,
  },
  {
    scope: "contracts",
    label: m.admin_data_count_contracts,
    count: (c) => c.contracts,
  },
  { scope: "acts", label: m.admin_data_count_acts, count: (c) => c.acts },
  {
    scope: "ledger_entries",
    label: m.admin_data_count_ledger_entries,
    count: (c) => c.ledger_entries,
  },
  {
    scope: "special_requests",
    label: m.admin_data_count_special_requests,
    count: (c) => c.special_requests,
  },
  {
    scope: "land_plots",
    label: m.admin_data_count_land_plots,
    count: (c) => c.land_plots,
  },
  {
    scope: "investment_contracts",
    label: m.admin_data_count_investment_contracts,
    count: (c) => c.investment_contracts,
  },
  {
    scope: "dossier_items",
    label: m.admin_data_count_dossier_items,
    count: (c) => c.dossier_items,
  },
  {
    scope: "public_records",
    label: m.admin_data_count_public_records,
    count: (c) => c.public_records,
  },
  {
    scope: "obligations",
    label: m.admin_data_count_obligations,
    count: (c) => c.obligations,
  },
  {
    scope: "notifications",
    label: m.admin_data_count_notifications,
    count: (c) => c.notifications,
  },
]

function kindLabel(kind: AdminDataKind): string {
  return DATA_KINDS.find((row) => row.scope === kind)?.label() ?? kind
}

/**
 * Очистка данных стенда (М15). Стенд, наполненный сидом под демонстрацию,
 * становится рабочим: демо-тендеры, объекты, заявки и протоколы должны уйти
 * до прихода настоящих процедур. Само удаление живет в БД одной транзакцией
 * со следом в аудите; здесь - обзор того, что уйдет, и действия за
 * подтверждением: точечно по одной записи любого вида, все записи вида и
 * весь стенд разом. Без `ALLOW_DATA_PURGE` на стороне api кнопки удаления
 * не действуют, и панель говорит об этом прямо, а не молчит серой кнопкой.
 */
function DataPanel() {
  const queryClient = useQueryClient()
  const { data: overview } = useQuery(dataOverviewQuery)
  const [phrase, setPhrase] = useState("")
  const [kind, setKind] = useState<AdminDataKind>("tenders")

  // После очистки устаревает все, что кабинеты успели закешировать:
  // реестры, карточки тендеров, уведомления - кеш сбрасывается целиком
  const refresh = () => queryClient.invalidateQueries()

  const purge = useMutation({
    mutationFn: (scope: AdminPurgeScope) => purgeData(scope, phrase),
    onSuccess: async (result, scope) => {
      const rows = deletedRows(result.deleted)
      notifySuccess(
        scope === "everything"
          ? m.admin_data_purged_toast({ rows })
          : m.admin_data_kind_deleted_toast({ rows })
      )
      await refresh()
    },
    onError: (error: unknown) => notifyError(problemMessage(error)),
  })
  const purgeOne = useMutation({
    mutationFn: ({
      recordKind,
      id,
    }: {
      recordKind: AdminDataKind
      id: string
    }) => purgeRecord(recordKind, id),
    onSuccess: async (result) => {
      notifySuccess(
        m.admin_data_record_deleted_toast({ rows: deletedRows(result.deleted) })
      )
      await refresh()
    },
    onError: (error: unknown) => notifyError(problemMessage(error)),
  })
  const demo = useMutation({
    mutationFn: deactivateDemoAccounts,
    onSuccess: async (result) => {
      notifySuccess(
        m.admin_data_demo_deactivated_toast({ count: result.deactivated })
      )
      await refresh()
    },
    onError: (error: unknown) => notifyError(problemMessage(error)),
  })

  if (overview === undefined) return null
  const enabled = overview.purge_enabled
  const { counts } = overview
  // Слово набрано целиком: массовые кнопки спят до этого момента, а
  // подтверждение в диалоге - второй рубеж, не единственный
  const armed = enabled && phrase.trim() === PURGE_CONFIRMATION

  return (
    <div className="flex flex-col gap-6">
      {!enabled && (
        <div
          role="alert"
          data-testid="admin-data-disabled"
          className="flex flex-col gap-1 rounded-lg border border-destructive p-3 text-sm"
        >
          <span className="font-medium">{m.admin_data_disabled_title()}</span>
          <span className="text-muted-foreground">
            {m.admin_data_disabled_hint()}
          </span>
        </div>
      )}

      <Panel
        title={m.admin_data_counts_title()}
        description={m.admin_data_mass_hint()}
      >
        <div className="flex flex-col gap-4">
          <Field>
            <FieldLabel htmlFor="purge-phrase">
              {m.admin_data_purge_phrase_label({ phrase: PURGE_CONFIRMATION })}
            </FieldLabel>
            <Input
              id="purge-phrase"
              className="max-w-xs"
              value={phrase}
              autoComplete="off"
              disabled={!enabled}
              onChange={(event) => setPhrase(event.target.value)}
            />
          </Field>
          <div className="overflow-x-auto">
            <Table data-testid="admin-data-counts">
              <TableBody>
                {DATA_KINDS.map((row) => {
                  const count = row.count(counts)
                  return (
                    <TableRow key={row.scope}>
                      <TableCell className="text-muted-foreground">
                        {row.label()}
                      </TableCell>
                      <TableCell className="font-medium tabular-nums">
                        {count}
                      </TableCell>
                      <TableCell>
                        <div className="flex flex-wrap items-center gap-2">
                          {/* Перечень записей вида - ниже, в панели записей:
                              кнопка лишь переключает ее на этот вид */}
                          <Button
                            variant="outline"
                            size="sm"
                            data-testid={`show-kind-${row.scope}`}
                            disabled={count === 0}
                            onClick={() => setKind(row.scope)}
                          >
                            {m.admin_data_show()}
                          </Button>
                          <ConfirmAction
                            title={m.admin_data_kind_confirm_title({
                              kind: row.label(),
                            })}
                            description={m.admin_data_kind_confirm_description({
                              kind: row.label(),
                              count,
                            })}
                            confirmLabel={m.admin_data_delete_all()}
                            disabled={!armed || count === 0 || purge.isPending}
                            onConfirm={() => purge.mutate(row.scope)}
                            trigger={
                              <Button
                                variant="destructive"
                                size="sm"
                                data-testid={`purge-kind-${row.scope}`}
                              >
                                {m.admin_data_delete_all()}
                              </Button>
                            }
                          />
                        </div>
                      </TableCell>
                    </TableRow>
                  )
                })}
                <TableRow>
                  <TableCell className="font-medium">
                    {m.admin_data_everything()}
                  </TableCell>
                  <TableCell />
                  <TableCell>
                    <ConfirmAction
                      title={m.admin_data_purge_confirm_title()}
                      description={m.admin_data_purge_confirm_description({
                        tenders: counts.tenders,
                        objects: counts.objects,
                        applications: counts.applications,
                      })}
                      confirmLabel={m.admin_data_purge_button()}
                      disabled={!armed || purge.isPending}
                      onConfirm={() => purge.mutate("everything")}
                      trigger={
                        <Button
                          variant="destructive"
                          size="sm"
                          data-testid="purge-all"
                        >
                          {m.admin_data_purge_button()}
                        </Button>
                      }
                    />
                  </TableCell>
                </TableRow>
              </TableBody>
            </Table>
          </div>
        </div>
      </Panel>

      <RecordsPanel
        kind={kind}
        onKindChange={setKind}
        enabled={enabled}
        pending={purgeOne.isPending}
        onDelete={(id) => purgeOne.mutate({ recordKind: kind, id })}
      />

      <Panel
        title={m.admin_data_demo_title()}
        description={m.admin_data_demo_hint()}
      >
        <div className="flex flex-wrap items-center gap-3">
          <span className="text-sm" data-testid="admin-demo-count">
            {m.admin_data_demo_count({ count: counts.demo_accounts })}
          </span>
          <ConfirmAction
            title={m.admin_data_demo_confirm_title()}
            description={m.admin_data_demo_confirm_description()}
            confirmLabel={m.admin_data_demo_button()}
            disabled={demo.isPending || counts.demo_accounts === 0}
            onConfirm={() => demo.mutate()}
            trigger={
              <Button variant="outline" data-testid="deactivate-demo">
                {m.admin_data_demo_button()}
              </Button>
            }
          />
        </div>
      </Panel>
    </div>
  )
}

/**
 * Записи одного вида с кнопкой удаления у каждой. Свой компонент, потому
 * что перечень ходит в сеть сам: вид меняется селектором, и запрос за
 * записями не должен тянуть за собой перерисовку обзора.
 */
function RecordsPanel({
  kind,
  onKindChange,
  enabled,
  pending,
  onDelete,
}: {
  kind: AdminDataKind
  onKindChange: (kind: AdminDataKind) => void
  enabled: boolean
  pending: boolean
  onDelete: (id: string) => void
}) {
  const { data: page } = useQuery(recordsQuery(kind))
  const label = kindLabel(kind)

  return (
    <Panel
      title={m.admin_data_records_title()}
      description={m.admin_data_records_hint()}
    >
      <div className="flex flex-col gap-4">
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="records-kind">
            {m.admin_data_records_kind_label()}
          </Label>
          <NativeSelect
            id="records-kind"
            className="max-w-xs"
            value={kind}
            onChange={(event) => {
              const next = DATA_KINDS.find(
                (row) => row.scope === event.target.value
              )
              if (next !== undefined) onKindChange(next.scope)
            }}
          >
            {DATA_KINDS.map((row) => (
              <NativeSelectOption key={row.scope} value={row.scope}>
                {row.label()}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </div>
        {page === undefined ? null : page.items.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {m.admin_data_records_empty()}
          </p>
        ) : (
          <div className="overflow-x-auto">
            <Table data-testid="admin-data-records">
              <TableHeader>
                <TableRow>
                  <TableHead scope="col">{m.admin_data_record()}</TableHead>
                  <TableHead scope="col">
                    {m.admin_data_record_details()}
                  </TableHead>
                  <TableHead scope="col">
                    {m.admin_data_record_created()}
                  </TableHead>
                  <TableHead scope="col">
                    <span className="sr-only">
                      {m.admin_data_delete_record()}
                    </span>
                  </TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {page.items.map((record) => {
                  const title = localizedRecordTitle(record)
                  return (
                    <TableRow key={record.id}>
                      <TableCell className="font-medium">{title}</TableCell>
                      <TableCell className="text-muted-foreground">
                        {record.details ?? ""}
                      </TableCell>
                      <TableCell suppressHydrationWarning>
                        {formatDateTime(record.created_at) ?? ""}
                      </TableCell>
                      <TableCell>
                        <ConfirmAction
                          title={m.admin_data_delete_record_confirm_title()}
                          description={m.admin_data_delete_record_confirm_description(
                            { title, kind: label }
                          )}
                          confirmLabel={m.admin_data_delete_record()}
                          disabled={!enabled || pending}
                          onConfirm={() => onDelete(record.id)}
                          trigger={
                            <Button
                              variant="destructive"
                              size="sm"
                              data-testid={`purge-record-${record.id}`}
                            >
                              {m.admin_data_delete_record()}
                            </Button>
                          }
                        />
                      </TableCell>
                    </TableRow>
                  )
                })}
              </TableBody>
            </Table>
          </div>
        )}
        {page?.truncated === true && (
          <p className="text-sm text-muted-foreground">
            {m.admin_data_records_truncated()}
          </p>
        )}
      </div>
    </Panel>
  )
}

/** Сохраненная казахская строка, если она есть; машинного перевода нет. */
function localizedRecordTitle(
  record: Pick<AdminRecordDto, "title" | "title_kk">
): string {
  return getLocale() === "kk" && record.title_kk != null
    ? record.title_kk
    : record.title
}

/** Сумма удаленных строк по таблицам - для одной строки тоста. */
function deletedRows(deleted: Record<string, unknown>): number {
  return Object.values(deleted).reduce<number>(
    (sum, value) => sum + (typeof value === "number" ? value : 0),
    0
  )
}

function SiteAnnouncementPanel() {
  const { data: announcement, dataUpdatedAt } = useQuery(
    adminSiteAnnouncementQuery
  )

  return (
    <SiteAnnouncementEditor
      key={dataUpdatedAt}
      initialForm={initialAnnouncementForm(announcement)}
    />
  )
}

type AnnouncementForm = Parameters<typeof saveSiteAnnouncement>[0]

function initialAnnouncementForm(
  announcement: SiteAnnouncementDto | null | undefined
): AnnouncementForm {
  if (announcement !== null && announcement !== undefined) {
    return {
      title: announcement.title,
      title_kk: announcement.title_kk,
      body: announcement.body,
      body_kk: announcement.body_kk,
      is_published: announcement.is_published,
    }
  }
  if (announcement === undefined) {
    return {
      title: "",
      title_kk: "",
      body: "",
      body_kk: "",
      is_published: false,
    }
  }
  return {
    title: m.admin_announcement_default_title({}, { locale: "ru" }),
    title_kk: m.admin_announcement_default_title({}, { locale: "kk" }),
    body: m.admin_announcement_default_body({}, { locale: "ru" }),
    body_kk: m.admin_announcement_default_body({}, { locale: "kk" }),
    is_published: false,
  }
}

function SiteAnnouncementEditor({
  initialForm,
}: {
  initialForm: AnnouncementForm
}) {
  const queryClient = useQueryClient()
  const [form, setForm] = useState(() => initialForm)

  const save = useMutation({
    mutationFn: () => saveSiteAnnouncement(form),
    onSuccess: async (saved) => {
      if (saved !== null) setForm(saved)
      notifySuccess(m.admin_announcement_saved())
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: ["admin", "site-announcement"],
        }),
        queryClient.invalidateQueries({ queryKey: ["site-announcement"] }),
      ])
    },
    onError: (error) => notifyError(problemMessage(error)),
  })

  return (
    <Panel
      title={m.admin_announcement_title()}
      description={m.admin_announcement_hint()}
    >
      <form
        onSubmit={(event) => {
          event.preventDefault()
          save.mutate()
        }}
      >
        <FieldGroup>
          <FieldSet>
            <FieldLegend>{m.admin_announcement_ru_version()}</FieldLegend>
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="site-announcement-title">
                  {m.admin_announcement_heading()}
                </FieldLabel>
                <Input
                  id="site-announcement-title"
                  value={form.title}
                  maxLength={200}
                  required
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      title: event.target.value,
                    }))
                  }
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="site-announcement-body">
                  {m.admin_announcement_body()}
                </FieldLabel>
                <Textarea
                  id="site-announcement-body"
                  className="min-h-64"
                  value={form.body}
                  maxLength={20_000}
                  required
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      body: event.target.value,
                    }))
                  }
                />
              </Field>
            </FieldGroup>
          </FieldSet>
          <FieldSet>
            <FieldLegend>{m.admin_announcement_kk_version()}</FieldLegend>
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="site-announcement-title-kk">
                  {m.admin_announcement_heading()}
                </FieldLabel>
                <Input
                  id="site-announcement-title-kk"
                  value={form.title_kk}
                  maxLength={200}
                  required
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      title_kk: event.target.value,
                    }))
                  }
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="site-announcement-body-kk">
                  {m.admin_announcement_body()}
                </FieldLabel>
                <Textarea
                  id="site-announcement-body-kk"
                  className="min-h-64"
                  value={form.body_kk}
                  maxLength={20_000}
                  required
                  onChange={(event) =>
                    setForm((current) => ({
                      ...current,
                      body_kk: event.target.value,
                    }))
                  }
                />
              </Field>
            </FieldGroup>
          </FieldSet>
          <FieldDescription>
            {m.admin_announcement_plain_text_hint()}
          </FieldDescription>
          <Field orientation="horizontal">
            <Switch
              id="site-announcement-published"
              checked={form.is_published}
              onCheckedChange={(isPublished) =>
                setForm((current) => ({
                  ...current,
                  is_published: isPublished,
                }))
              }
            />
            <FieldLabel htmlFor="site-announcement-published">
              {m.admin_announcement_publish()}
            </FieldLabel>
          </Field>
          <Button type="submit" disabled={save.isPending}>
            {m.admin_announcement_save()}
          </Button>
        </FieldGroup>
      </form>
    </Panel>
  )
}

/**
 * INV-A01: состояние hash-цепочки аудита - дата и итог последней сверки.
 *
 * Сверку ведет фоновый воркер, и до сих пор ее итог существовал только
 * строкой в журнале контейнера, которая уходила из ротации. Плашка стоит
 * выше справочников намеренно: разрыв цепочки означает, что доказательная
 * база системы под вопросом, и увидеть это нужно раньше, чем МРП.
 *
 * Отсутствие сверок - такое же состояние, как разрыв, и показывается так же
 * заметно: «никто не проверял» и «проверено, цела» путать нельзя.
 */
function AuditChainPanel() {
  const { data: chain } = useQuery(auditChainQuery)

  if (chain === undefined) return null

  const checked = formatDateTime(chain.checked_at)
  const alarming = checked === null || chain.intact === false

  return (
    <Panel title={m.admin_audit_chain_title()} titleAs="h2">
      <div
        data-testid="admin-audit-chain"
        role={alarming ? "alert" : undefined}
        className={
          alarming
            ? "flex flex-col gap-1.5 rounded-lg border border-destructive p-3 text-sm"
            : "flex flex-col gap-1.5 rounded-lg border p-3 text-sm"
        }
      >
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant={chain.intact === true ? "secondary" : "destructive"}>
            {checked === null
              ? m.admin_audit_chain_never()
              : chain.intact === true
                ? m.admin_audit_chain_intact()
                : m.admin_audit_chain_broken()}
          </Badge>
          {checked !== null && (
            <span className="text-muted-foreground" suppressHydrationWarning>
              {m.admin_audit_chain_checked_at({ date: checked })}
            </span>
          )}
          {chain.entries != null && (
            <span className="text-muted-foreground">
              {m.admin_audit_chain_entries({ count: chain.entries })}
            </span>
          )}
        </div>
        {chain.broken_at != null && (
          <span className="font-medium text-destructive">
            {m.admin_audit_chain_broken_at({ id: chain.broken_at })}
          </span>
        )}
        {chain.intact === false && (
          <span className="text-muted-foreground" suppressHydrationWarning>
            {m.admin_audit_chain_last_ok({
              date: formatDateTime(chain.last_intact_at) ?? "-",
            })}
          </span>
        )}
        <p className="text-muted-foreground">{m.admin_audit_chain_hint()}</p>
      </div>
    </Panel>
  )
}

/**
 * FR-1902, W-07: список пользователей, роли и жизненный цикл записи.
 *
 * Роли, сброс пароля и отключение стоят в одной таблице не для компактности:
 * уволившемуся снимают роли и отключают запись одним движением, и разнеси эти
 * действия по разным экранам - второе забудут. Все три пишутся в аудит.
 */
function UsersPanel() {
  const queryClient = useQueryClient()
  const { data: page } = useQuery(usersQuery)
  const { data: me } = useQuery(meQuery)
  const [role, setRole] = useState<Record<string, GrantableRole>>({})
  // Выданный одноразовый пароль живет только в состоянии страницы: ни в
  // кеш запросов, ни в хранилище браузера он не кладется - перезагрузка
  // его теряет, и это правильно
  const [issued, setIssued] = useState<{ user: string; password: string }>()

  const refresh = () => queryClient.invalidateQueries({ queryKey: ["admin"] })

  const grant = useMutation({
    mutationFn: ({ userId, next }: { userId: string; next: GrantableRole }) =>
      grantRole(userId, next),
    onSuccess: refresh,
  })
  // Снятие роли и отключение записи молчали: экран перерисовывался, и было
  // непонятно, прошло ли действие. Обе мутации пишутся в аудит - тем более
  // они обязаны отчитаться
  const revoke = useMutation({
    mutationFn: ({ userId, next }: { userId: string; next: string }) =>
      revokeRole(userId, next),
    onSuccess: async () => {
      notifySuccess(m.admin_role_revoked_toast())
      await refresh()
    },
    onError: (error: unknown) => notifyError(problemMessage(error)),
  })
  const toggle = useMutation({
    mutationFn: ({ userId, next }: { userId: string; next: boolean }) =>
      setUserActive(userId, next),
    onSuccess: async (_data, variables) => {
      notifySuccess(
        variables.next
          ? m.admin_user_activated_toast()
          : m.admin_user_deactivated_toast()
      )
      await refresh()
    },
    onError: (error: unknown) => notifyError(problemMessage(error)),
  })
  const reset = useMutation({
    mutationFn: (user: UserDto) => resetPassword(user.id),
    onSuccess: (password, user) =>
      setIssued({ user: user.full_name, password }),
  })

  const chosen = (user: UserDto): GrantableRole =>
    role[user.id] ?? GRANTABLE_ROLES[0]

  return (
    <section aria-labelledby="admin-users" className="flex flex-col gap-3">
      <h2 id="admin-users" className="font-heading text-lg font-semibold">
        {m.admin_users_title()}
      </h2>
      <p className="text-sm text-muted-foreground">{m.admin_users_hint()}</p>
      <p className="text-sm text-muted-foreground">
        {m.admin_users_lifecycle_hint()}
      </p>
      {issued !== undefined && (
        <div
          role="status"
          data-testid="admin-issued-password"
          className="flex flex-col gap-1.5 rounded-lg border p-3 text-sm"
        >
          <span>
            {m.admin_reset_password_result({
              user: issued.user,
              password: issued.password,
            })}
          </span>
          <span className="text-muted-foreground">
            {m.admin_reset_password_warning()}
          </span>
        </div>
      )}
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
                <TableHead scope="col">{m.admin_user_state()}</TableHead>
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
                            {/* Снятие роли отнимает доступ к кабинету и
                                пишется в аудит - крестик срабатывал
                                с первого промаха мышью */}
                            <ConfirmAction
                              title={m.admin_revoke_confirm_title()}
                              description={m.admin_revoke_confirm_description({
                                role: roleLabel(granted),
                                user: user.full_name,
                              })}
                              confirmLabel={m.admin_revoke_role()}
                              onConfirm={() =>
                                revoke.mutate({
                                  userId: user.id,
                                  next: granted,
                                })
                              }
                              trigger={
                                <button
                                  type="button"
                                  aria-label={`${m.admin_revoke_role()}: ${roleLabel(granted)}`}
                                  className="text-muted-foreground hover:text-destructive"
                                >
                                  ×
                                </button>
                              }
                            />
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
                  <TableCell>
                    <div className="flex flex-wrap items-center gap-2">
                      <Badge
                        variant={user.is_active ? "secondary" : "destructive"}
                      >
                        {user.is_active
                          ? m.admin_user_active()
                          : m.admin_user_disabled()}
                      </Badge>
                      {/*
                        Отключить себя нельзя: вернуть запись было бы некому,
                        а других админов может не быть. Сервер это тоже
                        отвергает - кнопка лишь не предлагает тупика.
                      */}
                      {user.is_active ? (
                        // Отключение отнимает вход целиком - подтверждается
                        <ConfirmAction
                          title={m.admin_deactivate_confirm_title()}
                          description={m.admin_deactivate_confirm_description({
                            user: user.full_name,
                          })}
                          confirmLabel={m.admin_deactivate()}
                          disabled={toggle.isPending || user.id === me?.id}
                          onConfirm={() =>
                            toggle.mutate({ userId: user.id, next: false })
                          }
                          trigger={
                            <Button
                              variant="outline"
                              size="sm"
                              data-testid={`toggle-active-${user.id}`}
                            >
                              {m.admin_deactivate()}
                            </Button>
                          }
                        />
                      ) : (
                        <Button
                          variant="outline"
                          size="sm"
                          data-testid={`toggle-active-${user.id}`}
                          disabled={toggle.isPending}
                          onClick={() =>
                            toggle.mutate({ userId: user.id, next: true })
                          }
                        >
                          {m.admin_activate()}
                        </Button>
                      )}
                      <Button
                        variant="outline"
                        size="sm"
                        data-testid={`reset-password-${user.id}`}
                        disabled={reset.isPending}
                        onClick={() => reset.mutate(user)}
                      >
                        {m.admin_reset_password()}
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
              {/* `amount` - строка Decimal («9999.00»): склеенная руками,
                  она печаталась без разрядных пробелов и с точкой вместо
                  запятой, мимо Intl и мимо локали */}
              {entry.year}: {formatTenge(entry.amount)}
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
