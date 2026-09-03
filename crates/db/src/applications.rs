//! Заявки участников и журнал регистрации (М4, FR-401–404).
//!
//! Подача и отзыв - одна транзакция `with_actor` вместе с журнальной записью
//! (Прил. 12): дедлайн стережет триггер БД (INV-037) и откатывает всё целиком,
//! запечатанность цен до вскрытия - RLS (INV-040), поэтому и чтения выполняются
//! под GUC `app.user_id`. Audit пишут триггеры INV-AUDIT.

use rust_decimal::Decimal;
use time::OffsetDateTime;
use tou_domain::redacted::Redacted;
use tou_domain::rule::RuleRejection;
use uuid::Uuid;

use crate::Db;

pub struct ApplicationRecord {
    pub id: Uuid,
    pub tender_id: Uuid,
    pub lot_id: Uuid,
    pub participant_id: Uuid,
    pub status: String,
    pub applicant_kind: String,
    /// Сведения о заявителе (Прил. 2): ИИН/БИН, адрес, контакты - ПДн
    /// за `Redacted` (NFR-07), распаковываются только на границе слоя
    pub applicant_details: Redacted<serde_json::Value>,
    pub qualification: Option<Redacted<serde_json::Value>>,
    pub submitted_at: OffsetDateTime,
    pub withdrawn_at: Option<OffsetDateTime>,
    /// Код основания отклонения из закрытого перечня (FR-502, INV-052)
    pub rejection_reason: Option<String>,
    /// Семь обязательных типов PDF приложены и сохранены зашифрованными.
    pub package_complete: bool,
    /// Ценовое предложение (Прил. 9). None - строку скрыла RLS (INV-040):
    /// до вскрытия цену видит только сам участник.
    pub price_amount: Option<Decimal>,
}

/// Строка выборки: то же, что [`ApplicationRecord`], но ПДн еще не завернуты
/// в `Redacted` - `query_as!` кладет столбцы в поля как есть, а обертка
/// доменная. Упаковка остается здесь.
pub(crate) struct ApplicationRow {
    pub(crate) id: Uuid,
    pub(crate) tender_id: Uuid,
    pub(crate) lot_id: Uuid,
    pub(crate) participant_id: Uuid,
    pub(crate) status: String,
    pub(crate) applicant_kind: String,
    pub(crate) applicant_details: serde_json::Value,
    pub(crate) qualification: Option<serde_json::Value>,
    pub(crate) submitted_at: OffsetDateTime,
    pub(crate) withdrawn_at: Option<OffsetDateTime>,
    pub(crate) rejection_reason: Option<String>,
    pub(crate) package_complete: bool,
    pub(crate) price_amount: Option<Decimal>,
}

impl From<ApplicationRow> for ApplicationRecord {
    fn from(row: ApplicationRow) -> Self {
        Self {
            id: row.id,
            tender_id: row.tender_id,
            lot_id: row.lot_id,
            participant_id: row.participant_id,
            status: row.status,
            applicant_kind: row.applicant_kind,
            applicant_details: Redacted::new(row.applicant_details),
            qualification: row.qualification.map(Redacted::new),
            submitted_at: row.submitted_at,
            withdrawn_at: row.withdrawn_at,
            rejection_reason: row.rejection_reason,
            package_complete: row.package_complete,
            price_amount: row.price_amount,
        }
    }
}

/// Выборка заявки: общий список столбцов + хвост запроса.
///
/// Цена приходит через LEFT JOIN - ее видимость решает RLS (INV-040),
/// поэтому `price_amount` остается nullable без аннотации. `!` у `status`
/// и `applicant_kind` - это `::text`, который планировщик считает
/// потенциально NULL, хотя столбцы NOT NULL.
macro_rules! application_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            crate::applications::ApplicationRow,
            r#"SELECT a.id, a.tender_id, a.lot_id, a.participant_id,
                      a.status::text AS "status!",
                      a.applicant_kind::text AS "applicant_kind!",
                      a.applicant_details, a.qualification, a.submitted_at,
                      a.withdrawn_at, a.rejection_reason,
                      core.application_package_complete(a.id) AS "package_complete!",
                      core.price_amount(p) AS price_amount
               FROM core.applications a
               LEFT JOIN core.price_proposals p ON p.application_id = a.id"# + $tail
            $(, $arg)*
        )
    };
}
pub(crate) use application_query;

pub struct FileRecord {
    pub id: Uuid,
    pub application_id: Uuid,
    pub file_key: String,
    pub filename: String,
    pub document_kind: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub encryption_version: i16,
    pub uploaded_at: OffsetDateTime,
}

macro_rules! file_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            FileRecord,
            "SELECT id, application_id, file_key, filename,
                    document_kind::text AS \"document_kind!\", content_type,
                    size_bytes, encryption_version, uploaded_at
             FROM core.application_files" + $tail
            $(, $arg)*
        )
    };
}
/// То же для `RETURNING`: там столбцы идут в конце запроса, поэтому макрос
/// принимает не хвост, а голову.
macro_rules! file_query_returning {
    ($head:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            FileRecord,
            $head + " RETURNING id, application_id, file_key, filename,
                                document_kind::text AS \"document_kind!\",
                                content_type, size_bytes, encryption_version, uploaded_at"
            $(, $arg)*
        )
    };
}

pub struct JournalRecord {
    pub id: Uuid,
    pub tender_id: Uuid,
    pub seq: i32,
    pub entry_kind: String,
    pub application_id: Option<Uuid>,
    pub actor_id: Option<Uuid>,
    pub occurred_at: OffsetDateTime,
    pub note: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum SubmitError {
    /// Тендер не в статусе `accepting` или лот не принадлежит тендеру (п. 36)
    #[error("прием заявок по тендеру не открыт")]
    NotAccepting,
    /// Один участник - одна заявка на лот (п. 22, UNIQUE)
    #[error("заявка на этот лот уже подана")]
    Duplicate,
    /// Отказ триггера БД (INV-037: дедлайн истек) - текст причины из RAISE
    #[error("{0}")]
    Rejected(RuleRejection),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

pub struct NewApplication<'a> {
    pub tender_id: Uuid,
    pub lot_id: Uuid,
    pub applicant_kind: &'a str,
    pub applicant_details: &'a serde_json::Value,
    pub qualification: Option<&'a serde_json::Value>,
    /// Первоначальная цена (Прил. 9) - запечатывается RLS до вскрытия
    pub price_amount: Decimal,
}

/// Подача заявки (FR-401): заявка + ценовое предложение + запись журнала -
/// одна транзакция. После дедлайна журнальный триггер (INV-037) откатывает всё.
pub async fn submit(
    db: &Db,
    actor: Uuid,
    new: NewApplication<'_>,
) -> Result<ApplicationRecord, SubmitError> {
    crate::with_actor(db, actor, async |tx| {
        // Прием открыт и лот принадлежит тендеру - иначе вставки не будет
        let inserted = sqlx::query_scalar!(
            "INSERT INTO core.applications
               (tender_id, lot_id, participant_id, applicant_kind, applicant_details, qualification)
             SELECT $1::uuid, $2::uuid, $3::uuid, $4::text::core.applicant_kind,
                    $5::jsonb, $6::jsonb
             WHERE EXISTS (
               SELECT 1 FROM core.tenders t
               JOIN core.lots l ON l.tender_id = t.id
               WHERE t.id = $1 AND l.id = $2 AND t.status = 'accepting'
             )
             RETURNING id",
            new.tender_id,
            new.lot_id,
            actor,
            new.applicant_kind,
            new.applicant_details,
            new.qualification
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_submit_error)?;

        let application_id = inserted.ok_or(SubmitError::NotAccepting)?;

        sqlx::query!(
            "INSERT INTO core.price_proposals (application_id, amount) VALUES ($1, $2)",
            application_id,
            new.price_amount
        )
        .execute(&mut *tx)
        .await
        .map_err(map_submit_error)?;

        // Прил. 12: факт подачи фиксируется журналом; seq и время ставит БД
        sqlx::query!(
            "INSERT INTO core.journal_entries (tender_id, entry_kind, application_id, actor_id)
             VALUES ($1, 'application_submitted', $2, $3)",
            new.tender_id,
            application_id,
            actor
        )
        .execute(&mut *tx)
        .await
        .map_err(map_submit_error)?;

        let record = application_query!(" WHERE a.id = $1", application_id)
            .fetch_one(&mut *tx)
            .await?;
        Ok(record.into())
    })
    .await
}

fn map_submit_error(err: sqlx::Error) -> SubmitError {
    if let sqlx::Error::Database(db_err) = &err {
        match db_err.code().as_deref() {
            Some("23505") => return SubmitError::Duplicate,
            Some("23514") => return SubmitError::Rejected(crate::rule::rejection(db_err.as_ref())),
            _ => {}
        }
    }
    SubmitError::Db(err)
}

#[derive(Debug, thiserror::Error)]
pub enum WithdrawError {
    /// Нет такой поданной заявки у этого участника
    #[error("заявка не найдена или уже не в статусе «подана»")]
    NotWithdrawable,
    /// Отказ триггера БД (INV-037): после дедлайна отзыв запрещен (FR-404)
    #[error("{0}")]
    Rejected(RuleRejection),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Отзыв заявки участником (FR-404): статус + запись журнала одной транзакцией;
/// после дедлайна журнальный триггер откатывает и смену статуса.
pub async fn withdraw(
    db: &Db,
    actor: Uuid,
    application_id: Uuid,
) -> Result<ApplicationRecord, WithdrawError> {
    crate::with_actor(db, actor, async |tx| {
        let updated = sqlx::query_scalar!(
            "UPDATE core.applications
             SET status = 'withdrawn', withdrawn_at = core.now()
             WHERE id = $1 AND participant_id = $2 AND status = 'submitted'
             RETURNING tender_id",
            application_id,
            actor
        )
        .fetch_optional(&mut *tx)
        .await?;

        let tender_id = updated.ok_or(WithdrawError::NotWithdrawable)?;

        // Отзыв заявки с подтвержденным взносом запускает срок возврата
        // (FR-1002, п. 26.1): 15 рабочих дней у департамента финансов
        schedule_refund_if_paid(&mut *tx, application_id).await?;

        sqlx::query!(
            "INSERT INTO core.journal_entries (tender_id, entry_kind, application_id, actor_id)
             VALUES ($1, 'application_withdrawn', $2, $3)",
            tender_id,
            application_id,
            actor
        )
        .execute(&mut *tx)
        .await
        .map_err(|err| {
            if let sqlx::Error::Database(db_err) = &err
                && db_err.code().as_deref() == Some("23514")
            {
                return WithdrawError::Rejected(crate::rule::rejection(db_err.as_ref()));
            }
            WithdrawError::Db(err)
        })?;

        let record = application_query!(" WHERE a.id = $1", application_id)
            .fetch_one(&mut *tx)
            .await?;
        Ok(record.into())
    })
    .await
}

/// Заявки участника (кабинет). Чтение под GUC: RLS отдаст его собственные цены.
pub async fn list_own(db: &Db, actor: Uuid) -> Result<Vec<ApplicationRecord>, sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        let rows = application_query!(
            " WHERE a.participant_id = $1 ORDER BY a.submitted_at DESC LIMIT $2",
            actor,
            crate::MAX_ROWS
        )
        .fetch_all(&mut *tx)
        .await?;
        crate::warn_if_capped(rows.len(), "applications::list_own");
        Ok(rows.into_iter().map(ApplicationRecord::from).collect())
    })
    .await
}

/// Срок возврата взноса (FR-1002, п. 26) ставится только там, где есть что
/// возвращать: поступление подтверждено и остаток счета положителен.
pub(crate) async fn schedule_refund_if_paid(
    tx: &mut sqlx::PgConnection,
    application_id: Uuid,
) -> Result<(), sqlx::Error> {
    // Агрегат без GROUP BY всегда дает ровно одну строку, а сравнение
    // с COALESCE не бывает NULL - отсюда `!` и `fetch_one`
    let has_balance = sqlx::query_scalar!(
        r#"SELECT COALESCE(sum(e.credit - e.debit), 0) > 0 AS "has_balance!"
           FROM core.ledger_accounts acc
           JOIN core.ledger_entries e ON e.account_id = acc.id
           WHERE acc.application_id = $1"#,
        application_id
    )
    .fetch_one(&mut *tx)
    .await?;

    if has_balance {
        crate::obligations::schedule(
            tx,
            tou_domain::obligation::ObligationAction::FeeRefund,
            crate::obligations::Subject {
                application_id: Some(application_id),
                ..Default::default()
            },
        )
        .await?;
    }
    Ok(())
}

/// Заявка по id глазами `actor`: цена видна по правилам RLS (INV-040).
pub async fn get(db: &Db, actor: Uuid, id: Uuid) -> Result<Option<ApplicationRecord>, sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        Ok(application_query!(" WHERE a.id = $1", id)
            .fetch_optional(&mut *tx)
            .await?
            .map(ApplicationRecord::from))
    })
    .await
}

/// Заявки тендера (secretary/commission, FR-402): цены до вскрытия скрыты RLS.
pub async fn list_for_tender(
    db: &Db,
    actor: Uuid,
    tender_id: Uuid,
) -> Result<Vec<ApplicationRecord>, sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        Ok(
            application_query!(" WHERE a.tender_id = $1 ORDER BY a.submitted_at", tender_id)
                .fetch_all(&mut *tx)
                .await?
                .into_iter()
                .map(ApplicationRecord::from)
                .collect(),
        )
    })
    .await
}

/// Метаданные файла заявки. Вставка возможна, пока заявка «подана»
/// и прием по тендеру не закрыт (состав заявки фиксируется дедлайном, п. 36–39).
pub struct NewApplicationFile<'a> {
    pub application_id: Uuid,
    pub file_key: &'a str,
    pub filename: &'a str,
    pub document_kind: &'a str,
    pub content_type: &'a str,
    pub size_bytes: i64,
}

pub async fn add_file(
    db: &Db,
    actor: Uuid,
    new: NewApplicationFile<'_>,
) -> Result<Option<FileRecord>, sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        file_query_returning!(
            "INSERT INTO core.application_files
               (application_id, file_key, filename, document_kind,
                content_type, size_bytes, encryption_version)
             SELECT $1::uuid, $2::text, $3::text,
                    $4::text::core.application_document_kind,
                    $5::text, $6::bigint, 1::smallint
             WHERE EXISTS (
               SELECT 1 FROM core.applications a
               JOIN core.tenders t ON t.id = a.tender_id
               WHERE a.id = $1 AND a.participant_id = $7 AND a.status = 'submitted'
                 AND (t.submission_deadline IS NULL OR core.now() <= t.submission_deadline)
             )",
            new.application_id,
            new.file_key,
            new.filename,
            new.document_kind,
            new.content_type,
            new.size_bytes,
            actor
        )
        .fetch_optional(&mut *tx)
        .await
    })
    .await
}

pub async fn list_files(db: &Db, application_id: Uuid) -> Result<Vec<FileRecord>, sqlx::Error> {
    file_query!(
        " WHERE application_id = $1 ORDER BY document_kind, uploaded_at",
        application_id
    )
    .fetch_all(db)
    .await
}

/// Файлы набора заявок одним запросом (списки без N+1).
pub async fn files_for(db: &Db, application_ids: &[Uuid]) -> Result<Vec<FileRecord>, sqlx::Error> {
    file_query!(
        " WHERE application_id = ANY($1) ORDER BY application_id, document_kind, uploaded_at",
        application_ids
    )
    .fetch_all(db)
    .await
}

pub async fn get_file(db: &Db, file_id: Uuid) -> Result<Option<FileRecord>, sqlx::Error> {
    file_query!(" WHERE id = $1", file_id)
        .fetch_optional(db)
        .await
}

/// Журнал регистрации тендера (Прил. 12) - для секретаря (FR-402).
pub async fn journal_of(db: &Db, tender_id: Uuid) -> Result<Vec<JournalRecord>, sqlx::Error> {
    sqlx::query_as!(
        JournalRecord,
        r#"SELECT id, tender_id, seq, entry_kind::text AS "entry_kind!",
                  application_id, actor_id, occurred_at, note
           FROM core.journal_entries WHERE tender_id = $1 ORDER BY seq"#,
        tender_id
    )
    .fetch_all(db)
    .await
}
