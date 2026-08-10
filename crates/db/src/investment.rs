//! Инвестиционные договоры особого порядка (М12, FR-1204, п. 91–94).
//!
//! Договор заводится в общей таблице `core.contracts` (объект, наниматель,
//! ставка, период найма - там же, INV-DB-02 продолжает действовать), а его
//! инвестиционная часть живет в `core.investment_contracts`. Правила п. 91–94
//! стерегут триггеры: комплект приложений перед подписанием (INV-091),
//! предельный срок (INV-094), условия продления (п. 93).

use rust_decimal::Decimal;
use time::{Date, OffsetDateTime};
use tou_domain::rule::RuleRejection;
use uuid::Uuid;

use crate::Db;

#[derive(Debug, thiserror::Error)]
pub enum InvestmentError {
    #[error("инвестиционный договор не найден")]
    NotFound,
    /// Отказ правила п. 91–94 (триггер или CHECK)
    #[error("{0}")]
    Rejected(RuleRejection),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

fn map_rule(err: sqlx::Error) -> InvestmentError {
    if let sqlx::Error::Database(db_err) = &err
        && matches!(
            db_err.code().as_deref(),
            Some("P0001") | Some("23514") | Some("23503") | Some("23505")
        )
    {
        return InvestmentError::Rejected(crate::rule::rejection(db_err.as_ref()));
    }
    InvestmentError::Db(err)
}

/// Обязательное приложение проекта (п. 91)
pub struct AttachmentRecord {
    pub code: String,
    pub ordinal: i32,
    pub label_ru: String,
    pub label_kk: Option<String>,
    pub label_en: Option<String>,
    pub rule_ref: String,
}

/// Закрытый перечень приложений п. 91.
pub async fn list_attachments(db: &Db) -> Result<Vec<AttachmentRecord>, sqlx::Error> {
    sqlx::query_as!(
        AttachmentRecord,
        "SELECT code, ordinal, label_ru, label_kk, label_en, rule_ref
         FROM refdata.investment_attachments ORDER BY ordinal"
    )
    .fetch_all(db)
    .await
}

pub struct InvestmentRecord {
    pub id: Uuid,
    pub contract_id: Uuid,
    pub special_request_id: Uuid,
    pub investment_amount: Decimal,
    pub term_months: i32,
    pub extended_at: Option<OffsetDateTime>,
    pub extension_months: Option<i32>,
    pub prolonged_at: Option<OffsetDateTime>,
    pub prolongation_months: Option<i32>,
    /// Принято актами приемки (п. 92)
    pub accepted_amount: Decimal,
    pub contract_status: String,
    pub object_name: Option<String>,
    pub tenant_name: Option<String>,
    pub monthly_rate: Decimal,
    /// Снимок расчета ставки - публикуемое обоснование (FR-1403, п. 97)
    pub rate_calculation: Option<serde_json::Value>,
    /// Коды приложенных документов п. 91
    pub attachments: Vec<String>,
}

/// Выборка инвестиционного договора: общий список столбцов + хвост запроса.
///
/// `!` там, где планировщик считает выражение потенциально NULL: результат
/// функции `investment_accepted`, приведение `::text`, `coalesce` над
/// подзапросом-массивом. `object_name` наоборот помечен `?` - столбец
/// `o.name` объявлен NOT NULL, а поле записи остается `Option` (публичный
/// API не меняется).
macro_rules! investment_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            InvestmentRecord,
            r#"SELECT i.id, i.contract_id, i.special_request_id,
                      i.investment_amount, i.term_months, i.extended_at, i.extension_months,
                      i.prolonged_at, i.prolongation_months, i.rate_calculation,
                      core.investment_accepted(i.contract_id) AS "accepted_amount!",
                      c.status::text AS "contract_status!", c.monthly_rate,
                      o.name AS "object_name?", u.full_name AS "tenant_name?",
                      coalesce(array(SELECT f.code FROM core.investment_contract_files f
                                     WHERE f.contract_id = i.contract_id ORDER BY f.code),
                               ARRAY[]::text[]) AS "attachments!"
               FROM core.investment_contracts i
               JOIN core.contracts c ON c.id = i.contract_id
               JOIN core.objects o ON o.id = c.object_id
               LEFT JOIN core.users u ON u.id = c.tenant_id"# + $tail
            $(, $arg)*
        )
    };
}

pub async fn list(db: &Db) -> Result<Vec<InvestmentRecord>, sqlx::Error> {
    let rows = investment_query!(" ORDER BY i.created_at DESC LIMIT $1", crate::MAX_ROWS)
        .fetch_all(db)
        .await?;
    crate::warn_if_capped(rows.len(), "investment::list");
    Ok(rows)
}

pub async fn get(db: &Db, id: Uuid) -> Result<Option<InvestmentRecord>, sqlx::Error> {
    investment_query!(" WHERE i.id = $1", id)
        .fetch_optional(db)
        .await
}

pub async fn by_request(
    db: &Db,
    request_id: Uuid,
) -> Result<Option<InvestmentRecord>, sqlx::Error> {
    investment_query!(" WHERE i.special_request_id = $1", request_id)
        .fetch_optional(db)
        .await
}

pub struct NewInvestmentContract {
    pub special_request_id: Uuid,
    /// Ставка договора: считает калькулятор Прил. 4 (FR-201)
    pub monthly_rate: Decimal,
    /// Срок договора в месяцах - не более семи лет (INV-094)
    pub term_months: i32,
    /// Снимок расчета ставки - публикуемое обоснование (FR-1403, п. 97)
    pub rate_calculation: serde_json::Value,
}

/// Составление инвестиционного договора по удовлетворенной заявке (п. 90–91).
/// Объект, наниматель и объем инвестиций переносятся из заявки один раз:
/// это существенные условия, дальше их держит `freeze_terms` (FR-901).
pub async fn create(
    db: &Db,
    actor: Uuid,
    new: NewInvestmentContract,
) -> Result<InvestmentRecord, InvestmentError> {
    crate::with_actor(db, actor, async |tx| {
        let request = sqlx::query!(
            "SELECT applicant_id, object_id, investment_amount
             FROM core.special_requests WHERE id = $1",
            new.special_request_id
        )
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(InvestmentError::NotFound)?;

        let object_id = request.object_id.ok_or_else(|| {
            InvestmentError::Rejected(RuleRejection::classify(
                "FR-1204: инвестиционный договор заключается на конкретный объект (п. 91)",
            ))
        })?;
        let investment_amount = request.investment_amount.ok_or_else(|| {
            InvestmentError::Rejected(RuleRejection::classify(
                "FR-1204: в заявке не указан объем инвестиций (п. 97)",
            ))
        })?;
        let tenant_id = request.applicant_id;

        let contract_id = sqlx::query_scalar!(
            "INSERT INTO core.contracts (object_id, tenant_id, monthly_rate)
             VALUES ($1, $2, $3) RETURNING id",
            object_id,
            tenant_id,
            new.monthly_rate
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_rule)?;

        let id = sqlx::query_scalar!(
            "INSERT INTO core.investment_contracts
               (contract_id, special_request_id, investment_amount, term_months,
                rate_calculation)
             VALUES ($1, $2, $3, $4, $5) RETURNING id",
            contract_id,
            new.special_request_id,
            investment_amount,
            new.term_months,
            new.rate_calculation
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_rule)?;

        let record = investment_query!(" WHERE i.id = $1", id)
            .fetch_one(&mut *tx)
            .await?;
        Ok(record)
    })
    .await
}

pub struct NewAttachmentFile<'a> {
    pub contract_id: Uuid,
    /// Позиция перечня п. 91 (FK на справочник)
    pub code: &'a str,
    pub file_key: &'a str,
    pub filename: &'a str,
    pub content_type: &'a str,
    pub size_bytes: i64,
}

/// Приложение к договору (п. 91): повторная загрузка заменяет прежний файл -
/// позиция перечня в договоре одна.
pub async fn add_file(
    db: &Db,
    actor: Uuid,
    new: NewAttachmentFile<'_>,
) -> Result<(), InvestmentError> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query!(
            "INSERT INTO core.investment_contract_files
               (contract_id, code, file_key, filename, content_type, size_bytes)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (contract_id, code) DO UPDATE
               SET file_key = EXCLUDED.file_key, filename = EXCLUDED.filename,
                   content_type = EXCLUDED.content_type, size_bytes = EXCLUDED.size_bytes,
                   uploaded_at = core.now()",
            new.contract_id,
            new.code,
            new.file_key,
            new.filename,
            new.content_type,
            new.size_bytes
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;
        Ok(())
    })
    .await
}

pub struct FileRecord {
    pub id: Uuid,
    pub contract_id: Uuid,
    pub code: String,
    pub file_key: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub uploaded_at: OffsetDateTime,
}

/// Выборка приложенного файла: общий список столбцов + хвост запроса.
macro_rules! file_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            FileRecord,
            "SELECT id, contract_id, code, file_key, filename, content_type,
                    size_bytes, uploaded_at
             FROM core.investment_contract_files" + $tail
            $(, $arg)*
        )
    };
}

pub async fn list_files(db: &Db, contract_id: Uuid) -> Result<Vec<FileRecord>, sqlx::Error> {
    file_query!(" WHERE contract_id = $1 ORDER BY code", contract_id)
        .fetch_all(db)
        .await
}

pub async fn get_file(db: &Db, file_id: Uuid) -> Result<Option<FileRecord>, sqlx::Error> {
    file_query!(" WHERE id = $1", file_id)
        .fetch_optional(db)
        .await
}

pub struct AcceptanceRecord {
    pub id: Uuid,
    pub contract_id: Uuid,
    pub act_date: Date,
    pub accepted_amount: Decimal,
    pub note: Option<String>,
    pub accepted_by_name: Option<String>,
    pub pdf_key: Option<String>,
    pub created_at: OffsetDateTime,
}

/// Выборка акта приемки: общий список столбцов + хвост запроса.
///
/// `accepted_by_name` получает `?`: имя приходит `LEFT JOIN`'ом, а
/// `core.users.full_name` - NOT NULL, и без аннотации sqlx вывел бы non-null.
macro_rules! acceptance_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            AcceptanceRecord,
            r#"SELECT a.id, a.contract_id, a.act_date, a.accepted_amount,
                    a.note, u.full_name AS "accepted_by_name?", a.pdf_key, a.created_at
             FROM core.investment_acceptances a
             LEFT JOIN core.users u ON u.id = a.accepted_by"# + $tail
            $(, $arg)*
        )
    };
}

pub async fn list_acceptances(
    db: &Db,
    contract_id: Uuid,
) -> Result<Vec<AcceptanceRecord>, sqlx::Error> {
    acceptance_query!(
        " WHERE a.contract_id = $1 ORDER BY a.act_date, a.created_at",
        contract_id
    )
    .fetch_all(db)
    .await
}

/// Один акт приемки (п. 92) - например, для его публикации (FR-1403).
pub async fn acceptance(db: &Db, id: Uuid) -> Result<Option<AcceptanceRecord>, sqlx::Error> {
    acceptance_query!(" WHERE a.id = $1", id)
        .fetch_optional(db)
        .await
}

/// Акт приемки инвестиций комиссией (п. 92).
pub async fn accept(
    db: &Db,
    actor: Uuid,
    contract_id: Uuid,
    act_date: Date,
    amount: Decimal,
    note: Option<&str>,
) -> Result<AcceptanceRecord, InvestmentError> {
    crate::with_actor(db, actor, async |tx| {
        let id = sqlx::query_scalar!(
            "INSERT INTO core.investment_acceptances
               (contract_id, act_date, accepted_amount, note, accepted_by)
             VALUES ($1, $2, $3, $4, $5) RETURNING id",
            contract_id,
            act_date,
            amount,
            note,
            actor
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_rule)?;

        let record = acceptance_query!(" WHERE a.id = $1", id)
            .fetch_one(&mut *tx)
            .await?;
        Ok(record)
    })
    .await
}

/// Печатная форма акта приемки (п. 92) - ключ проставляется один раз.
pub async fn attach_acceptance_pdf(
    db: &Db,
    actor: Uuid,
    acceptance_id: Uuid,
    pdf_key: &str,
) -> Result<(), InvestmentError> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query!(
            "UPDATE core.investment_acceptances SET pdf_key = $2 WHERE id = $1",
            acceptance_id,
            pdf_key
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;
        Ok(())
    })
    .await
}

/// Продление договора (п. 93): условия исполнения и порогов проверяет
/// триггер БД, здесь - сама отметка.
pub async fn extend(
    db: &Db,
    actor: Uuid,
    id: Uuid,
    prolongation: bool,
    months: i32,
) -> Result<InvestmentRecord, InvestmentError> {
    crate::with_actor(db, actor, async |tx| {
        // Продление и пролонгация пишут разные столбцы, а имя столбца
        // параметром не биндится. Поэтому ветвление дает не кусок SQL,
        // а целый запрос - он статичен и проверяется по схеме
        let updated = if prolongation {
            sqlx::query!(
                "UPDATE core.investment_contracts
                 SET prolonged_at = core.now(), prolongation_months = $2 WHERE id = $1",
                id,
                months
            )
            .execute(&mut *tx)
            .await
        } else {
            sqlx::query!(
                "UPDATE core.investment_contracts
                 SET extended_at = core.now(), extension_months = $2 WHERE id = $1",
                id,
                months
            )
            .execute(&mut *tx)
            .await
        }
        .map_err(map_rule)?;
        if updated.rows_affected() == 0 {
            return Err(InvestmentError::NotFound);
        }

        let record = investment_query!(" WHERE i.id = $1", id)
            .fetch_one(&mut *tx)
            .await?;
        Ok(record)
    })
    .await
}
