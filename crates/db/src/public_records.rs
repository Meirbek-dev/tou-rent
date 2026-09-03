//! Публикации особого порядка (М12, М14: FR-1403, FR-1202, п. 90, 92, 97).
//!
//! Публикуются три материала раздела 12: результат рассмотрения заявки
//! с обоснованием, обоснование ставки договора и акт приемки инвестиций.
//! Условия публикации (публикуемость категории, сформированная печатная
//! форма, срок публичного доступа) проверяет и считает БД - здесь запись,
//! чтение и снятие по истечении срока для фонового воркера.

use serde_json::Value;
use time::OffsetDateTime;
use tou_domain::obligation::ObligationAction;
use tou_domain::publication::{PublicRecordKind, PublicationFacts};
use tou_domain::rule::RuleRejection;
use uuid::Uuid;

use crate::Db;
use crate::obligations::Subject;

#[derive(Debug, thiserror::Error)]
pub enum PublicationError {
    #[error("материал не найден")]
    NotFound,
    /// Правило п. 92, 97 (домен) либо отказ БД (FR-1403, INV-076)
    #[error("{0}")]
    Rejected(RuleRejection),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

fn map_rule(err: sqlx::Error) -> PublicationError {
    if let sqlx::Error::Database(db_err) = &err
        && matches!(
            db_err.code().as_deref(),
            Some("P0001") | Some("23514") | Some("23503") | Some("23505")
        )
    {
        return PublicationError::Rejected(crate::rule::rejection(db_err.as_ref()));
    }
    PublicationError::Db(err)
}

pub struct PublicRecord {
    pub id: Uuid,
    pub kind: PublicRecordKind,
    pub special_request_id: Option<Uuid>,
    pub contract_id: Option<Uuid>,
    pub acceptance_id: Option<Uuid>,
    pub title: String,
    /// Тот же заголовок на казахском и английском (NFR-01). `None` -
    /// материал опубликован до появления столбцов либо перевода подписи
    /// категории в справочнике нет
    pub title_kk: Option<String>,
    pub title_en: Option<String>,
    pub file_key: Option<String>,
    /// Обоснование ставки - снимок расчета Прил. 4 (FR-201)
    pub payload: Value,
    pub published_at: OffsetDateTime,
    /// Момент автоматического снятия: публикация + 6 месяцев (INV-076)
    pub unpublish_at: OffsetDateTime,
    pub unpublished_at: Option<OffsetDateTime>,
}

/// Строка выборки: то же, что [`PublicRecord`], но `kind` - еще текст из БД.
///
/// Отдельный тип, потому что `query_as!` кладет столбцы в поля как есть,
/// а `PublicRecordKind` - доменный тип: чтобы макрос разбирал его сам,
/// домену пришлось бы зависеть от sqlx (арх. § 5). Разбор остается здесь.
struct PublicRecordRow {
    id: Uuid,
    kind: String,
    special_request_id: Option<Uuid>,
    contract_id: Option<Uuid>,
    acceptance_id: Option<Uuid>,
    title: String,
    title_kk: Option<String>,
    title_en: Option<String>,
    file_key: Option<String>,
    payload: Value,
    published_at: OffsetDateTime,
    unpublish_at: OffsetDateTime,
    unpublished_at: Option<OffsetDateTime>,
}

impl TryFrom<PublicRecordRow> for PublicRecord {
    type Error = sqlx::Error;

    fn try_from(row: PublicRecordRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            kind: row
                .kind
                .parse()
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            special_request_id: row.special_request_id,
            contract_id: row.contract_id,
            acceptance_id: row.acceptance_id,
            title: row.title,
            title_kk: row.title_kk,
            title_en: row.title_en,
            file_key: row.file_key,
            payload: row.payload,
            published_at: row.published_at,
            unpublish_at: row.unpublish_at,
            unpublished_at: row.unpublished_at,
        })
    }
}

impl PublicRecord {
    /// Состояние публикации в терминах домена (INV-076): материал публичен
    /// от публикации до снятия.
    pub fn facts(&self) -> PublicationFacts {
        PublicationFacts {
            has_pdf: self.file_key.is_some(),
            published: true,
            unpublished: self.unpublished_at.is_some(),
        }
    }
}

/// Выборка публикации: общий список столбцов + хвост запроса (см. `acts.rs`).
///
/// `!` у `kind` - это `::text`, который планировщик считает потенциально
/// NULL, хотя столбец NOT NULL.
macro_rules! record_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            PublicRecordRow,
            r#"SELECT id, kind::text AS "kind!", special_request_id, contract_id,
                      acceptance_id, title, title_kk, title_en, file_key, payload,
                      published_at, unpublish_at, unpublished_at
               FROM core.public_records"# + $tail
            $(, $arg)*
        )
    };
}

pub struct NewPublicRecord<'a> {
    pub kind: PublicRecordKind,
    pub special_request_id: Option<Uuid>,
    pub contract_id: Option<Uuid>,
    pub acceptance_id: Option<Uuid>,
    pub title: &'a str,
    /// Заголовок на казахском и английском, если он собирается из
    /// справочника (NFR-01); `None` - перевода нет
    pub title_kk: Option<&'a str>,
    pub title_en: Option<&'a str>,
    pub file_key: Option<&'a str>,
    pub payload: Value,
}

/// Публикация материала на портале (FR-1403, п. 97). Условия и срок
/// публичного доступа проверяет БД; срок публикации результата (5 рабочих
/// дней, п. 97) закрывается самим фактом публикации (FR-1702).
pub async fn publish(
    db: &Db,
    actor: Uuid,
    new: NewPublicRecord<'_>,
) -> Result<PublicRecord, PublicationError> {
    crate::with_actor(db, actor, async |tx| {
        // `$1::text::core.public_record_kind`: значение приходит строкой
        // доменного типа, а приведение к перечислению делает БД
        let id = sqlx::query_scalar!(
            "INSERT INTO core.public_records
               (kind, special_request_id, contract_id, acceptance_id,
                title, title_kk, title_en, file_key, payload, published_by)
             VALUES ($1::text::core.public_record_kind, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             RETURNING id",
            new.kind.as_str(),
            new.special_request_id,
            new.contract_id,
            new.acceptance_id,
            new.title,
            new.title_kk,
            new.title_en,
            new.file_key,
            new.payload,
            actor
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_rule)?;

        if let Some(request_id) = new.special_request_id {
            crate::obligations::complete(
                &mut *tx,
                ObligationAction::SpecialPublish,
                Subject::special_request(request_id),
            )
            .await?;
        }

        let row = record_query!(" WHERE id = $1", id)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_rule)?;
        PublicRecord::try_from(row).map_err(map_rule)
    })
    .await
}

/// Реестр портала (FR-1403): материалы, доступные публично сейчас.
///
/// Публикаций становится больше с каждым решением Правления, поэтому
/// реестр идет курсором по паре «дата публикации + материал»: за один
/// день их выкладывают пачкой, и одной даты для ключа мало.
pub async fn list_public(
    db: &Db,
    after: Option<crate::RowCursor>,
    limit: i64,
) -> Result<crate::Page<PublicRecord>, sqlx::Error> {
    let (after_at, after_id) = crate::RowCursor::parts(after);
    let rows = record_query!(
        " WHERE unpublished_at IS NULL
            AND ($1::timestamptz IS NULL OR (published_at, id) < ($1, $2::uuid))
          ORDER BY published_at DESC, id DESC LIMIT $3",
        after_at,
        after_id,
        crate::probe_limit(limit)
    )
    .fetch_all(db)
    .await?;
    let page = crate::Page::probe(rows, limit);
    crate::warn_if_truncated(page.truncated, "public_records::list_public");
    page.try_map(PublicRecord::try_from)
}

pub async fn get(db: &Db, id: Uuid) -> Result<Option<PublicRecord>, sqlx::Error> {
    record_query!(" WHERE id = $1", id)
        .fetch_optional(db)
        .await?
        .map(PublicRecord::try_from)
        .transpose()
}

/// Публикации по заявке особого порядка: результат, обоснование ставки
/// ее договора и акты приемки - все, что относится к одному решению.
pub async fn list_for_request(db: &Db, request_id: Uuid) -> Result<Vec<PublicRecord>, sqlx::Error> {
    record_query!(
        " p
         WHERE p.special_request_id = $1
            OR p.contract_id IN (SELECT i.contract_id FROM core.investment_contracts i
                                 WHERE i.special_request_id = $1)
            OR p.acceptance_id IN (SELECT a.id FROM core.investment_acceptances a
                                   JOIN core.investment_contracts i ON i.contract_id = a.contract_id
                                   WHERE i.special_request_id = $1)
         ORDER BY p.published_at DESC",
        request_id
    )
    .fetch_all(db)
    .await?
    .into_iter()
    .map(PublicRecord::try_from)
    .collect()
}

/// Материал, который Правила велят опубликовать, а публикации еще нет
/// (FR-1403): рабочий список уполномоченного подразделения.
pub struct PendingPublication {
    pub kind: PublicRecordKind,
    /// Заявка, договор или акт - предмет будущей публикации
    pub source_id: Uuid,
    pub title: String,
    /// Наименование объекта, о котором идет материал
    ///
    /// Круг 1 гаунтлета (G-134): заголовок вида «Обоснование ставки договора
    /// особого порядка» одинаков у всех таких материалов, и очередь из
    /// нескольких строк не давала различить, какой договор публикуется.
    /// Предмет теперь стоит рядом с заголовком отдельными полями, а не
    /// вплавляется в него: строка заголовка идет в публикацию как есть
    /// (п. 97) и переводится справочником, а предмет нужен работающему
    /// с очередью, а не порталу.
    pub object_name: Option<String>,
    /// Заявитель (решение) либо наниматель (договор, акт приемки)
    pub party_name: Option<String>,
    /// Регистрационный номер договора, если он присвоен (п. 126)
    pub reference: Option<String>,
    /// Событие, породившее обязанность публикации (решение, договор, акт)
    pub occurred_at: OffsetDateTime,
    /// Материал готов к публикации: печатная форма или расчет есть
    pub ready: bool,
}

/// Строка выборки: `kind` - еще текст из БД (см. [`PublicRecordRow`]).
struct PendingRow {
    kind: String,
    source_id: Uuid,
    title: String,
    object_name: Option<String>,
    party_name: Option<String>,
    reference: Option<String>,
    occurred_at: OffsetDateTime,
    ready: bool,
}

impl TryFrom<PendingRow> for PendingPublication {
    type Error = sqlx::Error;

    fn try_from(row: PendingRow) -> Result<Self, Self::Error> {
        Ok(Self {
            kind: row
                .kind
                .parse()
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            source_id: row.source_id,
            title: row.title,
            object_name: row.object_name,
            party_name: row.party_name,
            reference: row.reference,
            occurred_at: row.occurred_at,
            ready: row.ready,
        })
    }
}

/// Что ждет публикации: решения по публикуемым категориям (п. 87, 97),
/// обоснования ставок договоров особого порядка и акты приемки (п. 92).
///
/// Старое сверху: рабочий список подразделения разгружается работой, и
/// первым в нем должен стоять материал, ждущий дольше всех. Курсор - id
/// материала (`source_id`); ключ сортировки (`occurred_at`, `source_id`)
/// БД находит по нему в той же выборке, поэтому состав ключа остается
/// внутри запроса, а объединение трех источников не мешает листать.
pub async fn pending<'e, E>(
    db: E,
    after: Option<Uuid>,
    limit: i64,
) -> Result<crate::Page<PendingPublication>, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    // Столбцы под UNION планировщик считает потенциально NULL
    let rows = sqlx::query_as!(
        PendingRow,
        r#"WITH pending AS (
           SELECT 'decision'::text AS kind, r.id AS source_id,
                'Результат по заявке: ' || c.label_ru || ' (' || c.rule_ref || ')' AS title,
                o.name AS object_name,
                COALESCE(NULLIF(btrim(r.applicant_details ->> 'name'), ''), u.full_name)
                  AS party_name,
                NULL::text AS reference,
                d.decided_at AS occurred_at, d.pdf_key IS NOT NULL AS ready
         FROM core.special_board_decisions d
         JOIN core.special_requests r ON r.id = d.special_request_id
         JOIN refdata.special_categories c ON c.code = r.category
         JOIN core.users u ON u.id = r.applicant_id
         LEFT JOIN core.objects o ON o.id = r.object_id
         WHERE c.publishable
           AND NOT EXISTS (SELECT 1 FROM core.public_records p
                           WHERE p.special_request_id = r.id)
         UNION ALL
         SELECT 'rate', i.contract_id,
                'Обоснование ставки договора особого порядка',
                o.name, u.full_name, ct.reg_number, i.created_at,
                i.rate_calculation IS NOT NULL
         FROM core.investment_contracts i
         JOIN core.contracts ct ON ct.id = i.contract_id
         JOIN core.users u ON u.id = ct.tenant_id
         JOIN core.objects o ON o.id = ct.object_id
         WHERE NOT EXISTS (SELECT 1 FROM core.public_records p
                           WHERE p.contract_id = i.contract_id)
         UNION ALL
         SELECT 'investment_act', a.id,
                'Акт приемки инвестиций от ' || to_char(a.act_date, 'DD.MM.YYYY'),
                o.name, u.full_name, ct.reg_number,
                a.created_at, a.pdf_key IS NOT NULL
         FROM core.investment_acceptances a
         JOIN core.contracts ct ON ct.id = a.contract_id
         JOIN core.users u ON u.id = ct.tenant_id
         JOIN core.objects o ON o.id = ct.object_id
         WHERE NOT EXISTS (SELECT 1 FROM core.public_records p
                           WHERE p.acceptance_id = a.id)
         )
         SELECT kind AS "kind!", source_id AS "source_id!", title AS "title!",
                object_name AS "object_name?", party_name AS "party_name?",
                reference AS "reference?",
                occurred_at AS "occurred_at!", ready AS "ready!"
         FROM pending
         WHERE $1::uuid IS NULL
            OR (occurred_at, source_id)
               > (SELECT occurred_at, source_id FROM pending WHERE source_id = $1 LIMIT 1)
         ORDER BY occurred_at, source_id LIMIT $2"#,
        after,
        crate::probe_limit(limit)
    )
    .fetch_all(db)
    .await?;
    // См. `contracts::list_for_tenant`: с курсором `truncated` - признак
    // продолжения, а не упершейся в потолок выборки
    crate::Page::probe(rows, limit).try_map(PendingPublication::try_from)
}

/// Снятие материалов с публичного доступа по истечении шести месяцев
/// (INV-076, п. 76) - работа фонового воркера. Материал остается в досье
/// решения: запись туда делает триггер БД.
pub async fn take_expired(db: &Db) -> Result<Vec<PublicRecord>, sqlx::Error> {
    let mut tx = db.begin().await?;

    let ids = sqlx::query_scalar!(
        // Пачкой - см. `publications::take_expired`
        "UPDATE core.public_records
         SET unpublished_at = core.now()
         WHERE id IN (
           SELECT id FROM core.public_records
           WHERE unpublished_at IS NULL AND unpublish_at <= core.now()
           ORDER BY unpublish_at
           LIMIT $1
           FOR UPDATE SKIP LOCKED
         )
         RETURNING id",
        crate::BATCH_ROWS
    )
    .fetch_all(&mut *tx)
    .await?;

    // Один запрос на весь пакет - см. `publications::take_expired`
    let taken = record_query!(" WHERE id = ANY($1)", &ids)
        .fetch_all(&mut *tx)
        .await?;

    tx.commit().await?;
    taken.into_iter().map(PublicRecord::try_from).collect()
}
