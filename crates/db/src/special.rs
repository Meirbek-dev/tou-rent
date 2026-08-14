//! Особый порядок: каталог категорий и заявка (М12, FR-1201, п. 87–88).
//!
//! Категория приходит из `refdata.special_categories` (INV-087) - FK не дает
//! завести заявку по выдуманной категории, а требования категории (документы,
//! срок проверки, льготная схема, публикуемость) читаются оттуда же, без
//! зашитых в код предметных значений. Мутации идут через `with_actor`:
//! audit-триггер таблицы заявок пишет актора (регламент А.5).

use rust_decimal::Decimal;
use time::OffsetDateTime;
use tou_domain::obligation::{ObligationAction, Term};
use tou_domain::redacted::Redacted;
use tou_domain::rule::RuleRejection;
use tou_domain::special::Competition;
use uuid::Uuid;

use crate::Db;

/// Требуемый документ категории (п. 88)
pub struct CategoryDocument {
    pub category_code: String,
    pub code: String,
    pub ordinal: i32,
    pub label_ru: String,
    pub label_kk: Option<String>,
    pub label_en: Option<String>,
    pub required: bool,
}

/// Категория особого порядка со своими декларациями (FR-1201)
pub struct CategoryRecord {
    pub code: String,
    pub ordinal: i32,
    pub label_ru: String,
    pub label_kk: Option<String>,
    pub label_en: Option<String>,
    pub rule_ref: String,
    pub review_days: i32,
    /// Вид дней срока проверки: `business` | `calendar` (FR-1202)
    pub review_term: String,
    pub benefit_scheme: String,
    pub publishable: bool,
    /// Что делать при двух и более заявках (FR-1203, п. 86)
    pub competition: String,
    /// Порог сопоставимости сумм инвестиций, % (п. 97)
    pub comparable_margin_pct: Decimal,
}

/// Каталог категорий п. 87 в порядке номеров.
///
/// `!` у `review_term` и `competition` - это `::text`, который планировщик
/// считает потенциально NULL, хотя столбцы NOT NULL.
pub async fn list_categories(db: &Db) -> Result<Vec<CategoryRecord>, sqlx::Error> {
    sqlx::query_as!(
        CategoryRecord,
        r#"SELECT code, ordinal, label_ru, label_kk, label_en, rule_ref,
                  review_days, review_term::text AS "review_term!", benefit_scheme,
                  publishable, competition::text AS "competition!", comparable_margin_pct
           FROM refdata.special_categories ORDER BY ordinal"#
    )
    .fetch_all(db)
    .await
}

/// Требуемые документы всех категорий одним запросом (перечни короткие).
pub async fn list_category_documents(db: &Db) -> Result<Vec<CategoryDocument>, sqlx::Error> {
    sqlx::query_as!(
        CategoryDocument,
        "SELECT category_code, code, ordinal, label_ru, label_kk, label_en, required
         FROM refdata.special_category_documents ORDER BY category_code, ordinal"
    )
    .fetch_all(db)
    .await
}

pub struct RequestRecord {
    pub id: Uuid,
    pub applicant_id: Uuid,
    pub applicant_name: Option<String>,
    pub category: String,
    pub category_label: String,
    pub category_rule_ref: String,
    pub status: String,
    pub applicant_kind: String,
    /// Сведения о заявителе (Прил. 3): ПДн за `Redacted` (NFR-07)
    pub applicant_details: Redacted<serde_json::Value>,
    pub object_id: Option<Uuid>,
    pub object_name: Option<String>,
    pub purpose: String,
    pub requested_months: Option<i32>,
    /// Объем инвестиций (FR-1203, п. 97): им ранжируются конкурирующие заявки
    pub investment_amount: Option<Decimal>,
    /// Тендер, созданный переводом вопроса в общий порядок (п. 86)
    pub tender_id: Option<Uuid>,
    pub submitted_at: OffsetDateTime,
    pub withdrawn_at: Option<OffsetDateTime>,
}

/// Строка выборки: то же, что [`RequestRecord`], но ПДн еще не за `Redacted`.
///
/// Отдельный тип по той же причине, что и в `acts.rs`: `query_as!` кладет
/// столбец в поле как есть, а `Redacted` - доменная обертка (NFR-07).
struct RequestRow {
    id: Uuid,
    applicant_id: Uuid,
    applicant_name: Option<String>,
    category: String,
    category_label: String,
    category_rule_ref: String,
    status: String,
    applicant_kind: String,
    applicant_details: serde_json::Value,
    object_id: Option<Uuid>,
    object_name: Option<String>,
    purpose: String,
    requested_months: Option<i32>,
    investment_amount: Option<Decimal>,
    tender_id: Option<Uuid>,
    submitted_at: OffsetDateTime,
    withdrawn_at: Option<OffsetDateTime>,
}

impl From<RequestRow> for RequestRecord {
    fn from(row: RequestRow) -> Self {
        Self {
            id: row.id,
            applicant_id: row.applicant_id,
            applicant_name: row.applicant_name,
            category: row.category,
            category_label: row.category_label,
            category_rule_ref: row.category_rule_ref,
            status: row.status,
            applicant_kind: row.applicant_kind,
            applicant_details: Redacted::new(row.applicant_details),
            object_id: row.object_id,
            object_name: row.object_name,
            purpose: row.purpose,
            requested_months: row.requested_months,
            investment_amount: row.investment_amount,
            tender_id: row.tender_id,
            submitted_at: row.submitted_at,
            withdrawn_at: row.withdrawn_at,
        }
    }
}

/// Выборка заявки: общий список столбцов + хвост запроса (см. `acts.rs`).
///
/// `!` у `status` и `applicant_kind` - это `::text`, который планировщик
/// считает потенциально NULL, хотя столбцы NOT NULL. `?` у `applicant_name`
/// и `object_name` - наоборот: они из LEFT JOIN, а `core.users.full_name`
/// и `core.objects.name` - NOT NULL, и sqlx вывел бы non-null по столбцу.
macro_rules! request_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            RequestRow,
            r#"SELECT r.id, r.applicant_id, u.full_name AS "applicant_name?",
                      r.category, c.label_ru AS category_label,
                      c.rule_ref AS category_rule_ref,
                      r.status::text AS "status!",
                      r.applicant_kind::text AS "applicant_kind!",
                      r.applicant_details, r.object_id, o.name AS "object_name?", r.purpose,
                      r.requested_months, r.investment_amount, r.tender_id,
                      r.submitted_at, r.withdrawn_at
               FROM core.special_requests r
               JOIN refdata.special_categories c ON c.code = r.category
               LEFT JOIN core.users u ON u.id = r.applicant_id
               LEFT JOIN core.objects o ON o.id = r.object_id"# + $tail
            $(, $arg)*
        )
    };
}

pub struct FileRecord {
    pub id: Uuid,
    pub special_request_id: Uuid,
    pub document_code: Option<String>,
    pub file_key: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub uploaded_at: OffsetDateTime,
}

/// Выборка документа: общий список столбцов + хвост запроса (см. `acts.rs`).
macro_rules! file_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            FileRecord,
            "SELECT id, special_request_id, document_code, file_key, filename,
                    content_type, size_bytes, uploaded_at
             FROM core.special_request_files" + $tail
            $(, $arg)*
        )
    };
}
/// То же для `RETURNING`: там столбцы идут в конце, поэтому макрос принимает
/// не хвост, а голову (см. `identities.rs`).
macro_rules! file_query_returning {
    ($head:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            FileRecord,
            $head + " RETURNING id, special_request_id, document_code, file_key,
                                filename, content_type, size_bytes, uploaded_at"
            $(, $arg)*
        )
    };
}

#[derive(Debug, thiserror::Error)]
pub enum SpecialError {
    #[error("заявка особого порядка не найдена")]
    NotFound,
    /// Отказ правила: FK категории (INV-087), триггер порядка состояний
    /// либо триггер перечня документов категории
    #[error("{0}")]
    Rejected(RuleRejection),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

fn map_rule(err: sqlx::Error) -> SpecialError {
    if let sqlx::Error::Database(db_err) = &err
        && matches!(
            db_err.code().as_deref(),
            Some("P0001") | Some("23514") | Some("23503") | Some("23505")
        )
    {
        return SpecialError::Rejected(crate::rule::rejection(db_err.as_ref()));
    }
    SpecialError::Db(err)
}

pub struct NewRequest<'a> {
    /// Код категории п. 87 (INV-087): проверяется FK
    pub category: &'a str,
    pub applicant_kind: &'a str,
    pub applicant_details: &'a serde_json::Value,
    pub object_id: Option<Uuid>,
    pub purpose: &'a str,
    pub requested_months: Option<i32>,
    /// Объем инвестиций - обязателен для инвестиционной категории (п. 97)
    pub investment_amount: Option<Decimal>,
}

/// Подача заявки особого порядка (Прил. 3, п. 88).
pub async fn submit(
    db: &Db,
    actor: Uuid,
    new: NewRequest<'_>,
) -> Result<RequestRecord, SpecialError> {
    crate::with_actor(db, actor, async |tx| {
        // `$3::text::core.applicant_kind`: значение приходит строкой,
        // а приведение к перечислению делает БД
        let id = sqlx::query_scalar!(
            "INSERT INTO core.special_requests
               (applicant_id, category, applicant_kind, applicant_details,
                object_id, purpose, requested_months, investment_amount)
             VALUES ($1, $2, $3::text::core.applicant_kind, $4, $5, $6, $7, $8)
             RETURNING id",
            actor,
            new.category,
            new.applicant_kind,
            new.applicant_details,
            new.object_id,
            new.purpose,
            new.requested_months,
            new.investment_amount
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_rule)?;

        // Срок проверки объявляет категория (FR-1201, п. 89): 15 календарных
        // дней в общем случае, у отдельных категорий - 10 рабочих
        let category = sqlx::query!(
            r#"SELECT review_days, review_term::text AS "review_term!"
               FROM refdata.special_categories WHERE code = $1"#,
            new.category
        )
        .fetch_one(&mut *tx)
        .await?;
        let term = u32::try_from(category.review_days)
            .ok()
            .and_then(|days| Term::from_parts(days, &category.review_term))
            .unwrap_or_else(|| ObligationAction::SpecialReview.rule().term);

        crate::obligations::schedule_with_term(
            &mut *tx,
            ObligationAction::SpecialReview,
            crate::obligations::Subject::special_request(id),
            term,
        )
        .await?;

        let record = request_query!(" WHERE r.id = $1", id)
            .fetch_one(&mut *tx)
            .await?;
        Ok(record.into())
    })
    .await
}

/// Заявки заявителя, свежие сверху (кабинет).
pub async fn list_own(db: &Db, applicant: Uuid) -> Result<Vec<RequestRecord>, sqlx::Error> {
    let rows = request_query!(
        " WHERE r.applicant_id = $1 ORDER BY r.submitted_at DESC LIMIT $2",
        applicant,
        crate::MAX_ROWS
    )
    .fetch_all(db)
    .await?;
    crate::warn_if_capped(rows.len(), "special::list_own");
    Ok(rows.into_iter().map(RequestRecord::from).collect())
}

/// Рабочий список заявок (FR-1202): по умолчанию - те, что в рассмотрении
/// (поданные ждут заключения, вынесенные ждут решения). С явным перечнем
/// состояний - например, удовлетворенные, по которым заключается договор.
pub async fn list_by_status(db: &Db, statuses: &[&str]) -> Result<Vec<RequestRecord>, sqlx::Error> {
    let names: Vec<String> = statuses.iter().map(|status| (*status).to_owned()).collect();
    let rows = request_query!(
        " WHERE r.status = ANY($1::text[]::core.special_request_status[])
          ORDER BY r.submitted_at LIMIT $2",
        &names,
        crate::MAX_ROWS
    )
    .fetch_all(db)
    .await?;
    crate::warn_if_capped(rows.len(), "special::list_by_status");
    Ok(rows.into_iter().map(RequestRecord::from).collect())
}

pub async fn get(db: &Db, id: Uuid) -> Result<Option<RequestRecord>, sqlx::Error> {
    Ok(request_query!(" WHERE r.id = $1", id)
        .fetch_optional(db)
        .await?
        .map(RequestRecord::from))
}

/// Отзыв заявки заявителем: пока решение не принято (порядок состояний
/// стережет триггер БД, он же ставит время отзыва).
pub async fn withdraw(db: &Db, actor: Uuid, id: Uuid) -> Result<RequestRecord, SpecialError> {
    crate::with_actor(db, actor, async |tx| {
        let updated = sqlx::query_scalar!(
            "UPDATE core.special_requests SET status = 'withdrawn'
             WHERE id = $1 AND applicant_id = $2 AND status IN ('submitted', 'under_review')
             RETURNING id",
            id,
            actor
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_rule)?;

        updated.ok_or(SpecialError::NotFound)?;

        // Заявка выбыла из процесса - открытые сроки по ней снимаются (FR-1702)
        crate::obligations::cancel_for(&mut *tx, crate::obligations::Subject::special_request(id))
            .await?;

        let record = request_query!(" WHERE r.id = $1", id)
            .fetch_one(&mut *tx)
            .await?;
        Ok(record.into())
    })
    .await
}

/// Документ заявки: содержимое лежит в RustFS, здесь - метаданные.
pub struct NewFile<'a> {
    pub request_id: Uuid,
    /// Позиция перечня категории (п. 88); None - прочий документ
    pub document_code: Option<&'a str>,
    pub file_key: &'a str,
    pub filename: &'a str,
    pub content_type: &'a str,
    pub size_bytes: i64,
}

/// Вложение к своей заявке до решения; позицию перечня категории проверяет
/// триггер БД. `None` - заявки нет либо она уже не принимает документы.
pub async fn add_file(
    db: &Db,
    actor: Uuid,
    new: NewFile<'_>,
) -> Result<Option<FileRecord>, SpecialError> {
    crate::with_actor(db, actor, async |tx| {
        let record = file_query_returning!(
            "INSERT INTO core.special_request_files
               (special_request_id, document_code, file_key, filename, content_type, size_bytes)
             SELECT $1, $2, $3, $4, $5, $6
             WHERE EXISTS (
               SELECT 1 FROM core.special_requests r
               WHERE r.id = $1 AND r.applicant_id = $7
                 AND r.status IN ('submitted', 'under_review')
             )",
            new.request_id,
            new.document_code,
            new.file_key,
            new.filename,
            new.content_type,
            new.size_bytes,
            actor
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_rule)?;

        Ok(record)
    })
    .await
}

pub async fn list_files(db: &Db, request_id: Uuid) -> Result<Vec<FileRecord>, sqlx::Error> {
    file_query!(
        " WHERE special_request_id = $1 ORDER BY uploaded_at",
        request_id
    )
    .fetch_all(db)
    .await
}

pub async fn get_file(db: &Db, file_id: Uuid) -> Result<Option<FileRecord>, sqlx::Error> {
    file_query!(" WHERE id = $1", file_id)
        .fetch_optional(db)
        .await
}

/// Конкуренция вокруг заявки (FR-1203, п. 86, 97): правило категории,
/// активные конкуренты на тот же объект и объемы инвестиций.
pub async fn competition(
    db: &Db,
    request_id: Uuid,
) -> Result<Option<(Competition, Vec<Uuid>)>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT c.competition::text AS "rule!", c.comparable_margin_pct,
                  r.investment_amount AS own_amount
           FROM core.special_requests r
           JOIN refdata.special_categories c ON c.code = r.category
           WHERE r.id = $1"#,
        request_id
    )
    .fetch_optional(db)
    .await?;

    let Some(row) = row else { return Ok(None) };

    // `!` у `id`: столбцы возвращает функция, а не таблица - происхождение
    // планировщик не сообщает и считает их потенциально NULL
    let rivals = sqlx::query!(
        r#"SELECT id AS "id!", investment_amount FROM core.special_competitors($1)"#,
        request_id
    )
    .fetch_all(db)
    .await?;

    let competition = Competition {
        rule: row.rule.parse().unwrap_or_default(),
        rivals: rivals.len(),
        best_rival_amount: rivals
            .iter()
            .filter_map(|rival| rival.investment_amount)
            .max(),
        own_amount: row.own_amount,
        comparable_margin_pct: row.comparable_margin_pct,
    };
    Ok(Some((
        competition,
        rivals.into_iter().map(|rival| rival.id).collect(),
    )))
}

/// Перевод вопроса в общий порядок (FR-1203, п. 86): по решению Правления
/// создается черновик тендера, а все конкурирующие заявки уходят в него
/// вместе - вопрос один, тендер тоже один.
///
/// Лоты организатор добавляет сам: ставку считает калькулятор Прил. 4
/// (FR-201), из заявки ее не вывести. Без лотов тендер не публикуется (FR-303).
pub async fn redirect_to_tender(
    db: &Db,
    actor: Uuid,
    request_id: Uuid,
    title: &str,
) -> Result<Uuid, SpecialError> {
    crate::with_actor(db, actor, async |tx| {
        // Организатором становится подразделение, проводившее проверку (п. 89)
        let organizer = sqlx::query_scalar!(
            "SELECT reviewer_id FROM core.special_reviews WHERE special_request_id = $1",
            request_id
        )
        .fetch_optional(&mut *tx)
        .await?;
        let organizer = organizer.ok_or(SpecialError::NotFound)?;

        let tender_id = sqlx::query_scalar!(
            "INSERT INTO core.tenders (title, organizer_id) VALUES ($1, $2) RETURNING id",
            title,
            organizer
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_rule)?;

        // Заявка-инициатор и ее конкуренты уходят в один тендер
        sqlx::query!(
            "UPDATE core.special_requests SET tender_id = $2
             WHERE id = $1 OR id IN (SELECT id FROM core.special_competitors($1))",
            request_id,
            tender_id
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;

        Ok(tender_id)
    })
    .await
}

/// Конкуренты, ушедшие в общий порядок вместе с заявкой (п. 86): их решения
/// принимаются тем же составом и с тем же обоснованием.
pub async fn redirect_competitors(
    db: &Db,
    actor: Uuid,
    request_id: Uuid,
    rationale: &str,
) -> Result<Vec<Uuid>, SpecialError> {
    crate::with_actor(db, actor, async |tx| {
        // `!` у `id`: столбец возвращает функция, а не таблица
        let rivals = sqlx::query_scalar!(
            r#"SELECT id AS "id!" FROM core.special_competitors($1)"#,
            request_id
        )
        .fetch_all(&mut *tx)
        .await?;

        for rival in &rivals {
            // Конкуренту тоже нужно заключение (INV-090): если его нет,
            // заявка остается поданной и подразделение доводит ее само
            let has_review = sqlx::query_scalar!(
                r#"SELECT EXISTS (
                     SELECT 1 FROM core.special_reviews WHERE special_request_id = $1
                   ) AS "has_review!""#,
                rival
            )
            .fetch_one(&mut *tx)
            .await?;
            if !has_review {
                continue;
            }

            sqlx::query!(
                "INSERT INTO core.special_board_decisions
                   (special_request_id, decision, rationale, decided_by)
                 VALUES ($1, 'redirect', $2, $3)
                 ON CONFLICT (special_request_id) DO NOTHING",
                rival,
                rationale,
                actor
            )
            .execute(&mut *tx)
            .await
            .map_err(map_rule)?;

            crate::obligations::complete(
                &mut *tx,
                ObligationAction::SpecialDecision,
                crate::obligations::Subject::special_request(*rival),
            )
            .await?;
        }

        Ok(rivals)
    })
    .await
}

/// Заключение уполномоченного подразделения (п. 89).
pub struct ReviewRecord {
    pub id: Uuid,
    pub special_request_id: Uuid,
    pub reviewer_id: Uuid,
    pub reviewer_name: Option<String>,
    pub conclusion: String,
    pub recommendation: String,
    pub created_at: OffsetDateTime,
}

/// Выборка заключения: общий список столбцов + хвост (см. `acts.rs`).
macro_rules! review_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            ReviewRecord,
            r#"SELECT v.id, v.special_request_id, v.reviewer_id,
                      u.full_name AS "reviewer_name?", v.conclusion,
                      v.recommendation::text AS "recommendation!", v.created_at
               FROM core.special_reviews v
               LEFT JOIN core.users u ON u.id = v.reviewer_id"# + $tail
            $(, $arg)*
        )
    };
}

/// Решение Правления (п. 90).
pub struct DecisionRecord {
    pub id: Uuid,
    pub special_request_id: Uuid,
    pub decision: String,
    pub rationale: String,
    pub decided_by: Uuid,
    pub decided_by_name: Option<String>,
    pub decided_at: OffsetDateTime,
    pub pdf_key: Option<String>,
}

/// Выборка решения: общий список столбцов + хвост (см. `acts.rs`).
macro_rules! decision_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            DecisionRecord,
            r#"SELECT d.id, d.special_request_id, d.decision::text AS "decision!",
                      d.rationale, d.decided_by, u.full_name AS "decided_by_name?",
                      d.decided_at, d.pdf_key
               FROM core.special_board_decisions d
               LEFT JOIN core.users u ON u.id = d.decided_by"# + $tail
            $(, $arg)*
        )
    };
}

pub async fn review_of(db: &Db, request_id: Uuid) -> Result<Option<ReviewRecord>, sqlx::Error> {
    review_query!(" WHERE v.special_request_id = $1", request_id)
        .fetch_optional(db)
        .await
}

pub async fn decision_of(db: &Db, request_id: Uuid) -> Result<Option<DecisionRecord>, sqlx::Error> {
    decision_query!(" WHERE d.special_request_id = $1", request_id)
        .fetch_optional(db)
        .await
}

/// Заключение подразделения (FR-1202, п. 89): выносит заявку на рассмотрение
/// Правления (следствие применяет триггер), закрывает срок проверки и ставит
/// срок решения - 10 рабочих дней Правлению (п. 90).
pub async fn review(
    db: &Db,
    actor: Uuid,
    request_id: Uuid,
    conclusion: &str,
    recommendation: &str,
) -> Result<ReviewRecord, SpecialError> {
    crate::with_actor(db, actor, async |tx| {
        let id = sqlx::query_scalar!(
            "INSERT INTO core.special_reviews
               (special_request_id, reviewer_id, conclusion, recommendation)
             VALUES ($1, $2, $3, $4::text::core.special_decision)
             RETURNING id",
            request_id,
            actor,
            conclusion,
            recommendation
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_rule)?;

        let subject = crate::obligations::Subject::special_request(request_id);
        crate::obligations::complete(&mut *tx, ObligationAction::SpecialReview, subject).await?;
        crate::obligations::schedule(&mut *tx, ObligationAction::SpecialDecision, subject).await?;

        let record = review_query!(" WHERE v.id = $1", id)
            .fetch_one(&mut *tx)
            .await?;
        Ok(record)
    })
    .await
}

/// Решение Правления (FR-1202, п. 90): INV-090 стережет триггер - без
/// заключения подразделения вставка отклоняется. Срок решения закрывается
/// самим фактом решения (FR-1702).
pub async fn decide(
    db: &Db,
    actor: Uuid,
    request_id: Uuid,
    decision: &str,
    rationale: &str,
) -> Result<DecisionRecord, SpecialError> {
    crate::with_actor(db, actor, async |tx| {
        let id = sqlx::query_scalar!(
            "INSERT INTO core.special_board_decisions
               (special_request_id, decision, rationale, decided_by)
             VALUES ($1, $2::text::core.special_decision, $3, $4)
             RETURNING id",
            request_id,
            decision,
            rationale,
            actor
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_rule)?;

        let subject = crate::obligations::Subject::special_request(request_id);
        crate::obligations::complete(&mut *tx, ObligationAction::SpecialDecision, subject).await?;

        // Результат публикуется за пять рабочих дней (п. 97, FR-1403) -
        // но только по публикуемой категории (INV-087, данные справочника)
        let publishable = sqlx::query_scalar!(
            "SELECT c.publishable FROM core.special_requests r
             JOIN refdata.special_categories c ON c.code = r.category
             WHERE r.id = $1",
            request_id
        )
        .fetch_one(&mut *tx)
        .await?;
        if publishable {
            crate::obligations::schedule(&mut *tx, ObligationAction::SpecialPublish, subject)
                .await?;
        }

        let record = decision_query!(" WHERE d.id = $1", id)
            .fetch_one(&mut *tx)
            .await?;
        Ok(record)
    })
    .await
}

/// Печатная форма протокола решения (п. 90): ключ проставляется один раз,
/// само решение остается неизменяемым (триггер `freeze_special_decision`).
pub async fn attach_decision_pdf(
    db: &Db,
    actor: Uuid,
    decision_id: Uuid,
    pdf_key: &str,
) -> Result<(), SpecialError> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query!(
            "UPDATE core.special_board_decisions SET pdf_key = $2 WHERE id = $1",
            decision_id,
            pdf_key
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;
        Ok(())
    })
    .await
}

/// Файлы пачки заявок одним запросом (список кабинета).
pub async fn files_for(db: &Db, request_ids: &[Uuid]) -> Result<Vec<FileRecord>, sqlx::Error> {
    if request_ids.is_empty() {
        return Ok(Vec::new());
    }
    file_query!(
        " WHERE special_request_id = ANY($1) ORDER BY uploaded_at",
        request_ids
    )
    .fetch_all(db)
    .await
}
