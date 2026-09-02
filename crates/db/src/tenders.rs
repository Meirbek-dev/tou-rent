//! Тендеры и лоты (М3): `core.tenders` / `core.lots`.
//!
//! Переходы статусов выполняются обычным UPDATE - их законность охраняет
//! триггер INV-021 (и правило 10 дней FR-303); нарушение возвращается
//! как [`TransitionError::Rejected`] с текстом причины из БД.

use rust_decimal::Decimal;
use time::OffsetDateTime;
use tou_domain::rule::RuleRejection;
use uuid::Uuid;

use crate::Db;

#[derive(Debug, Clone)]
pub struct TenderRecord {
    pub id: Uuid,
    pub status: String,
    pub title: String,
    pub title_kk: String,
    pub organizer_id: Uuid,
    pub announced_at: Option<OffsetDateTime>,
    pub submission_deadline: Option<OffsetDateTime>,
    pub opening_at: Option<OffsetDateTime>,
    pub opened_at: Option<OffsetDateTime>,
    pub trading_at: Option<OffsetDateTime>,
    pub zoom_url: Option<String>,
    pub zoom_recording_url: Option<String>,
    pub repeat_of: Option<Uuid>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct LotRecord {
    pub id: Uuid,
    pub tender_id: Uuid,
    pub seq: i32,
    pub object_id: Uuid,
    pub purpose: String,
    pub purpose_kk: String,
    pub lease_months: i32,
    pub base_rate_monthly: Decimal,
    pub guarantee_fee: Decimal,
    pub rate_calculation: serde_json::Value,
    pub viewing_terms: Option<String>,
    /// Единица ставки (FR-205): `monthly` - за месяц, `hourly` - за час
    pub rate_unit: String,
    /// Объем разыгрываемых часов почасового лота (п. 97)
    pub hours_total: Option<i32>,
    /// Лот отменен (FR-305, п. 78): объект освобожден, взносы на возврате
    pub cancelled_at: Option<OffsetDateTime>,
    pub cancel_reason: Option<String>,
}

/// Выборка тендера: общий список столбцов + хвост запроса (см. `acts.rs`).
///
/// `!` у `status` - это `::text`, который планировщик считает потенциально
/// NULL, хотя столбец NOT NULL.
macro_rules! tender_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            TenderRecord,
            r#"SELECT id, status::text AS "status!", title, title_kk, organizer_id,
                      announced_at, submission_deadline, opening_at, opened_at,
                      trading_at, zoom_url, zoom_recording_url, repeat_of,
                      created_at, updated_at
               FROM core.tenders"# + $tail
            $(, $arg)*
        )
    };
}
/// То же для `RETURNING`: столбцы идут в конце запроса, поэтому макрос
/// принимает не хвост, а голову (см. `identities.rs`).
macro_rules! tender_query_returning {
    ($head:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            TenderRecord,
            $head + r#" RETURNING id, status::text AS "status!", title, title_kk, organizer_id,
                                  announced_at, submission_deadline, opening_at, opened_at,
                                  trading_at, zoom_url, zoom_recording_url, repeat_of,
                                  created_at, updated_at"#
            $(, $arg)*
        )
    };
}

/// Выборка лота: `!` у `rate_unit` - по той же причине, что у `status`.
macro_rules! lot_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            LotRecord,
            r#"SELECT id, tender_id, seq, object_id, purpose, purpose_kk, lease_months,
                      base_rate_monthly, guarantee_fee, rate_calculation,
                      viewing_terms, rate_unit::text AS "rate_unit!", hours_total,
                      cancelled_at, cancel_reason
               FROM core.lots"# + $tail
            $(, $arg)*
        )
    };
}
macro_rules! lot_query_returning {
    ($head:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            LotRecord,
            $head + r#" RETURNING id, tender_id, seq, object_id, purpose, purpose_kk, lease_months,
                                  base_rate_monthly, guarantee_fee, rate_calculation,
                                  viewing_terms, rate_unit::text AS "rate_unit!", hours_total,
                                  cancelled_at, cancel_reason"#
            $(, $arg)*
        )
    };
}

pub async fn get(db: &Db, id: Uuid) -> Result<Option<TenderRecord>, sqlx::Error> {
    tender_query!(" WHERE id = $1", id).fetch_optional(db).await
}

/// Реестр тендеров. `public_only` - только опубликованные (для guest, п. 5–6).
pub async fn list(
    db: &Db,
    after: Option<Uuid>,
    limit: i64,
    public_only: bool,
) -> Result<Vec<TenderRecord>, sqlx::Error> {
    // Свежие объявления сверху: id - uuid v7, монотонен по времени создания,
    // поэтому курсор идет «вниз» (id < after). Реестр, начинающийся с самых
    // старых тендеров, прятал бы новую публикацию на последней странице.
    tender_query!(
        " WHERE ($1::uuid IS NULL OR id < $1)
            AND (NOT $3 OR status <> 'draft')
          ORDER BY id DESC LIMIT $2",
        after,
        limit,
        public_only
    )
    .fetch_all(db)
    .await
}

pub async fn lots_of(db: &Db, tender_id: Uuid) -> Result<Vec<LotRecord>, sqlx::Error> {
    lot_query!(" WHERE tender_id = $1 ORDER BY seq", tender_id)
        .fetch_all(db)
        .await
}

/// Лоты страницы тендеров одним запросом (реестр без N+1).
pub async fn lots_for(db: &Db, tender_ids: &[Uuid]) -> Result<Vec<LotRecord>, sqlx::Error> {
    lot_query!(
        " WHERE tender_id = ANY($1) ORDER BY tender_id, seq",
        tender_ids
    )
    .fetch_all(db)
    .await
}

/// Снимок ставки (FR-202) уже посчитан вызывающей стороной из refdata + domain.
pub struct NewLot<'a> {
    pub object_id: Uuid,
    pub purpose: &'a str,
    pub purpose_kk: &'a str,
    pub lease_months: i32,
    pub base_rate_monthly: Decimal,
    pub guarantee_fee: Decimal,
    pub rate_calculation: &'a serde_json::Value,
    pub viewing_terms: Option<&'a str>,
    /// FR-205: `monthly` - ставка за месяц, `hourly` - за час (п. 97)
    pub rate_unit: &'a str,
    /// Объем разыгрываемых часов почасового лота; взнос считает БД (FR-206)
    pub hours_total: Option<i32>,
}

/// Тендер и все его лоты - одна транзакция (FR-301): либо создается целиком,
/// либо ничего. Снимки ставок в лотах уже посчитаны вызывающей стороной.
pub async fn create(
    db: &Db,
    actor: Uuid,
    title: &str,
    title_kk: &str,
    organizer_id: Uuid,
    lots: &[NewLot<'_>],
) -> Result<(TenderRecord, Vec<LotRecord>), sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        let tender = tender_query_returning!(
            "INSERT INTO core.tenders (title, title_kk, organizer_id) VALUES ($1, $2, $3)",
            title,
            title_kk,
            organizer_id
        )
        .fetch_one(&mut *tx)
        .await?;

        let mut created = Vec::with_capacity(lots.len());
        for (idx, lot) in lots.iter().enumerate() {
            // `$10::text::core.rate_unit`: единица приходит строкой, приведение
            // к перечислению делает БД
            let record = lot_query_returning!(
                "INSERT INTO core.lots (tender_id, seq, object_id, purpose, purpose_kk, lease_months,
                    base_rate_monthly, guarantee_fee, rate_calculation, viewing_terms,
                    rate_unit, hours_total)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11::text::core.rate_unit, $12)",
                tender.id,
                idx as i32 + 1,
                lot.object_id,
                lot.purpose,
                lot.purpose_kk,
                lot.lease_months,
                lot.base_rate_monthly,
                lot.guarantee_fee,
                lot.rate_calculation,
                lot.viewing_terms,
                lot.rate_unit,
                lot.hours_total
            )
            .fetch_one(&mut *tx)
            .await?;
            created.push(record);
        }
        Ok((tender, created))
    })
    .await
}

#[derive(Debug, Clone)]
pub struct TenderDocRecord {
    pub id: Uuid,
    pub tender_id: Uuid,
    pub version: i32,
    pub title: String,
    pub file_key: String,
    pub published_at: OffsetDateTime,
}

pub async fn documents(db: &Db, tender_id: Uuid) -> Result<Vec<TenderDocRecord>, sqlx::Error> {
    sqlx::query_as!(
        TenderDocRecord,
        "SELECT id, tender_id, version, title, file_key, published_at
         FROM core.tender_docs WHERE tender_id = $1
         ORDER BY version DESC, published_at DESC",
        tender_id
    )
    .fetch_all(db)
    .await
}

pub async fn documents_for(
    db: &Db,
    tender_ids: &[Uuid],
) -> Result<Vec<TenderDocRecord>, sqlx::Error> {
    sqlx::query_as!(
        TenderDocRecord,
        "SELECT id, tender_id, version, title, file_key, published_at
         FROM core.tender_docs WHERE tender_id = ANY($1)
         ORDER BY tender_id, version DESC, published_at DESC",
        tender_ids
    )
    .fetch_all(db)
    .await
}

pub async fn document(db: &Db, id: Uuid) -> Result<Option<TenderDocRecord>, sqlx::Error> {
    sqlx::query_as!(
        TenderDocRecord,
        "SELECT id, tender_id, version, title, file_key, published_at
         FROM core.tender_docs WHERE id = $1",
        id
    )
    .fetch_optional(db)
    .await
}

pub async fn add_document(
    db: &Db,
    actor: Uuid,
    tender_id: Uuid,
    title: &str,
    file_key: &str,
) -> Result<Option<TenderDocRecord>, sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        let draft = sqlx::query_scalar!(
            r#"SELECT status::text AS "status!" FROM core.tenders
               WHERE id = $1 FOR UPDATE"#,
            tender_id
        )
        .fetch_optional(&mut *tx)
        .await?;
        if draft.as_deref() != Some("draft") {
            return Ok(None);
        }
        sqlx::query_as!(
            TenderDocRecord,
            "INSERT INTO core.tender_docs (tender_id, version, title, file_key)
             SELECT $1, COALESCE(max(version), 0) + 1, $2, $3
             FROM core.tender_docs WHERE tender_id = $1
             RETURNING id, tender_id, version, title, file_key, published_at",
            tender_id,
            title,
            file_key
        )
        .fetch_optional(&mut *tx)
        .await
    })
    .await
}

/// Правка полей черновика (даты, ссылки; FR-301, FR-306).
/// Возвращает None, если тендер не найден или уже не черновик.
pub struct DraftFields<'a> {
    pub title: &'a str,
    pub title_kk: &'a str,
    pub submission_deadline: Option<OffsetDateTime>,
    pub opening_at: Option<OffsetDateTime>,
    pub trading_at: Option<OffsetDateTime>,
    pub zoom_url: Option<&'a str>,
}

/// Отказ CHECK на правке черновика - не поломка, а перепутанные даты:
/// `deadline_before_opening` ловит вскрытие раньше окончания приема заявок,
/// и организатору полагается объяснение, а не 500 (FR-303, п. 27).
pub async fn update_draft(
    db: &Db,
    actor: Uuid,
    id: Uuid,
    f: DraftFields<'_>,
) -> Result<Option<TenderRecord>, TransitionError> {
    crate::with_actor(db, actor, async |tx| update_draft_on(tx, id, f).await).await
}

/// То же в транзакции вызывающего - вариант `*_on` (арх. v3 § 6): тест
/// откатывает свои изменения, а не оставляет их на стенде.
pub async fn update_draft_on(
    tx: &mut sqlx::PgConnection,
    id: Uuid,
    f: DraftFields<'_>,
) -> Result<Option<TenderRecord>, TransitionError> {
    tender_query_returning!(
        "UPDATE core.tenders
         SET title = $2, title_kk = $3, submission_deadline = $4, opening_at = $5,
             trading_at = $6, zoom_url = $7
         WHERE id = $1 AND status = 'draft'",
        id,
        f.title,
        f.title_kk,
        f.submission_deadline,
        f.opening_at,
        f.trading_at,
        f.zoom_url
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(map_rule)
}

/// Отказ по ограничению БД - причина из перечня, все прочее - поломка.
///
/// `P0001` здесь наравне с `23514`: сроки тендера стережет и CHECK таблицы,
/// и триггеры публикации, и по коду они не различимы для вызывающего.
fn map_rule(err: sqlx::Error) -> TransitionError {
    if let sqlx::Error::Database(db_err) = &err
        && matches!(
            db_err.code().as_deref(),
            Some("P0001") | Some("23514") | Some("23503") | Some("23505")
        )
    {
        return TransitionError::Rejected(crate::rule::rejection(db_err.as_ref()));
    }
    TransitionError::Db(err)
}

/// Ссылка на запись торгов (FR-306, п. 72): вносится после того, как торги
/// завершены, - до итогов записи не существует. Возвращает None, если тендер
/// не найден или еще не подведен: проверку держит условие запроса, а не
/// порядок вызовов в обработчике.
pub async fn set_recording_url(
    db: &Db,
    actor: Uuid,
    id: Uuid,
    recording_url: Option<&str>,
) -> Result<Option<TenderRecord>, sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        tender_query_returning!(
            "UPDATE core.tenders SET zoom_recording_url = $2
             WHERE id = $1 AND status IN ('summed_up', 'contracted')",
            id,
            recording_url
        )
        .fetch_optional(&mut *tx)
        .await
    })
    .await
}

#[derive(Debug, thiserror::Error)]
pub enum TransitionError {
    #[error("тендер не найден")]
    NotFound,
    /// Переход отклонен правилами БД (INV-021, FR-303) - текст причины из RAISE
    #[error("{0}")]
    Rejected(RuleRejection),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Смена статуса; законность перехода решает триггер БД (INV-021).
pub async fn transition(
    db: &Db,
    actor: Uuid,
    id: Uuid,
    to_status: &str,
) -> Result<TenderRecord, TransitionError> {
    crate::with_actor(db, actor, async |tx| {
        // Смена на тот же статус - не переход: не no-op, а ошибка оператора (INV-021)
        let result = tender_query_returning!(
            "UPDATE core.tenders SET status = $2::text::core.tender_status
             WHERE id = $1 AND status IS DISTINCT FROM $2::text::core.tender_status",
            id,
            to_status
        )
        .fetch_optional(&mut *tx)
        .await;

        match result {
            Ok(Some(record)) => Ok(record),
            Ok(None) => {
                let exists = sqlx::query_scalar!(
                    r#"SELECT status::text AS "status!" FROM core.tenders WHERE id = $1"#,
                    id
                )
                .fetch_optional(&mut *tx)
                .await?;
                match exists {
                    Some(current) => {
                        Err(TransitionError::Rejected(RuleRejection::classify(format!(
                            "INV-021: тендер уже в статусе {current} - повторный переход невозможен"
                        ))))
                    }
                    None => Err(TransitionError::NotFound),
                }
            }
            Err(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("23514") => Err(
                TransitionError::Rejected(crate::rule::rejection(db_err.as_ref())),
            ),
            Err(other) => Err(TransitionError::Db(other)),
        }
    })
    .await
}

/// Количество тендеров по статусам - бизнес-метрика дашборда (T17).
pub async fn count_by_status(db: &Db) -> Result<Vec<(String, i64)>, sqlx::Error> {
    // ORDER BY по порядковому номеру: под псевдонимом `status!` имя
    // выходного столбца сменилось, и `ORDER BY status` попал бы на
    // перечисление, а не на его текст
    let rows = sqlx::query!(
        r#"SELECT status::text AS "status!", count(*) AS "count!"
           FROM core.tenders GROUP BY status ORDER BY 1"#
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|r| (r.status, r.count)).collect())
}
