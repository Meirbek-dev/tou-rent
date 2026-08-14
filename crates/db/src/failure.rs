//! Несостоявшийся тендер (М8, FR-801–802): факты процесса, признание по
//! основанию п. 81 и следствие п. 82–83.
//!
//! Основание не выбирается человеком - оно выводится из данных тендера
//! (сколько заявок подано, сколько допущено, уклонились ли победители)
//! и проверяется повторно при записи: БД не примет `failed` без основания.

use time::OffsetDateTime;
use tou_domain::failure::{Consequence, Facts, FailureGround};
use tou_domain::obligation::ObligationAction;
use tou_domain::rule::{RuleRejection, RuleViolation};
use uuid::Uuid;

use crate::Db;

#[derive(Debug, thiserror::Error)]
pub enum FailureError {
    #[error("тендер не найден")]
    NotFound,
    /// Основание п. 81 не наступило либо переход запрещен (INV-021)
    #[error("{0}")]
    Rejected(RuleRejection),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

fn map_rule(err: sqlx::Error) -> FailureError {
    if let sqlx::Error::Database(db_err) = &err
        && matches!(
            db_err.code().as_deref(),
            Some("P0001") | Some("23514") | Some("23503")
        )
    {
        return FailureError::Rejected(crate::rule::rejection(db_err.as_ref()));
    }
    FailureError::Db(err)
}

/// Состояние тендера глазами п. 81–83.
pub struct FailureState {
    pub facts: Facts,
    /// Наступившее основание (если наступило)
    pub ground: Option<FailureGround>,
    /// Следствие при текущих фактах (если основание есть)
    pub consequence: Option<Consequence>,
    /// Сколько несостоявшихся тендеров уже было в этой цепочке повторов
    pub previous_failures: usize,
    /// Уже признан несостоявшимся
    pub failed: bool,
}

/// Факты тендера для п. 81: заявки без отозванных, допущенные, состоялось
/// ли вскрытие, уклонились ли победитель и второе место.
pub async fn state(db: &Db, tender_id: Uuid) -> Result<Option<FailureState>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT
           (SELECT count(*) FROM core.applications a
            WHERE a.tender_id = t.id AND a.status <> 'withdrawn')::bigint AS "applications!",
           (SELECT count(*) FROM core.applications a
            WHERE a.tender_id = t.id AND a.status = 'admitted')::bigint AS "admitted!",
           t.opened_at IS NOT NULL AS "opened!",
           (t.submission_deadline IS NOT NULL AND t.submission_deadline < core.now()) AS "deadline_passed!",
           t.status::text AS "status!",
           t.failure_ground,
           t.repeat_of,
           -- Лоты с завершенными торгами и лоты, где договориться больше не с кем:
           -- победитель уклонился, а участника № 2 нет либо уклонился и он (п. 81.4)
           (SELECT count(*) FROM core.auctions a
            JOIN core.lots l ON l.id = a.lot_id
            WHERE l.tender_id = t.id AND a.status = 'finished')::bigint AS "finished_lots!",
           (SELECT count(*) FROM core.auctions a
            JOIN core.lots l ON l.id = a.lot_id
            WHERE l.tender_id = t.id AND a.status = 'finished'
              AND EXISTS (SELECT 1 FROM core.evasions e
                          WHERE e.lot_id = l.id AND e.place = 'winner')
              AND (a.runner_up_application_id IS NULL
                   OR EXISTS (SELECT 1 FROM core.evasions e
                              WHERE e.lot_id = l.id AND e.place = 'runner_up'))
           )::bigint AS "exhausted_lots!"
         FROM core.tenders t WHERE t.id = $1"#,
        tender_id
    )
    .fetch_optional(db)
    .await?;

    let Some(row) = row else { return Ok(None) };

    let applications = row.applications;
    let admitted = row.admitted;
    let opened = row.opened;
    let deadline_passed = row.deadline_passed;
    let status = row.status;
    let ground = row.failure_ground;
    let repeat_of = row.repeat_of;
    let finished_lots = row.finished_lots;
    let exhausted_lots = row.exhausted_lots;

    // Уклонение победителя и № 2 (п. 116–118, FR-903): основание п. 81.4
    // наступает, когда по каждому разыгранному лоту договариваться больше
    // не с кем - иначе договор еще идет с участником № 2 (A-055)
    let facts = Facts {
        applications: applications.max(0) as usize,
        admitted: admitted.max(0) as usize,
        deadline_passed,
        opened,
        winners_evaded: finished_lots > 0 && exhausted_lots == finished_lots,
    };

    let previous_failures = failures_in_chain(db, repeat_of).await?;
    let detected = facts.ground();

    Ok(Some(FailureState {
        facts,
        ground: detected,
        consequence: detected.map(|ground| Consequence::of(ground, facts, previous_failures)),
        previous_failures,
        failed: status == "failed" || ground.is_some(),
    }))
}

/// Сколько несостоявшихся тендеров в цепочке повторов до этого (п. 83).
async fn failures_in_chain(db: &Db, mut previous: Option<Uuid>) -> Result<usize, sqlx::Error> {
    let mut count = 0;
    // Цепочка коротка по смыслу процесса; ограничение - страховка от цикла
    for _ in 0..16 {
        let Some(id) = previous else { break };
        let row = sqlx::query!(
            "SELECT failure_ground, repeat_of FROM core.tenders WHERE id = $1",
            id
        )
        .fetch_optional(db)
        .await?;
        let Some(row) = row else { break };
        let (ground, parent) = (row.failure_ground, row.repeat_of);
        if ground.is_some() {
            count += 1;
        }
        previous = parent;
    }
    Ok(count)
}

/// Признание несостоявшимся (FR-801). Основание берется из фактов, а не от
/// вызывающего: если оно не наступило, переход отклоняется. Срок протокола
/// (п. 82) ставится тем же событием.
pub async fn declare_failed(
    db: &Db,
    actor: Uuid,
    tender_id: Uuid,
) -> Result<FailureGround, FailureError> {
    let state = state(db, tender_id).await?.ok_or(FailureError::NotFound)?;
    let ground = state.ground.ok_or_else(|| {
        FailureError::Rejected(RuleRejection::new(
            RuleViolation::TenderFailureGround,
            format!(
                "основание п. 81 не наступило: заявок {}, допущено {} - тендер несостоявшимся \
                 не признается (FR-801)",
                state.facts.applications, state.facts.admitted
            ),
        ))
    })?;
    let consequence = state.consequence.unwrap_or(Consequence::Repeat);

    crate::with_actor(db, actor, async |tx| {
        // `$3::text::...`: доменное значение приходит строкой, приведение
        // к перечислению делает БД
        let updated = sqlx::query_scalar!(
            "UPDATE core.tenders
             SET status = 'failed', failure_ground = $2,
                 consequence = $3::text::core.failure_consequence,
                 failed_at = core.now()
             WHERE id = $1 AND status <> 'failed'
             RETURNING id",
            tender_id,
            ground.as_str(),
            consequence.as_str()
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_rule)?;

        if updated.is_none() {
            return Err(FailureError::Rejected(RuleRejection::new(
                RuleViolation::TenderStatusTransition,
                "тендер уже признан несостоявшимся либо переход из текущего статуса запрещен \
                 (INV-021)",
            )));
        }

        // Протокол о несостоявшемся - за три рабочих дня (FR-1702, п. 82)
        crate::obligations::schedule(
            &mut *tx,
            ObligationAction::FailedProtocol,
            crate::obligations::Subject::tender(tender_id),
        )
        .await?;

        Ok(ground)
    })
    .await
}

pub struct GroundRow {
    pub code: String,
    pub label_ru: String,
    pub label_kk: Option<String>,
    pub label_en: Option<String>,
    pub rule_ref: String,
}

/// Закрытый перечень оснований п. 81 (для протокола и интерфейса).
pub async fn grounds(db: &Db) -> Result<Vec<GroundRow>, sqlx::Error> {
    sqlx::query_as!(
        GroundRow,
        "SELECT code, label_ru, label_kk, label_en, rule_ref
         FROM refdata.failure_grounds ORDER BY rule_ref"
    )
    .fetch_all(db)
    .await
}

pub struct FailedTenderRow {
    pub id: Uuid,
    pub title: String,
    pub failure_ground: Option<String>,
    pub consequence: Option<String>,
    pub failed_at: Option<OffsetDateTime>,
}

/// Повторный тендер (п. 82): новый черновик со ссылкой на несостоявшийся,
/// лоты копируются со снимками ставок. Возможен один раз - второй повтор
/// означал бы третью попытку в обход п. 83.
pub async fn repeat_tender(db: &Db, actor: Uuid, tender_id: Uuid) -> Result<Uuid, FailureError> {
    let state = state(db, tender_id).await?.ok_or(FailureError::NotFound)?;
    if !state.failed {
        return Err(FailureError::Rejected(RuleRejection::new(
            RuleViolation::TenderFailureGround,
            "повторный тендер объявляется только после признания несостоявшимся (п. 82)",
        )));
    }
    if matches!(state.consequence, Some(Consequence::BoardReferral)) {
        return Err(FailureError::Rejected(RuleRejection::new(
            RuleViolation::TenderFailureGround,
            "тендер не состоялся дважды - вопрос передается Правлению, а не на повтор (п. 83)",
        )));
    }

    crate::with_actor(db, actor, async |tx| {
        let existing = sqlx::query_scalar!(
            "SELECT id FROM core.tenders WHERE repeat_of = $1",
            tender_id
        )
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(id) = existing {
            return Ok(id);
        }

        let new_id = sqlx::query_scalar!(
            "INSERT INTO core.tenders (title, status, organizer_id, repeat_of)
             SELECT t.title || ' (повторный)', 'draft', t.organizer_id, t.id
             FROM core.tenders t WHERE t.id = $1
             RETURNING id",
            tender_id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_rule)?;
        let new_id = new_id.ok_or(FailureError::NotFound)?;

        // Лоты переносятся со снимком расчета: условия повторного тендера
        // те же, пока организатор не изменит их в черновике (п. 82)
        sqlx::query!(
            "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                    base_rate_monthly, guarantee_fee, rate_calculation,
                                    viewing_terms, rate_unit, hours_total)
             SELECT $2, l.seq, l.object_id, l.purpose, l.lease_months,
                    l.base_rate_monthly, l.guarantee_fee, l.rate_calculation,
                    l.viewing_terms, l.rate_unit, l.hours_total
             FROM core.lots l WHERE l.tender_id = $1",
            tender_id,
            new_id
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;

        Ok(new_id)
    })
    .await
}
