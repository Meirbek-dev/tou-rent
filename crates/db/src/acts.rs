//! Акты приема-передачи и возврата (М9, FR-904, Прил. 7–8).
//!
//! Составление акта - событие аренды, а не запись в справочнике: передача
//! включает начисление платы с даты акта и делает объект сданным (FR-103),
//! возврат закрывает договор. Следствия применяет триггер БД - они наступают
//! и при вставке мимо приложения.

use time::OffsetDateTime;
use tou_domain::act::{ActKind, ActState};
use tou_domain::obligation::ObligationAction;
use uuid::Uuid;

use crate::Db;

#[derive(Debug, thiserror::Error)]
pub enum ActError {
    #[error("договор не найден")]
    NotFound,
    /// Порядок актов (FR-904) или отказ БД
    #[error("{0}")]
    Rejected(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

impl From<tou_domain::act::ActError> for ActError {
    fn from(err: tou_domain::act::ActError) -> Self {
        ActError::Rejected(err.to_string())
    }
}

fn map_rule(err: sqlx::Error) -> ActError {
    if let sqlx::Error::Database(db_err) = &err
        && matches!(
            db_err.code().as_deref(),
            Some("P0001") | Some("23514") | Some("23503") | Some("23505") | Some("23P01")
        )
    {
        return ActError::Rejected(db_err.message().to_owned());
    }
    ActError::Db(err)
}

pub struct ActRecord {
    pub id: Uuid,
    pub contract_id: Uuid,
    pub kind: ActKind,
    pub act_date: time::Date,
    pub note: Option<String>,
    pub pdf_key: Option<String>,
    pub signed_scan_key: Option<String>,
    /// Способ подписания (ТЗ § 2): выводится из наличия скана триггером
    pub signature_status: String,
    pub created_at: OffsetDateTime,
}

/// Строка выборки: то же, что [`ActRecord`], но `kind` - еще текст из БД.
///
/// Отдельный тип, потому что `query_as!` кладет столбцы в поля как есть,
/// а `ActKind` - доменный тип: чтобы макрос разбирал его сам, домену
/// пришлось бы зависеть от sqlx (арх. § 5). Разбор остается здесь.
struct ActRow {
    id: Uuid,
    contract_id: Uuid,
    kind: String,
    act_date: time::Date,
    note: Option<String>,
    pdf_key: Option<String>,
    signed_scan_key: Option<String>,
    signature_status: String,
    created_at: OffsetDateTime,
}

impl TryFrom<ActRow> for ActRecord {
    type Error = sqlx::Error;

    fn try_from(row: ActRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            contract_id: row.contract_id,
            kind: row
                .kind
                .parse()
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            act_date: row.act_date,
            note: row.note,
            pdf_key: row.pdf_key,
            signed_scan_key: row.signed_scan_key,
            signature_status: row.signature_status,
            created_at: row.created_at,
        })
    }
}

/// Выборка акта: общий список столбцов + хвост запроса.
///
/// Список столбцов остается в одном месте, как и было, но запрос теперь
/// проверяется по схеме: sqlx принимает последовательность строковых
/// литералов через `+`, и `$tail` подставляется literal'ом до разбора.
/// `!` у `kind` и `signature_status` - это `::text`, который планировщик
/// считает потенциально NULL, хотя столбцы NOT NULL.
macro_rules! act_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            ActRow,
            r#"SELECT id, contract_id, kind::text AS "kind!", act_date, note,
                      pdf_key, signed_scan_key,
                      signature_status::text AS "signature_status!", created_at
               FROM core.acts"# + $tail
            $(, $arg)*
        )
    };
}

pub async fn list_for_contract(db: &Db, contract_id: Uuid) -> Result<Vec<ActRecord>, sqlx::Error> {
    act_query!(
        " WHERE contract_id = $1 ORDER BY act_date, created_at",
        contract_id
    )
    .fetch_all(db)
    .await?
    .into_iter()
    .map(ActRecord::try_from)
    .collect()
}

pub async fn get(db: &Db, id: Uuid) -> Result<Option<ActRecord>, sqlx::Error> {
    act_query!(" WHERE id = $1", id)
        .fetch_optional(db)
        .await?
        .map(ActRecord::try_from)
        .transpose()
}

/// Состояние договора глазами актов (FR-904): что уже составлено.
pub async fn state(db: &Db, contract_id: Uuid) -> Result<Option<ActState>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT c.registered_at IS NOT NULL AS "registered!",
                EXISTS (SELECT 1 FROM core.acts a
                        WHERE a.contract_id = c.id AND a.kind = 'handover') AS "handed_over!",
                EXISTS (SELECT 1 FROM core.acts a
                        WHERE a.contract_id = c.id AND a.kind = 'return') AS "returned!"
         FROM core.contracts c WHERE c.id = $1"#,
        contract_id
    )
    .fetch_optional(db)
    .await?;

    let Some(row) = row else { return Ok(None) };
    Ok(Some(ActState {
        registered: row.registered,
        handed_over: row.handed_over,
        returned: row.returned,
    }))
}

/// Составление акта (FR-904). Порядок проверяет домен, следствия - триггер
/// БД: плата начинает начисляться с даты передачи, возврат закрывает договор
/// и освобождает объект (FR-103).
pub async fn create(
    db: &Db,
    actor: Uuid,
    contract_id: Uuid,
    kind: ActKind,
    act_date: time::Date,
    note: Option<&str>,
) -> Result<ActRecord, ActError> {
    let state = state(db, contract_id).await?.ok_or(ActError::NotFound)?;
    state.check(kind)?;

    crate::with_actor(db, actor, async |tx| {
        // `$2::text::core.act_kind`: значение приходит строкой доменного
        // типа, а приведение к перечислению делает БД
        let id = sqlx::query_scalar!(
            "INSERT INTO core.acts (contract_id, kind, act_date, note, created_by)
             VALUES ($1, $2::text::core.act_kind, $3, $4, $5)
             RETURNING id",
            contract_id,
            kind.as_str(),
            act_date,
            note,
            actor
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_rule)?;

        // Возврат объекта открывает срок возврата депозита: пять рабочих
        // дней при отсутствии претензий (FR-1003, п. 136). Передача такого
        // следствия не имеет - там депозит только начинает работать
        if kind == ActKind::Return {
            crate::obligations::schedule(
                &mut *tx,
                ObligationAction::DepositRefund,
                crate::obligations::Subject::contract(contract_id),
            )
            .await?;
        }

        let row = act_query!(" WHERE id = $1", id)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_rule)?;
        ActRecord::try_from(row).map_err(map_rule)
    })
    .await
}

/// Печатная форма акта (Прил. 7–8) в RustFS.
pub async fn attach_pdf(db: &Db, actor: Uuid, id: Uuid, key: &str) -> Result<(), ActError> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query!("UPDATE core.acts SET pdf_key = $2 WHERE id = $1", id, key)
            .execute(&mut *tx)
            .await
            .map(|_| ())
            .map_err(map_rule)
    })
    .await
}

/// Скан подписанного акта (без ЭЦП, как и договор).
pub async fn attach_scan(db: &Db, actor: Uuid, id: Uuid, key: &str) -> Result<(), ActError> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query!(
            "UPDATE core.acts SET signed_scan_key = $2 WHERE id = $1",
            id,
            key
        )
        .execute(&mut *tx)
        .await
        .map(|_| ())
        .map_err(map_rule)
    })
    .await
}
