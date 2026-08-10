//! Уклонение победителя и участника № 2 (М9, FR-903, FR-505, п. 116–120).
//!
//! Уклонение выводится из фактов конвейера (домен `evasion::Facts`), а его
//! следствия применяет БД: договор прекращается, взнос уклонившегося
//! удерживается проводкой книги (п. 116), а сам он попадает в реестр,
//! который отклоняет его будущие заявки основанием п. 52.4 (FR-505).
//!
//! Здесь остаются сроки: протокол о победителе № 2 за 5 рабочих дней
//! (п. 117) и уведомление № 2 не позднее следующего рабочего дня (п. 118).

use time::OffsetDateTime;
use tou_domain::evasion::{Consequence, EvasionGround, Facts, Place};
use tou_domain::obligation::ObligationAction;
use uuid::Uuid;

use crate::Db;
use crate::obligations::Subject;

#[derive(Debug, thiserror::Error)]
pub enum EvasionError {
    #[error("договор не найден")]
    NotFound,
    /// Правило п. 116–117 (домен) либо отказ БД
    #[error("{0}")]
    Rejected(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

impl From<tou_domain::evasion::EvasionError> for EvasionError {
    fn from(err: tou_domain::evasion::EvasionError) -> Self {
        EvasionError::Rejected(err.to_string())
    }
}

fn map_rule(err: sqlx::Error) -> EvasionError {
    if let sqlx::Error::Database(db_err) = &err
        && matches!(
            db_err.code().as_deref(),
            Some("P0001") | Some("23514") | Some("23503") | Some("23505")
        )
    {
        return EvasionError::Rejected(db_err.message().to_owned());
    }
    EvasionError::Db(err)
}

pub struct EvasionRecord {
    pub id: Uuid,
    pub contract_id: Uuid,
    pub tender_id: Option<Uuid>,
    pub lot_id: Option<Uuid>,
    pub lot_seq: Option<i32>,
    pub user_id: Uuid,
    pub user_name: String,
    pub place: Place,
    pub ground: EvasionGround,
    pub ground_label: String,
    pub note: Option<String>,
    pub declared_at: OffsetDateTime,
}

/// Строка выборки: `place` и `ground` - еще текст из БД (см. `acts.rs`).
struct EvasionRow {
    id: Uuid,
    contract_id: Uuid,
    tender_id: Option<Uuid>,
    lot_id: Option<Uuid>,
    lot_seq: Option<i32>,
    user_id: Uuid,
    user_name: String,
    place: String,
    ground: String,
    ground_label: String,
    note: Option<String>,
    declared_at: OffsetDateTime,
}

impl TryFrom<EvasionRow> for EvasionRecord {
    type Error = sqlx::Error;

    fn try_from(row: EvasionRow) -> Result<Self, Self::Error> {
        // Замыкание тут не годится: у `place` и `ground` разные типы ошибки
        // разбора, а замыкание закрепляется за первым из них
        Ok(Self {
            id: row.id,
            contract_id: row.contract_id,
            tender_id: row.tender_id,
            lot_id: row.lot_id,
            lot_seq: row.lot_seq,
            user_id: row.user_id,
            user_name: row.user_name,
            place: row
                .place
                .parse()
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            ground: row
                .ground
                .parse()
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            ground_label: row.ground_label,
            note: row.note,
            declared_at: row.declared_at,
        })
    }
}

/// Выборка уклонения: общий список столбцов + хвост (см. `acts.rs`).
///
/// `lot_seq` получает `?`: `core.lots.seq` - NOT NULL, и без аннотации sqlx
/// решил бы, что за `LEFT JOIN` NULL прийти не может.
macro_rules! evasion_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            EvasionRow,
            r#"SELECT e.id, e.contract_id, e.tender_id, e.lot_id, l.seq AS "lot_seq?",
                      e.user_id, u.full_name AS user_name, e.place::text AS "place!",
                      e.ground, g.label_ru AS ground_label, e.note, e.declared_at
               FROM core.evasions e
               JOIN core.users u ON u.id = e.user_id
               JOIN refdata.evasion_grounds g ON g.code = e.ground
               LEFT JOIN core.lots l ON l.id = e.lot_id"# + $tail
            $(, $arg)*
        )
    };
}

/// Факты договора глазами п. 116–118: чей договор, передан ли экземпляр,
/// подписан ли он и есть ли в итогах участник № 2.
pub async fn facts(db: &Db, contract_id: Uuid) -> Result<Option<Facts>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT c.place::text AS "place!",
                c.handed_to_tenant_at IS NOT NULL AS "handed_to_tenant!",
                c.tenant_signed_at IS NOT NULL AS "tenant_signed!",
                EXISTS (SELECT 1 FROM core.evasions e WHERE e.contract_id = c.id) AS "declared!",
                COALESCE((SELECT a.runner_up_application_id IS NOT NULL
                          FROM core.auctions a WHERE a.lot_id = c.lot_id), false)
                  AS "runner_up_available!"
         FROM core.contracts c WHERE c.id = $1"#,
        contract_id
    )
    .fetch_optional(db)
    .await?;

    let Some(row) = row else { return Ok(None) };

    Ok(Some(Facts {
        place: row
            .place
            .parse()
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        handed_to_tenant: row.handed_to_tenant,
        tenant_signed: row.tenant_signed,
        declared: row.declared,
        runner_up_available: row.runner_up_available,
    }))
}

/// Признание уклонения (FR-903, п. 116). Домен решает, возможно ли оно
/// и что из него следует; удержание взноса и прекращение договора - БД.
/// Сроки п. 117–118 ставятся тем же событием (FR-1702).
pub async fn declare(
    db: &Db,
    actor: Uuid,
    contract_id: Uuid,
    ground: EvasionGround,
    note: Option<&str>,
) -> Result<(EvasionRecord, Consequence), EvasionError> {
    let facts = facts(db, contract_id)
        .await?
        .ok_or(EvasionError::NotFound)?;
    let consequence = facts.check()?;

    crate::with_actor(db, actor, async |tx| {
        let id = sqlx::query_scalar!(
            "INSERT INTO core.evasions
               (contract_id, tender_id, lot_id, application_id, user_id, place,
                ground, note, declared_by)
             SELECT c.id, c.tender_id, c.lot_id, c.winner_application_id, c.tenant_id,
                    c.place, $2, $3, $4
             FROM core.contracts c WHERE c.id = $1
             RETURNING id",
            contract_id,
            ground.as_str(),
            note,
            actor
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_rule)?;
        let id = id.ok_or(EvasionError::NotFound)?;

        // Сроки уклонения (п. 117–118) идут по тендеру: протокол о победителе
        // № 2 и его уведомление. Если второго места нет, ставить их не на что -
        // тендер идет к признанию несостоявшимся (п. 81.4).
        if consequence == Consequence::OfferToRunnerUp
            && let Some(tender_id) = sqlx::query_scalar!(
                "SELECT tender_id FROM core.contracts WHERE id = $1",
                contract_id
            )
            .fetch_one(&mut *tx)
            .await?
        {
            let subject = Subject::tender(tender_id);
            crate::obligations::schedule(&mut *tx, ObligationAction::Winner2Protocol, subject)
                .await?;
            crate::obligations::schedule(&mut *tx, ObligationAction::NotifyRunnerUp, subject)
                .await?;
        }

        let row = evasion_query!(" WHERE e.id = $1", id)
            .fetch_one(&mut *tx)
            .await
            .map_err(map_rule)?;
        Ok((EvasionRecord::try_from(row).map_err(map_rule)?, consequence))
    })
    .await
}

/// Уклонения тендера (панель организатора и протокол о победителе № 2).
pub async fn list_for_tender(db: &Db, tender_id: Uuid) -> Result<Vec<EvasionRecord>, sqlx::Error> {
    evasion_query!(" WHERE e.tender_id = $1 ORDER BY e.declared_at", tender_id)
        .fetch_all(db)
        .await?
        .into_iter()
        .map(EvasionRecord::try_from)
        .collect()
}

pub struct EvaderRow {
    pub user_id: Uuid,
    pub full_name: String,
    pub evasions: i32,
    pub last_declared_at: Option<OffsetDateTime>,
    pub last_ground: Option<String>,
    pub last_tender_id: Option<Uuid>,
}

/// Реестр уклонистов (FR-505, п. 120): их заявки отклоняются автоматически
/// основанием п. 52.4 - реестр показывает, кого и за что это касается.
pub async fn registry(db: &Db) -> Result<Vec<EvaderRow>, sqlx::Error> {
    // Все столбцы view планировщик считает потенциально NULL
    let rows = sqlx::query_as!(
        EvaderRow,
        r#"SELECT user_id AS "user_id!", full_name AS "full_name!",
                  evasions AS "evasions!", last_declared_at, last_ground, last_tender_id
           FROM core.evader_registry ORDER BY last_declared_at DESC LIMIT $1"#,
        crate::MAX_ROWS
    )
    .fetch_all(db)
    .await?;
    crate::warn_if_capped(rows.len(), "evasion::registry");
    Ok(rows)
}

pub struct GroundRow {
    pub code: String,
    pub label_ru: String,
    pub label_kk: Option<String>,
    pub label_en: Option<String>,
    pub rule_ref: String,
}

/// Закрытый перечень оснований уклонения (п. 116) - для формы секретаря.
pub async fn grounds(db: &Db) -> Result<Vec<GroundRow>, sqlx::Error> {
    sqlx::query_as!(
        GroundRow,
        "SELECT code, label_ru, label_kk, label_en, rule_ref
         FROM refdata.evasion_grounds ORDER BY code"
    )
    .fetch_all(db)
    .await
}

/// Участник № 2 по лоту (п. 74): кому переходит право на договор.
pub struct RunnerUp {
    pub application_id: Uuid,
    pub participant_id: Uuid,
    pub participant_name: String,
    pub amount: rust_decimal::Decimal,
}

pub async fn runner_up_of_lot(db: &Db, lot_id: Uuid) -> Result<Option<RunnerUp>, sqlx::Error> {
    sqlx::query_as!(
        RunnerUp,
        r#"SELECT a.runner_up_application_id AS "application_id!", app.participant_id,
                u.full_name AS participant_name, a.runner_up_amount AS "amount!"
         FROM core.auctions a
         JOIN core.applications app ON app.id = a.runner_up_application_id
         JOIN core.users u ON u.id = app.participant_id
         WHERE a.lot_id = $1"#,
        lot_id
    )
    .fetch_optional(db)
    .await
}
