//! Двигатель обязательств (М17, FR-1702): постановка сроков на события
//! процесса, закрытие при исполнении, эскалация просрочек.
//!
//! Срок считает БД функцией `refdata.add_business_days` (FR-1701) - та же,
//! что и в домене (`domain::calendar`, паритет проверяет гейт G12): даже
//! если приложение и БД разойдутся во времени, календарь у них один.

use time::OffsetDateTime;
use tou_domain::obligation::{ObligationAction, Term};
use tou_domain::role::Role;
use uuid::Uuid;

use crate::Db;

/// Предмет обязательства: заполняется ровно один (UNIQUE-ключ идемпотентности).
#[derive(Debug, Clone, Copy, Default)]
pub struct Subject {
    pub tender_id: Option<Uuid>,
    pub contract_id: Option<Uuid>,
    pub application_id: Option<Uuid>,
    /// Заявка особого порядка (FR-1202, п. 89–90)
    pub special_request_id: Option<Uuid>,
}

impl Subject {
    pub fn tender(id: Uuid) -> Self {
        Self {
            tender_id: Some(id),
            ..Self::default()
        }
    }

    pub fn contract(id: Uuid) -> Self {
        Self {
            contract_id: Some(id),
            ..Self::default()
        }
    }

    pub fn special_request(id: Uuid) -> Self {
        Self {
            special_request_id: Some(id),
            ..Self::default()
        }
    }
}

pub struct ObligationRecord {
    pub id: Uuid,
    pub action: String,
    pub rule_ref: String,
    pub assignee_role: Role,
    pub tender_id: Option<Uuid>,
    pub tender_title: Option<String>,
    pub due_at: OffsetDateTime,
    pub status: String,
    pub completed_at: Option<OffsetDateTime>,
}

/// Строка выборки: `assignee_role` - еще текст из БД (см. `acts.rs`).
struct ObligationRow {
    id: Uuid,
    action: String,
    rule_ref: String,
    assignee_role: String,
    tender_id: Option<Uuid>,
    tender_title: Option<String>,
    due_at: OffsetDateTime,
    status: String,
    completed_at: Option<OffsetDateTime>,
}

impl TryFrom<ObligationRow> for ObligationRecord {
    type Error = sqlx::Error;

    fn try_from(row: ObligationRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            action: row.action,
            rule_ref: row.rule_ref,
            assignee_role: row
                .assignee_role
                .parse()
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            tender_id: row.tender_id,
            tender_title: row.tender_title,
            due_at: row.due_at,
            status: row.status,
            completed_at: row.completed_at,
        })
    }
}

/// Выборка обязательства: общий список столбцов + хвост (см. `acts.rs`).
///
/// `tender_title` получает `?`: срок может быть не привязан к тендеру, а
/// `core.tenders.title` - NOT NULL, и sqlx выводит nullability по столбцу.
macro_rules! obligation_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            ObligationRow,
            r#"SELECT o.id, o.action, o.rule_ref,
                      o.assignee_role::text AS "assignee_role!",
                      o.tender_id, t.title AS "tender_title?", o.due_at,
                      o.status::text AS "status!", o.completed_at
               FROM core.obligations o
               LEFT JOIN core.tenders t ON t.id = o.tender_id"# + $tail
            $(, $arg)*
        )
    };
}

/// Постановка срока на событие процесса (FR-1702). Идемпотентна: повторное
/// событие по тому же предмету срок не сдвигает и дубля не создает.
///
/// `started_at = core.now()`: событие произошло сейчас - от него и отсчет.
pub async fn schedule(
    tx: &mut sqlx::PgConnection,
    action: ObligationAction,
    subject: Subject,
) -> Result<Option<Uuid>, sqlx::Error> {
    let term = action.rule().term;
    schedule_with_term(tx, action, subject, term).await
}

/// Тот же срок, но заданный данными: категория особого порядка объявляет
/// свой срок проверки (FR-1201), поэтому он приходит из справочника,
/// а не из `rule()`. Пункт Правил и исполнитель остаются от правила.
pub async fn schedule_with_term(
    tx: &mut sqlx::PgConnection,
    action: ObligationAction,
    subject: Subject,
    term: Term,
) -> Result<Option<Uuid>, sqlx::Error> {
    let rule = action.rule();
    // Рабочие дни считает refdata.add_business_days (FR-1701), календарные -
    // обычный интервал. Дни отсчитываются по местному календарю (Asia/Almaty,
    // NFR-03): «рабочий день» в Правилах - казахстанский, а не UTC-сутки.
    // Время суток сохраняется от момента события.
    // Вид срока и число дней уходят параметрами, а не в текст запроса: так
    // SQL остается одной статической строкой (пригодной для `query!`, T46),
    // а число дней проверяется типом, а не форматированием
    let business = matches!(term, Term::BusinessDays(_));
    let days = i32::try_from(term.days()).unwrap_or(i32::MAX);

    sqlx::query_scalar!(
        "INSERT INTO core.obligations
           (rule_ref, action, assignee_role, tender_id, contract_id, application_id,
            special_request_id, due_at, started_at)
         VALUES ($1, $2, $3::text::core.role, $4, $5, $6, $7,
                 CASE WHEN $8
                      THEN ((refdata.add_business_days(
                               (core.now() AT TIME ZONE 'Asia/Almaty')::date, $9)
                             + (core.now() AT TIME ZONE 'Asia/Almaty')::time)
                            AT TIME ZONE 'Asia/Almaty')
                      ELSE core.now() + make_interval(days => $9)
                 END,
                 core.now())
         ON CONFLICT (action, tender_id, contract_id, application_id, special_request_id)
           DO NOTHING
         RETURNING id",
        rule.rule_ref,
        action.as_str(),
        rule.assignee.as_str(),
        subject.tender_id,
        subject.contract_id,
        subject.application_id,
        subject.special_request_id,
        business,
        days
    )
    .fetch_optional(tx)
    .await
}

/// Закрытие обязательства исполнением (FR-1702): вызывается там же, где
/// происходит само действие, - срок закрывает факт, а не отметка человека.
pub async fn complete(
    tx: &mut sqlx::PgConnection,
    action: ObligationAction,
    subject: Subject,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE core.obligations
         SET status = 'done', completed_at = core.now()
         WHERE action = $1 AND status <> 'done'
           AND tender_id IS NOT DISTINCT FROM $2
           AND contract_id IS NOT DISTINCT FROM $3
           AND application_id IS NOT DISTINCT FROM $4
           AND special_request_id IS NOT DISTINCT FROM $5",
        action.as_str(),
        subject.tender_id,
        subject.contract_id,
        subject.application_id,
        subject.special_request_id
    )
    .execute(tx)
    .await
    .map(|_| ())
}

/// Снятие открытых сроков предмета (FR-1702): предмет выбыл из процесса -
/// отзыв заявки особого порядка (п. 88) снимает и срок проверки, и срок
/// решения. Исполненные сроки не трогаются: они уже факт.
pub async fn cancel_for(tx: &mut sqlx::PgConnection, subject: Subject) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE core.obligations
         SET status = 'cancelled'
         WHERE status IN ('pending', 'overdue')
           AND tender_id IS NOT DISTINCT FROM $1
           AND contract_id IS NOT DISTINCT FROM $2
           AND application_id IS NOT DISTINCT FROM $3
           AND special_request_id IS NOT DISTINCT FROM $4",
        subject.tender_id,
        subject.contract_id,
        subject.application_id,
        subject.special_request_id
    )
    .execute(tx)
    .await
    .map(|_| ())
}

/// Закрытие нескольких сроков тендера одним фактом (FR-1702): событие
/// процесса иногда исполняет сразу несколько обязательств - протокол
/// о победителе № 2 и уведомление участника № 2 уходят вместе (п. 117–118).
pub async fn complete_tender(
    db: &Db,
    actor: Uuid,
    actions: &[ObligationAction],
    tender_id: Uuid,
) -> Result<(), sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        for action in actions {
            complete(&mut *tx, *action, Subject::tender(tender_id)).await?;
        }
        Ok(())
    })
    .await
}

/// «Мои сроки» (FR-1702): открытые обязательства ролей пользователя,
/// ближайшие сверху. Исполненные не показываются - их место в аудите.
pub async fn for_roles(db: &Db, roles: &[Role]) -> Result<Vec<ObligationRecord>, sqlx::Error> {
    let names: Vec<String> = roles.iter().map(|r| r.as_str().to_owned()).collect();

    let rows = obligation_query!(
        " WHERE o.assignee_role = ANY($1::text[]::core.role[])
            AND o.status IN ('pending', 'overdue')
          ORDER BY o.due_at LIMIT $2",
        &names,
        crate::MAX_ROWS
    )
    .fetch_all(db)
    .await?;
    crate::warn_if_capped(rows.len(), "obligations::for_roles");
    rows.into_iter().map(ObligationRecord::try_from).collect()
}

/// Просроченное обязательство и получатель эскалации.
pub struct Escalation {
    pub obligation_id: Uuid,
    pub action: String,
    pub rule_ref: String,
    pub tender_id: Option<Uuid>,
    pub tender_title: Option<String>,
    pub due_at: OffsetDateTime,
    pub recipient_id: Uuid,
}

/// Просроченные сроки: перевод в `overdue` и список получателей эскалации
/// (все носители роли-исполнителя). Уведомление о каждом сроке - однократное
/// (`escalated_at`), поэтому воркер можно запускать как угодно часто.
pub async fn take_overdue(db: &Db) -> Result<Vec<Escalation>, sqlx::Error> {
    let mut tx = db.begin().await?;

    // Актор события - система: обязательство просрочилось само, без человека
    //
    // Пачкой, а не всем накопившимся сразу: LIMIT в UPDATE не ставится,
    // поэтому строки отбираются подзапросом. FOR UPDATE SKIP LOCKED -
    // чтобы два экземпляра воркера (NFR-12) разбирали разные пачки,
    // а не ждали друг друга.
    // `!` - столбцы приходят из CTE, а не из таблицы: происхождение
    // планировщик не сообщает и считает их потенциально NULL.
    // `tender_title` наоборот - `?`: он из LEFT JOIN, а `core.tenders.title`
    // NOT NULL, и sqlx вывел бы non-null по самому столбцу
    let rows = sqlx::query_as!(
        Escalation,
        r#"WITH overdue AS (
             UPDATE core.obligations o
             SET status = 'overdue', escalated_at = core.now()
             WHERE o.id IN (
               SELECT id FROM core.obligations
               WHERE status = 'pending' AND due_at < core.now() AND escalated_at IS NULL
               ORDER BY due_at
               LIMIT $1
               FOR UPDATE SKIP LOCKED
             )
             RETURNING o.id, o.action, o.rule_ref, o.assignee_role, o.tender_id, o.due_at
           )
           SELECT overdue.id AS "obligation_id!", overdue.action AS "action!",
                  overdue.rule_ref AS "rule_ref!", overdue.tender_id,
                  t.title AS "tender_title?", overdue.due_at AS "due_at!",
                  rg.user_id AS "recipient_id!"
           FROM overdue
           JOIN core.role_grants rg ON rg.role = overdue.assignee_role
           JOIN core.users u ON u.id = rg.user_id AND u.is_active
           LEFT JOIN core.tenders t ON t.id = overdue.tender_id"#,
        crate::BATCH_ROWS
    )
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(rows)
}

/// Праздники производственного календаря (FR-1701): читает домен для
/// расчетов, правит админ.
pub async fn holidays(db: &Db) -> Result<Vec<(time::Date, String)>, sqlx::Error> {
    let rows = sqlx::query!("SELECT day, label_ru FROM refdata.holidays ORDER BY day")
        .fetch_all(db)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.day, row.label_ru))
        .collect())
}

pub async fn add_holiday(
    db: &Db,
    actor: Uuid,
    day: time::Date,
    label: &str,
) -> Result<(), sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query!(
            "INSERT INTO refdata.holidays (day, label_ru) VALUES ($1, $2)
             ON CONFLICT (day) DO UPDATE SET label_ru = EXCLUDED.label_ru",
            day,
            label
        )
        .execute(&mut *tx)
        .await
        .map(|_| ())
    })
    .await
}

pub async fn remove_holiday(db: &Db, actor: Uuid, day: time::Date) -> Result<bool, sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query!("DELETE FROM refdata.holidays WHERE day = $1", day)
            .execute(&mut *tx)
            .await
            .map(|done| done.rows_affected() > 0)
    })
    .await
}
