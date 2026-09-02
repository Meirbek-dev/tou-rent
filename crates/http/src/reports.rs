//! Отчетность (арх. § 9, контур 3): реестры решений, договоров и поступлений
//! с выгрузкой CSV.
//!
//! Требования в ТЗ нет (Q-012), поэтому реестр отдает то, что система уже
//! записала: колонки, строки и период - без придуманных форм, реквизитов
//! и подписей (A-079). Один контракт на все три реестра: колонки приходят
//! с сервера, кабинет рисует таблицу и ссылку на выгрузку.

use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::{Date, OffsetDateTime};
use tou_db::RowCursor;
use tou_db::reports::{self, Period, RegistryRow};
use tou_domain::policy::{Action, Compound};
use tou_domain::report::{Registry, to_csv};
use utoipa::{IntoParams, ToSchema};

use crate::announcement::format_decimal_ru;
use crate::dto::cursor;
use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::request::{Json, Path, Query};
use crate::state::AppState;

/// Реестр отчетности на проводе: перечень закрыт (арх. § 9).
///
/// Тип, а не строка: клиенту контракта нужен перечень допустимых значений,
/// иначе `/reports/{registry}` выглядит как «сюда можно любое слово», и
/// опечатка ловится только 422 в бою. Значения - те же, что у
/// [`Registry::as_str`]: провод не меняется.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReportRegistryDto {
    /// Решения Правления: особый порядок (п. 90) и земельные участки (п. 106)
    Decisions,
    /// Договоры найма всех оснований (п. 126)
    Contracts,
    /// Поступления депозитной книги (FR-1001)
    Receipts,
}

impl From<Registry> for ReportRegistryDto {
    fn from(registry: Registry) -> Self {
        match registry {
            Registry::Decisions => Self::Decisions,
            Registry::Contracts => Self::Contracts,
            Registry::Receipts => Self::Receipts,
        }
    }
}

/// Колонка реестра: машинный ключ и шапка на трех языках (NFR-01).
///
/// Ключ нужен кабинету как идентификатор колонки - раньше им служил русский
/// заголовок, и стоило перевести шапку, как ключ строки менялся вместе с
/// языком.
#[derive(Debug, Serialize, ToSchema)]
pub struct RegistryColumnDto {
    pub key: String,
    pub title_ru: String,
    pub title_kk: String,
    pub title_en: String,
}

/// Ячейка реестра (арх. § 9).
///
/// Значение приходит либо подписью на трех языках (закрытые перечни -
/// решение, состояние договора, вид счета), либо как есть в `text_ru`
/// (имена, номера, суммы - они одинаковы во всех локалях), либо моментом
/// времени: сервер отдает ISO 8601, а кабинет печатает его своим
/// `formatDateTime` - в зоне отображения и с подписью зоны (NFR-03).
/// Сервер не печатает даты сам потому, что печатал их по-русски и в чужой
/// зоне: на `/en/` реестр был единственным экраном с «08.08.2026» без
/// признака пояса.
#[derive(Debug, Serialize, ToSchema)]
pub struct RegistryCellDto {
    /// Значение по-русски; оно же уходит в выгрузку CSV (делопроизводство)
    pub text_ru: String,
    /// Казахский вариант; пусто - кабинет откатывается на `text_ru`
    pub text_kk: String,
    /// Английский вариант; пусто - кабинет откатывается на `text_ru`
    pub text_en: String,
    /// Моменты ячейки (ISO 8601): один - дата события, два - границы
    /// периода. Пусто - ячейка не про время.
    pub timestamps: Vec<String>,
}

/// Подпись значения из закрытого перечня: ru, kk, en.
type Label = (&'static str, &'static str, &'static str);

/// Значения нет - прочерк одинаков во всех локалях.
const DASH: Label = ("-", "-", "-");

impl RegistryCellDto {
    /// Значение, одинаковое во всех локалях: имя, номер, сумма.
    fn plain(text: String) -> Self {
        Self {
            text_ru: text,
            text_kk: String::new(),
            text_en: String::new(),
            timestamps: Vec::new(),
        }
    }

    /// Значение из закрытого перечня.
    fn label((ru, kk, en): Label) -> Self {
        Self {
            text_ru: ru.to_owned(),
            text_kk: kk.to_owned(),
            text_en: en.to_owned(),
            timestamps: Vec::new(),
        }
    }

    /// Момент события.
    fn moment(value: Option<OffsetDateTime>) -> Self {
        Self {
            text_ru: crate::admission::format_almaty(value),
            text_kk: String::new(),
            text_en: String::new(),
            timestamps: value.and_then(iso8601).into_iter().collect(),
        }
    }

    /// Период: обе границы или прочерк - половина периода бессмысленна.
    fn period(from: Option<OffsetDateTime>, to: Option<OffsetDateTime>) -> Self {
        let (Some(from), Some(to)) = (from, to) else {
            return Self::label(DASH);
        };
        Self {
            text_ru: format!(
                "{} - {}",
                crate::admission::format_almaty(Some(from)),
                crate::admission::format_almaty(Some(to))
            ),
            text_kk: String::new(),
            text_en: String::new(),
            timestamps: [from, to].into_iter().filter_map(iso8601).collect(),
        }
    }

    /// Строка выгрузки: CSV ведет делопроизводство и остается русским.
    fn csv(&self) -> String {
        self.text_ru.clone()
    }
}

/// Момент на проводе - ISO 8601. Отказ форматирования не выдумывает дату:
/// ячейка остается с русским текстом, и кабинет покажет его.
fn iso8601(value: OffsetDateTime) -> Option<String> {
    value.format(&Rfc3339).ok()
}

/// Реестр отчетности: колонки и строки в одном порядке (арх. § 9).
#[derive(Debug, Serialize, ToSchema)]
pub struct RegistryDto {
    pub registry: ReportRegistryDto,
    pub title_ru: String,
    pub title_kk: String,
    pub title_en: String,
    pub columns: Vec<RegistryColumnDto>,
    /// Строки реестра в порядке колонок
    pub rows: Vec<Vec<RegistryCellDto>>,
    /// Курсор продолжения; `null` - реестр показан до конца
    pub next_after: Option<String>,
    /// Показана не вся выборка: за строками ответа есть еще
    pub truncated: bool,
}

fn parse_period(from: Option<&str>, to: Option<&str>) -> Result<Period, ApiError> {
    let parse = |value: Option<&str>| -> Result<Option<Date>, ApiError> {
        match value.filter(|raw| !raw.is_empty()) {
            None => Ok(None),
            Some(raw) => Date::parse(raw, &time::format_description::well_known::Iso8601::DATE)
                .map(Some)
                .map_err(|_| ApiError::Validation(format!("дата «{raw}» - ISO 8601"))),
        }
    };
    Ok(Period {
        from: parse(from)?,
        to: parse(to)?,
    })
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct PeriodParams {
    /// Начало периода включительно (ISO 8601)
    pub from: Option<String>,
    /// Конец периода включительно (ISO 8601)
    pub to: Option<String>,
}

impl PeriodParams {
    fn period(&self) -> Result<Period, ApiError> {
        parse_period(self.from.as_deref(), self.to.as_deref())
    }
}

/// Параметры экрана реестра: период плюс страница (ТЗ § 7).
#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct RegistryParams {
    /// Начало периода включительно (ISO 8601)
    pub from: Option<String>,
    /// Конец периода включительно (ISO 8601)
    pub to: Option<String>,
    /// Курсор следующей страницы - значение `next_after` предыдущей
    pub after: Option<String>,
    pub limit: Option<i64>,
}

impl RegistryParams {
    fn period(&self) -> Result<Period, ApiError> {
        parse_period(self.from.as_deref(), self.to.as_deref())
    }
}

/// Реестр видят те, кто его ведет: решения - Правление и подразделение,
/// договоры - организатор и финансы, поступления - финансы.
///
/// Реестр не отдельная область ответственности, а срез трех уже разведенных:
/// поэтому права на него нет, а есть право на саму область (INV-POL-01).
fn require_access(user: &CurrentUser, registry: Registry) -> Result<(), ApiError> {
    match registry {
        Registry::Decisions => user.require_any(Compound::SPECIAL_DECISION_ACCESS),
        Registry::Contracts => user.require_any(Compound::CONTRACT_REGISTRY_READ),
        Registry::Receipts => user.require(Action::LedgerRead),
    }
}

/// Страница реестра до превращения в DTO: строки уже собраны в ячейки,
/// а курсор еще жив - после `map` в ячейки его взять неоткуда.
struct RegistryPage {
    rows: Vec<Vec<RegistryCellDto>>,
    next: Option<RowCursor>,
    truncated: bool,
}

/// Курсор последней строки страницы и признак усечения - одинаково для
/// всех трех реестров, у которых строки разного типа.
fn page_tail<T: RegistryRow>(page: &tou_db::Page<T>) -> (Option<RowCursor>, bool) {
    (page.last().map(RegistryRow::cursor), page.truncated)
}

async fn build(
    state: &AppState,
    registry: Registry,
    period: Period,
    after: Option<RowCursor>,
    limit: i64,
) -> Result<RegistryPage, ApiError> {
    let mut conn = state.db.acquire().await?;

    let (rows, next, truncated) = match registry {
        Registry::Decisions => {
            let page = reports::decisions_page(&mut conn, period, after, limit).await?;
            let (next, truncated) = page_tail(&page);
            let rows = page
                .into_iter()
                .map(|row| {
                    vec![
                        RegistryCellDto::moment(Some(row.decided_at)),
                        RegistryCellDto::label(order_kind(&row.order_kind)),
                        RegistryCellDto::plain(row.subject),
                        optional(row.applicant),
                        RegistryCellDto::label(decision(&row.decision)),
                        RegistryCellDto::plain(row.rationale),
                    ]
                })
                .collect();
            (rows, next, truncated)
        }
        Registry::Contracts => {
            let page = reports::contracts_page(&mut conn, period, after, limit).await?;
            let (next, truncated) = page_tail(&page);
            let rows = page
                .into_iter()
                .map(|row| {
                    vec![
                        // «б/н» - не перечень, а отсутствие номера: одинаково
                        // читается и в выгрузке, и на экране
                        RegistryCellDto::plain(row.reg_number.unwrap_or_else(|| "б/н".to_owned())),
                        RegistryCellDto::moment(row.registered_at),
                        RegistryCellDto::plain(row.object_name),
                        optional(row.tenant_name),
                        RegistryCellDto::plain(format_decimal_ru(row.monthly_rate)),
                        RegistryCellDto::period(row.lease_from, row.lease_to),
                        RegistryCellDto::label(contract_status(&row.status)),
                        RegistryCellDto::label(source(&row.source)),
                    ]
                })
                .collect();
            (rows, next, truncated)
        }
        Registry::Receipts => {
            let page = reports::receipts_page(&mut conn, period, after, limit).await?;
            let (next, truncated) = page_tail(&page);
            let rows = page
                .into_iter()
                .map(|row| {
                    vec![
                        RegistryCellDto::moment(Some(row.occurred_at)),
                        RegistryCellDto::label(account_kind(&row.account_kind)),
                        optional(row.payer),
                        RegistryCellDto::plain(format_decimal_ru(row.amount)),
                        optional(row.rule_ref),
                        optional(row.recorded_by),
                    ]
                })
                .collect();
            (rows, next, truncated)
        }
    };

    Ok(RegistryPage {
        rows,
        next,
        truncated,
    })
}

/// Весь реестр за период - страница за страницей до конца выборки.
///
/// Выгрузка - это документ, а не экран: обрезанный на тысяче строк CSV и
/// есть та ошибка, ради которой признак усечения появился, - в нем нехватку
/// записей заметят последней. Цикл конечен по построению: ключ курсора
/// строго убывает, поэтому каждый следующий запрос берет строки, которых
/// в предыдущих не было, и упирается в конец выборки.
async fn build_all(
    state: &AppState,
    registry: Registry,
    period: Period,
) -> Result<Vec<Vec<String>>, ApiError> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut after = None;

    loop {
        let page = build(state, registry, period, after, tou_db::MAX_ROWS).await?;
        rows.extend(
            page.rows
                .iter()
                .map(|row| row.iter().map(RegistryCellDto::csv).collect()),
        );
        match (page.truncated, page.next) {
            (true, Some(cursor)) => after = Some(cursor),
            _ => return Ok(rows),
        }
    }
}

/// Необязательное значение БД: пусто - прочерк, а не пустая ячейка.
fn optional(value: Option<String>) -> RegistryCellDto {
    match value {
        Some(value) => RegistryCellDto::plain(value),
        None => RegistryCellDto::label(DASH),
    }
}

// Подписи закрытых перечней реестра. Переводы kk/en - черновые (регламент
// А.5): они пришли не из Правил, а из смысла кода, и правятся здесь же, без
// пересборки фронта. Paraglide тут не при чем - перечень ведет сервер, как и
// прочие справочники (`serverLabel`).

fn order_kind(kind: &str) -> Label {
    match kind {
        // draft kk/en
        "special" => (
            "особый порядок (п. 90)",
            "ерекше тәртіп (90-т.)",
            "special procedure (cl. 90)",
        ),
        // draft kk/en
        "land" => (
            "земельный участок (п. 106)",
            "жер учаскесі (106-т.)",
            "land plot (cl. 106)",
        ),
        _ => DASH,
    }
}

fn decision(decision: &str) -> Label {
    match decision {
        // draft kk/en
        "grant" => ("предоставить", "беру", "grant"),
        // draft kk/en
        "refuse" => ("отказать", "бас тарту", "refuse"),
        // draft kk/en
        "redirect" => (
            "направить в общий порядок",
            "жалпы тәртіпке жіберу",
            "redirect to the general procedure",
        ),
        _ => DASH,
    }
}

fn contract_status(status: &str) -> Label {
    match status {
        // draft kk/en
        "draft" => ("проект", "жоба", "draft"),
        // draft kk/en
        "signing" => ("на подписании", "қол қоюда", "signing"),
        // draft kk/en
        "active" => ("действует", "қолданыста", "active"),
        // draft kk/en
        "completed" => ("исполнен", "орындалды", "completed"),
        // draft kk/en
        "terminated" => ("прекращен", "тоқтатылды", "terminated"),
        // draft kk/en
        "cancelled" => ("отменен", "күші жойылды", "cancelled"),
        _ => DASH,
    }
}

fn source(source: &str) -> Label {
    match source {
        // draft kk/en
        "tender" => ("тендер", "тендер", "tender"),
        // draft kk/en
        "special" => ("особый порядок", "ерекше тәртіп", "special procedure"),
        // draft kk/en
        "land" => ("земельный участок", "жер учаскесі", "land plot"),
        // draft kk/en
        _ => ("иное", "өзге", "other"),
    }
}

fn account_kind(kind: &str) -> Label {
    match kind {
        // draft kk/en
        "participant_fee" => ("гарантийный взнос", "кепілдік жарна", "participation fee"),
        // draft kk/en
        "contract_deposit" => (
            "депозит по договору",
            "шарт бойынша депозит",
            "contract deposit",
        ),
        _ => DASH,
    }
}

/// Название реестра на трех языках. Русское берется из домена (оно же в
/// шапке выгрузки), kk/en - черновые переводы рядом с прочими подписями.
fn registry_title(registry: Registry) -> Label {
    match registry {
        // draft kk/en
        Registry::Decisions => (
            registry.title_ru(),
            "Шешімдер тізілімі",
            "Decisions register",
        ),
        // draft kk/en
        Registry::Contracts => (
            registry.title_ru(),
            "Шарттар тізілімі",
            "Contracts register",
        ),
        // draft kk/en
        Registry::Receipts => (
            registry.title_ru(),
            "Түсімдер тізілімі",
            "Receipts register",
        ),
    }
}

/// Ключ колонки и перевод шапки. Порядок - тот же, что у
/// [`Registry::columns`]: русские заголовки живут в домене (они же шапка
/// CSV), здесь - только то, чего домену знать незачем. Согласованность
/// проверяет тест `columns_match_the_domain_header`.
fn column_keys(registry: Registry) -> &'static [(&'static str, &'static str, &'static str)] {
    match registry {
        Registry::Decisions => &[
            // draft kk/en
            ("decided_at", "Шешім күні", "Decision date"),
            ("order_kind", "Тәртіп", "Procedure"),
            ("subject", "Мәні", "Subject"),
            ("applicant", "Өтініш беруші", "Applicant"),
            ("decision", "Шешім", "Decision"),
            ("rationale", "Негіздеме", "Rationale"),
        ],
        Registry::Contracts => &[
            // draft kk/en
            ("reg_number", "Тіркеу нөмірі", "Reg. number"),
            ("registered_at", "Тіркелген күні", "Registration date"),
            ("object_name", "Нысан", "Property"),
            ("tenant_name", "Жалдаушы", "Tenant"),
            ("monthly_rate", "Айлық мөлшерлеме, ₸", "Monthly rate, ₸"),
            ("lease_period", "Жалдау мерзімі", "Lease period"),
            ("status", "Күйі", "Status"),
            ("source", "Негізі", "Source"),
        ],
        Registry::Receipts => &[
            // draft kk/en
            ("occurred_at", "Күні", "Date"),
            ("account_kind", "Шот", "Account"),
            ("payer", "Төлеуші", "Payer"),
            ("amount", "Сомасы, ₸", "Amount, ₸"),
            ("rule_ref", "Негізі", "Basis"),
            ("recorded_by", "Растаған", "Confirmed by"),
        ],
    }
}

/// Шапка реестра для кабинета: ключ плюс три языка.
fn columns(registry: Registry) -> Vec<RegistryColumnDto> {
    registry
        .columns()
        .iter()
        .zip(column_keys(registry))
        .map(|(title_ru, (key, title_kk, title_en))| RegistryColumnDto {
            key: (*key).to_owned(),
            title_ru: (*title_ru).to_owned(),
            title_kk: (*title_kk).to_owned(),
            title_en: (*title_en).to_owned(),
        })
        .collect()
}

/// Реестр отчетности за период (арх. § 9): колонки и строки.
#[utoipa::path(
    get,
    path = "/api/v1/reports/{registry}",
    tag = "reports",
    params(
        ("registry" = ReportRegistryDto, Path, description = "Реестр отчетности"),
        RegistryParams
    ),
    responses(
        (status = 200, description = "Страница реестра", body = RegistryDto),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
        (status = 422, description = "Неизвестный реестр, дата или курсор",
         body = crate::error::Problem),
    )
)]
pub async fn registry(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(registry): Path<String>,
    Query(params): Query<RegistryParams>,
) -> Result<Json<RegistryDto>, ApiError> {
    let registry: Registry = registry
        .parse()
        .map_err(|_| ApiError::Validation(format!("неизвестный реестр: {registry}")))?;
    require_access(&user, registry)?;

    let after = params.after.as_deref().map(cursor::parse).transpose()?;
    let page = build(
        &state,
        registry,
        params.period()?,
        after,
        crate::page_limit(params.limit),
    )
    .await?;

    let (title_ru, title_kk, title_en) = registry_title(registry);
    Ok(Json(RegistryDto {
        registry: registry.into(),
        title_ru: title_ru.to_owned(),
        title_kk: title_kk.to_owned(),
        title_en: title_en.to_owned(),
        columns: columns(registry),
        rows: page.rows,
        next_after: cursor::next(page.truncated, page.next),
        truncated: page.truncated,
    }))
}

/// Выгрузка реестра (арх. § 9): CSV с шапкой из тех же колонок.
#[utoipa::path(
    get,
    path = "/api/v1/reports/{registry}/export.csv",
    tag = "reports",
    params(
        ("registry" = ReportRegistryDto, Path, description = "Реестр отчетности"),
        PeriodParams
    ),
    responses(
        (status = 200, description = "Выгрузка реестра", content_type = "text/csv"),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
        (status = 422, description = "Неизвестный реестр или дата периода",
         body = crate::error::Problem),
    )
)]
pub async fn registry_csv(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(registry): Path<String>,
    Query(params): Query<PeriodParams>,
) -> Result<Response, ApiError> {
    let registry: Registry = registry
        .parse()
        .map_err(|_| ApiError::Validation(format!("неизвестный реестр: {registry}")))?;
    require_access(&user, registry)?;

    let rows = build_all(&state, registry, params.period()?).await?;
    let csv = to_csv(registry, &rows);

    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", registry.file_name()),
            ),
        ],
        csv,
    )
        .into_response())
}

/// Перечень реестров (арх. § 9): кабинет показывает те, что доступны роли.
#[utoipa::path(
    get,
    path = "/api/v1/reports",
    tag = "reports",
    responses((status = 200, description = "Доступные реестры", body = [RegistrySummaryDto]))
)]
pub async fn list_registries(user: CurrentUser) -> Result<Json<Vec<RegistrySummaryDto>>, ApiError> {
    Ok(Json(
        Registry::ALL
            .into_iter()
            .filter(|registry| require_access(&user, *registry).is_ok())
            .map(|registry| {
                let (title_ru, title_kk, title_en) = registry_title(registry);
                RegistrySummaryDto {
                    registry: registry.into(),
                    title_ru: title_ru.to_owned(),
                    title_kk: title_kk.to_owned(),
                    title_en: title_en.to_owned(),
                }
            })
            .collect(),
    ))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RegistrySummaryDto {
    pub registry: ReportRegistryDto,
    pub title_ru: String,
    pub title_kk: String,
    pub title_en: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ключи и переводы шапки идут в том же порядке, что русские заголовки
    /// домена: они собираются `zip`, и лишний ключ молча отбросил бы колонку.
    #[test]
    fn columns_match_the_domain_header() {
        let mut keys = std::collections::BTreeSet::new();
        for registry in Registry::ALL {
            let header = registry.columns();
            let extra = column_keys(registry);
            assert_eq!(
                header.len(),
                extra.len(),
                "{registry:?}: шапка домена и перевод разошлись"
            );
            let dto = columns(registry);
            assert_eq!(dto.len(), header.len());
            for (column, title_ru) in dto.iter().zip(header) {
                assert_eq!(&column.title_ru, title_ru);
                assert!(!column.title_kk.is_empty(), "{}: нет kk", column.key);
                assert!(!column.title_en.is_empty(), "{}: нет en", column.key);
                assert!(
                    keys.insert(format!("{registry:?}.{}", column.key)),
                    "{}: ключ колонки повторяется",
                    column.key
                );
            }
        }
    }

    /// Провод реестра не меняется: значение перечня контракта - то же слово,
    /// что понимает [`Registry::from_str`].
    #[test]
    fn registry_wire_values_are_unchanged() {
        for registry in Registry::ALL {
            let dto: ReportRegistryDto = registry.into();
            let wire = serde_json::to_string(&dto).expect("сериализация перечня");
            assert_eq!(wire, format!("\"{}\"", registry.as_str()));
            assert_eq!(
                registry.as_str().parse::<Registry>().expect("разбор"),
                registry
            );
        }
        let (title_ru, title_kk, title_en) = registry_title(Registry::Decisions);
        assert_eq!(title_ru, Registry::Decisions.title_ru());
        assert!(!title_kk.is_empty() && !title_en.is_empty());
    }

    /// Ячейка-дата несет ISO 8601 кабинету и русский текст выгрузке: экран
    /// печатает время сам (NFR-03), а CSV остается прежним.
    #[test]
    fn moment_cell_carries_iso_and_keeps_the_csv_text() {
        let at = OffsetDateTime::from_unix_timestamp(1_785_312_000).expect("момент");
        let cell = RegistryCellDto::moment(Some(at));
        assert_eq!(cell.timestamps, vec![at.format(&Rfc3339).expect("iso")]);
        assert_eq!(cell.csv(), crate::admission::format_almaty(Some(at)));

        let empty = RegistryCellDto::moment(None);
        assert!(empty.timestamps.is_empty());
        assert_eq!(empty.csv(), "-");

        let period = RegistryCellDto::period(Some(at), Some(at));
        assert_eq!(period.timestamps.len(), 2, "период - две границы");
        assert_eq!(RegistryCellDto::period(Some(at), None).timestamps.len(), 0);
    }

    /// Перечни ячеек переведены: на `/en/` реестр решений печатал
    /// «предоставить» и «особый порядок (п. 90)» (G-009).
    #[test]
    fn enumerated_cells_carry_three_languages() {
        for cell in [
            RegistryCellDto::label(order_kind("special")),
            RegistryCellDto::label(decision("grant")),
            RegistryCellDto::label(contract_status("active")),
            RegistryCellDto::label(source("tender")),
            RegistryCellDto::label(account_kind("participant_fee")),
        ] {
            assert!(!cell.text_ru.is_empty());
            assert!(!cell.text_kk.is_empty(), "{}: нет kk", cell.text_ru);
            assert!(!cell.text_en.is_empty(), "{}: нет en", cell.text_ru);
        }
        // Незнакомое значение - прочерк, а не пустая ячейка
        assert_eq!(RegistryCellDto::label(decision("unknown")).text_en, "-");
        assert_eq!(optional(None).text_ru, "-");
    }
}
