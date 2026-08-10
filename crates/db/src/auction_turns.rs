//! Регламент торгов по кругу (М6, FR-604–605): состав круга, очередность
//! хода, выбытие спасовавшего, оглашение предложений отсутствующих.
//!
//! Порядок хода - по журналу регистрации заявок (п. 37–39): единственный
//! законный порядок в процессе. Кто ходит и кто выбыл, решает домен
//! (`domain::turn`), а БД проверяет то же самое триггером - ставка вне
//! очереди не пройдет даже мимо приложения.

use rust_decimal::Decimal;
use tou_domain::ids::ApplicationId;
use tou_domain::turn::{Circle, Participant, ParticipantState, Progress, TurnError};
use uuid::Uuid;

use crate::Db;

#[derive(Debug, thiserror::Error)]
pub enum CircleError {
    #[error("торги не найдены")]
    NotFound,
    /// Правило круга (FR-604–605): не ваш ход, выбыл, не явился, не допущен
    #[error("{0}")]
    Rejected(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

impl From<TurnError> for CircleError {
    fn from(err: TurnError) -> Self {
        CircleError::Rejected(err.to_string())
    }
}

pub struct ParticipantRow {
    pub application_id: Uuid,
    pub applicant_name: String,
    pub turn_order: i32,
    pub status: ParticipantState,
    pub initial_price: Decimal,
}

/// Строка выборки: то же, что [`ParticipantRow`], но `status` - еще текст
/// из БД. Отдельный тип по той же причине, что и в `acts.rs`: `ParticipantState` -
/// доменный тип, и разбирать его макросу пришлось бы через зависимость
/// домена от sqlx (арх. § 5).
struct ParticipantSqlRow {
    application_id: Uuid,
    applicant_name: String,
    turn_order: i32,
    status: String,
    initial_price: Decimal,
}

impl TryFrom<ParticipantSqlRow> for ParticipantRow {
    type Error = sqlx::Error;

    fn try_from(row: ParticipantSqlRow) -> Result<Self, Self::Error> {
        Ok(Self {
            application_id: row.application_id,
            applicant_name: row.applicant_name,
            turn_order: row.turn_order,
            status: row
                .status
                .parse()
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            initial_price: row.initial_price,
        })
    }
}

/// Выборка участника круга: общий список столбцов + хвост (см. `acts.rs`).
///
/// `!` у имени и статуса: `COALESCE` от `jsonb ->> text` и `::text` от
/// перечисления планировщик считает потенциально NULL.
macro_rules! participant_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            ParticipantSqlRow,
            r#"SELECT p.application_id,
                      COALESCE(a.applicant_details->>'name', '-') AS "applicant_name!",
                      p.turn_order, p.status::text AS "status!", p.initial_price
               FROM core.auction_participants p
               JOIN core.applications a ON a.id = p.application_id"# + $tail
            $(, $arg)*
        )
    };
}

pub async fn participants(db: &Db, auction_id: Uuid) -> Result<Vec<ParticipantRow>, sqlx::Error> {
    participant_query!(" WHERE p.auction_id = $1 ORDER BY p.turn_order", auction_id)
        .fetch_all(db)
        .await?
        .into_iter()
        .map(ParticipantRow::try_from)
        .collect()
}

async fn participants_in(
    conn: &mut sqlx::PgConnection,
    auction_id: Uuid,
) -> Result<Vec<ParticipantRow>, sqlx::Error> {
    participant_query!(" WHERE p.auction_id = $1 ORDER BY p.turn_order", auction_id)
        .fetch_all(conn)
        .await?
        .into_iter()
        .map(ParticipantRow::try_from)
        .collect()
}

fn circle_of(rows: &[ParticipantRow]) -> Circle {
    Circle::new(
        rows.iter()
            .map(|row| Participant {
                application_id: ApplicationId::new(row.application_id),
                order: row.turn_order,
                state: row.status,
            })
            .collect(),
    )
}

/// Отметка неявки допущенного участника (FR-605, п. 70): его первоначальное
/// предложение оглашается и попадает в ленту, повышать он не может.
/// Отмечается до конца торгов; повторная отметка ничего не меняет.
pub async fn mark_absent(
    db: &Db,
    actor: Uuid,
    auction_id: Uuid,
    application_id: Uuid,
) -> Result<(), CircleError> {
    crate::with_actor(db, actor, async |tx| {
        let updated = sqlx::query_scalar!(
            "UPDATE core.auction_participants
             SET status = 'absent', changed_at = core.now()
             WHERE auction_id = $1 AND application_id = $2 AND status <> 'absent'
             RETURNING initial_price",
            auction_id,
            application_id
        )
        .fetch_optional(&mut *tx)
        .await?;

        let Some(initial_price) = updated else {
            return Ok(());
        };

        // Оглашение первоначального предложения (п. 70): в ленте оно есть,
        // но шагу торгов не подчиняется - это не повышение
        sqlx::query!(
            "INSERT INTO core.bids (id, auction_id, application_id, amount, announced)
             VALUES (uuidv7(), $1, $2, $3, true)",
            auction_id,
            application_id,
            initial_price
        )
        .execute(&mut *tx)
        .await
        .map_err(map_rule)?;

        // Ход мог принадлежать неявившемуся - передаем дальше
        advance_if_current(&mut *tx, auction_id, application_id).await?;
        Ok(())
    })
    .await
}

fn map_rule(err: sqlx::Error) -> CircleError {
    if let sqlx::Error::Database(db_err) = &err
        && matches!(db_err.code().as_deref(), Some("23514") | Some("P0001"))
    {
        return CircleError::Rejected(db_err.message().to_owned());
    }
    CircleError::Db(err)
}

/// Пас участника (FR-604, п. 65): не готов повысить - выбывает из торгов.
/// Ход переходит следующему; если соперников не осталось, `Progress::Finished`
/// говорит вызывающему, что торги пора завершать.
pub async fn pass(
    db: &Db,
    actor: Uuid,
    auction_id: Uuid,
    application_id: Uuid,
) -> Result<Progress, CircleError> {
    crate::with_actor(db, actor, async |tx| {
        let rows = participants_in(&mut *tx, auction_id).await?;
        if rows.is_empty() {
            return Err(CircleError::NotFound);
        }
        let circle = circle_of(&rows);
        let current = current_turn(&mut *tx, auction_id).await?;

        circle.check_move(ApplicationId::new(application_id), current)?;

        sqlx::query!(
            "UPDATE core.auction_participants
             SET status = 'passed', changed_at = core.now()
             WHERE auction_id = $1 AND application_id = $2",
            auction_id,
            application_id
        )
        .execute(&mut *tx)
        .await?;

        // Круг пересобирается уже без выбывшего
        let rows = participants_in(&mut *tx, auction_id).await?;
        let circle = circle_of(&rows);
        let progress = circle.after_move(ApplicationId::new(application_id));
        set_turn(&mut *tx, auction_id, progress).await?;
        Ok(progress)
    })
    .await
}

/// Ход после принятой ставки (FR-604): очередь идет дальше по кругу.
pub(crate) async fn advance_after_bid(
    tx: &mut sqlx::PgConnection,
    auction_id: Uuid,
    application_id: Uuid,
) -> Result<Progress, sqlx::Error> {
    let rows = participants_in(&mut *tx, auction_id).await?;
    if rows.is_empty() {
        // Круг не собран (торги старого образца) - очередности нет
        return Ok(Progress::Finished);
    }
    let circle = circle_of(&rows);
    let next = circle
        .next_turn(ApplicationId::new(application_id))
        .map_or(Progress::Finished, Progress::Turn);
    set_turn(&mut *tx, auction_id, next).await?;
    Ok(next)
}

/// Передача хода, если он принадлежал этому участнику (неявка).
async fn advance_if_current(
    tx: &mut sqlx::PgConnection,
    auction_id: Uuid,
    application_id: Uuid,
) -> Result<(), sqlx::Error> {
    if current_turn(&mut *tx, auction_id).await? == Some(ApplicationId::new(application_id)) {
        let rows = participants_in(&mut *tx, auction_id).await?;
        let circle = circle_of(&rows);
        let progress = circle.after_move(ApplicationId::new(application_id));
        set_turn(tx, auction_id, progress).await?;
    }
    Ok(())
}

async fn current_turn(
    tx: &mut sqlx::PgConnection,
    auction_id: Uuid,
) -> Result<Option<ApplicationId>, sqlx::Error> {
    let value = sqlx::query_scalar!(
        "SELECT current_turn_application_id FROM core.auctions WHERE id = $1",
        auction_id
    )
    .fetch_optional(tx)
    .await?;
    Ok(value.flatten().map(ApplicationId::new))
}

async fn set_turn(
    tx: &mut sqlx::PgConnection,
    auction_id: Uuid,
    progress: Progress,
) -> Result<(), sqlx::Error> {
    let next = match progress {
        Progress::Turn(id) => Some(id.into_uuid()),
        Progress::Finished => None,
    };
    sqlx::query!(
        "UPDATE core.auctions SET current_turn_application_id = $2 WHERE id = $1",
        auction_id,
        next
    )
    .execute(tx)
    .await
    .map(|_| ())
}

/// Первый ход при старте торгов (FR-604): за участником с наименьшим
/// номером журнала. Если явившихся нет - ход не назначается (п. 71).
pub(crate) async fn open_circle(
    tx: &mut sqlx::PgConnection,
    auction_id: Uuid,
) -> Result<Option<ApplicationId>, sqlx::Error> {
    let rows = participants_in(&mut *tx, auction_id).await?;
    let circle = circle_of(&rows);
    let first = circle.first_turn();
    set_turn(
        tx,
        auction_id,
        first.map_or(Progress::Finished, Progress::Turn),
    )
    .await?;
    Ok(first)
}

/// Остались ли в круге соперники: один активный (или ни одного) - торги
/// пора завершать (п. 65, 71).
pub async fn rivals_remain(db: &Db, auction_id: Uuid) -> Result<bool, sqlx::Error> {
    let mut conn = db.acquire().await?;
    let rows = participants_in(&mut conn, auction_id).await?;
    Ok(circle_of(&rows).active_count() > 1)
}
