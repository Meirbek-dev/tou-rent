//! Онлайн-торги по лоту (М6, FR-601–603, 606).
//!
//! Сервер - единственный источник времени и порядка ставок (NFR-03): `placed_at`
//! проставляет триггер, тотальный порядок дает `core.bids.seq`. Правила шага
//! (INV-063) и таймера (INV-066) продублированы в БД - она последний рубеж,
//! этот слой лишь переводит отказы в типизированные ошибки.

use rust_decimal::Decimal;
use time::OffsetDateTime;
use tou_domain::auction::{self, Outcome};
use tou_domain::ids::ApplicationId;
use tou_domain::money::Money;
use tou_domain::obligation::ObligationAction;
use uuid::Uuid;

use crate::Db;

pub struct AuctionRecord {
    pub id: Uuid,
    pub lot_id: Uuid,
    pub tender_id: Uuid,
    pub lot_seq: i32,
    pub lot_purpose: String,
    /// `scheduled` | `running` | `finished` | `cancelled` (`core.auction_status`)
    pub status: String,
    pub starting_bid: Decimal,
    pub bid_step: Decimal,
    pub started_at: Option<OffsetDateTime>,
    /// Момент окончания по часам сервера (FR-602); клиент только отображает
    pub ends_at: Option<OffsetDateTime>,
    pub extended_once: bool,
    pub finished_at: Option<OffsetDateTime>,
    pub finished_early: bool,
    pub winner_application_id: Option<Uuid>,
    pub winner_amount: Option<Decimal>,
    pub runner_up_application_id: Option<Uuid>,
    pub runner_up_amount: Option<Decimal>,
    /// Чей сейчас ход в круге (FR-604); None - круг не начат либо окончен
    pub current_turn_application_id: Option<Uuid>,
}

/// Выборка комнаты: общий список столбцов + хвост запроса (см. `acts.rs`).
///
/// `!` у `status` - это `::text`, который планировщик считает потенциально
/// NULL, хотя столбец NOT NULL.
macro_rules! auction_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            AuctionRecord,
            r#"SELECT a.id, a.lot_id, l.tender_id, l.seq AS lot_seq, l.purpose AS lot_purpose,
                      a.status::text AS "status!", a.starting_bid, a.bid_step,
                      a.started_at, a.ends_at, a.extended_once, a.finished_at, a.finished_early,
                      a.winner_application_id, a.winner_amount,
                      a.runner_up_application_id, a.runner_up_amount,
                      a.current_turn_application_id
               FROM core.auctions a
               JOIN core.lots l ON l.id = a.lot_id"# + $tail
            $(, $arg)*
        )
    };
}

pub struct BidRecord {
    pub id: Uuid,
    pub auction_id: Uuid,
    pub application_id: Uuid,
    /// Имя заявителя из снимка заявки - лента озвучивается секретарем (п. 65)
    pub applicant_name: String,
    pub amount: Decimal,
    /// Порядковый номер ставки, назначенный сервером
    pub seq: i64,
    pub placed_at: OffsetDateTime,
}

/// Выборка ставки: общий список столбцов + хвост запроса.
///
/// `!` у имени: `COALESCE` от `jsonb ->> text` планировщик считает
/// потенциально NULL, хотя второй аргумент - литерал.
macro_rules! bid_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            BidRecord,
            r#"SELECT b.id, b.auction_id, b.application_id,
                      COALESCE(app.applicant_details->>'name', '-') AS "applicant_name!",
                      b.amount, b.seq, b.placed_at
               FROM core.bids b
               JOIN core.applications app ON app.id = b.application_id"# + $tail
            $(, $arg)*
        )
    };
}

#[derive(Debug, thiserror::Error)]
pub enum ScheduleError {
    #[error("лот не найден")]
    LotNotFound,
    /// Стартовую ставку не из чего вычислить (п. 62): нет допущенных с ценой
    #[error("по лоту нет допущенных заявок с предложением цены")]
    NoAdmittedBids,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Комната лота (FR-601): стартовая ставка = максимум первоначальных
/// предложений допущенных (INV-062, п. 62), шаг = 5 % от нее (п. 63) -
/// оба значения фиксируются один раз, повторный вызов возвращает готовую.
pub async fn schedule_for_lot(
    db: &Db,
    actor: Uuid,
    lot_id: Uuid,
) -> Result<AuctionRecord, ScheduleError> {
    crate::with_actor(db, actor, async |tx| {
        // Гонка двух «открыть торги» по одному лоту сериализуется здесь,
        // UNIQUE(lot_id) остается страховкой
        sqlx::query!(
            "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
            format!("auction:{lot_id}")
        )
        .fetch_one(&mut *tx)
        .await?;

        if let Some(existing) = fetch_by_lot(&mut *tx, lot_id).await? {
            return Ok(existing);
        }

        let lot_exists = sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM core.lots WHERE id = $1) AS "exists!""#,
            lot_id
        )
        .fetch_one(&mut *tx)
        .await?;
        if !lot_exists {
            return Err(ScheduleError::LotNotFound);
        }

        let starting_bid = sqlx::query_scalar!(
            "SELECT max(core.price_amount(p))
             FROM core.applications a
             JOIN core.price_proposals p ON p.application_id = a.id
             WHERE a.lot_id = $1 AND a.status = 'admitted'",
            lot_id
        )
        .fetch_one(&mut *tx)
        .await?;
        let starting_bid = starting_bid.ok_or(ScheduleError::NoAdmittedBids)?;
        let step = auction::bid_step(Money::new(starting_bid)).amount();

        let id = sqlx::query_scalar!(
            "INSERT INTO core.auctions (lot_id, starting_bid, bid_step)
             VALUES ($1, $2, $3) RETURNING id",
            lot_id,
            starting_bid,
            step
        )
        .fetch_one(&mut *tx)
        .await?;

        // Состав круга (FR-604): допущенные заявки в порядке журнала
        sqlx::query!(
            "INSERT INTO core.auction_participants
               (auction_id, application_id, turn_order, initial_price)
             SELECT $1, a.id,
                    COALESCE(j.seq, 1000 + row_number() OVER (ORDER BY a.submitted_at))::int,
                    core.price_amount(p) AS amount
             FROM core.applications a
             JOIN core.price_proposals p ON p.application_id = a.id
             LEFT JOIN core.journal_entries j
               ON j.application_id = a.id AND j.entry_kind = 'application_submitted'
             WHERE a.lot_id = $2 AND a.status = 'admitted'
             ON CONFLICT (auction_id, application_id) DO NOTHING",
            id,
            lot_id
        )
        .execute(&mut *tx)
        .await?;

        fetch(&mut *tx, id).await?.ok_or(ScheduleError::LotNotFound)
    })
    .await
}

async fn fetch(
    conn: &mut sqlx::PgConnection,
    id: Uuid,
) -> Result<Option<AuctionRecord>, sqlx::Error> {
    auction_query!(" WHERE a.id = $1", id)
        .fetch_optional(conn)
        .await
}

async fn fetch_by_lot(
    conn: &mut sqlx::PgConnection,
    lot_id: Uuid,
) -> Result<Option<AuctionRecord>, sqlx::Error> {
    auction_query!(" WHERE a.lot_id = $1", lot_id)
        .fetch_optional(conn)
        .await
}

pub async fn get(db: &Db, id: Uuid) -> Result<Option<AuctionRecord>, sqlx::Error> {
    let mut conn = db.acquire().await?;
    fetch(&mut conn, id).await
}

pub async fn by_lot(db: &Db, lot_id: Uuid) -> Result<Option<AuctionRecord>, sqlx::Error> {
    let mut conn = db.acquire().await?;
    fetch_by_lot(&mut conn, lot_id).await
}

/// Торги всех лотов тендера - состояние для кабинета секретаря.
pub async fn list_for_tender(db: &Db, tender_id: Uuid) -> Result<Vec<AuctionRecord>, sqlx::Error> {
    auction_query!(" WHERE l.tender_id = $1 ORDER BY l.seq", tender_id)
        .fetch_all(db)
        .await
}

/// Лента ставок в серверном порядке; `after_seq` - курсор дочитывания
/// после реконнекта (NFR-05).
pub async fn bids_of(
    db: &Db,
    auction_id: Uuid,
    after_seq: Option<i64>,
) -> Result<Vec<BidRecord>, sqlx::Error> {
    bid_query!(
        " WHERE b.auction_id = $1 AND ($2::bigint IS NULL OR b.seq > $2)
          ORDER BY b.seq LIMIT $3",
        auction_id,
        after_seq,
        crate::MAX_ROWS
    )
    .fetch_all(db)
    .await
}

/// Допущенная заявка участника на лот - право торговаться (п. 62).
pub async fn admitted_application_of(
    db: &Db,
    participant_id: Uuid,
    lot_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM core.applications
         WHERE participant_id = $1 AND lot_id = $2 AND status = 'admitted'",
        participant_id,
        lot_id
    )
    .fetch_optional(db)
    .await
}

#[derive(Debug, thiserror::Error)]
pub enum TransitionError {
    #[error("торги не найдены")]
    NotFound,
    /// Отказ БД (INV-066) или неподходящий статус комнаты
    #[error("{0}")]
    Rejected(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Старт торгов председателем (FR-602): таймер `duration_minutes` от объявления;
/// в демо-режиме длительность задается параметром запуска api.
pub async fn start(
    db: &Db,
    actor: Uuid,
    id: Uuid,
    duration_minutes: i64,
) -> Result<AuctionRecord, TransitionError> {
    crate::with_actor(db, actor, async |tx| {
        let updated = sqlx::query_scalar!(
            "UPDATE core.auctions
             SET status = 'running', started_at = core.now(),
                 ends_at = core.now() + make_interval(mins => $2)
             WHERE id = $1 AND status = 'scheduled'
             RETURNING id",
            id,
            i32::try_from(duration_minutes).unwrap_or(i32::MAX)
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_rule_violation)?;

        match updated {
            Some(id) => {
                // Круг торгов (FR-604): первый ход - за участником с
                // наименьшим номером журнала регистрации
                crate::auction_turns::open_circle(&mut *tx, id).await?;
                require(&mut *tx, id).await
            }
            None => Err(status_conflict(&mut *tx, id, "старт возможен из статуса scheduled").await),
        }
    })
    .await
}

/// Продление таймера решением председателя (FR-602): ровно 15 минут и только
/// один раз - оба правила стережет триггер `enforce_auction_extension`.
pub async fn extend(db: &Db, actor: Uuid, id: Uuid) -> Result<AuctionRecord, TransitionError> {
    crate::with_actor(db, actor, async |tx| {
        // Продление идет параметром, а не в тексте запроса: значение
        // доменное (INV-066), и подставлять его форматированием незачем -
        // так SQL остается литералом (T46)
        let updated = sqlx::query_scalar!(
            "UPDATE core.auctions
             SET ends_at = ends_at + make_interval(mins => $2)
             WHERE id = $1 AND status = 'running'
             RETURNING id",
            id,
            // make_interval(mins => …) принимает int - домен объявляет продление
            // в i64, но 15 минут в int влезают с любым запасом
            i32::try_from(auction::EXTENSION_MINUTES).unwrap_or(i32::MAX)
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_rule_violation)?;

        match updated {
            Some(id) => require(&mut *tx, id).await,
            None => Err(status_conflict(&mut *tx, id, "продлить можно только идущие торги").await),
        }
    })
    .await
}

/// Завершение торгов (FR-606): итог считает домен по ленте ставок, БД
/// проверяет, что победитель и второе место - реальные ставки этих торгов.
/// `early` - досрочно при общем согласии (п. 67).
/// Завершение торгов запускает срок протокола итогов (FR-1702, п. 73):
/// сроки Правил - следствие событий, а не ручная отметка.
pub async fn finish(
    db: &Db,
    actor: Uuid,
    id: Uuid,
    early: bool,
) -> Result<(AuctionRecord, Outcome), TransitionError> {
    crate::with_actor(db, actor, async |tx| finish_on(tx, id, early).await).await
}

/// Завершение в транзакции вызывающего - тело [`finish`].
///
/// Отдельно от него, потому что у завершения два повода: решение
/// председателя (актор - человек) и истекший таймер (актор - система,
/// [`finish_expired`]). Правило одно, и оно не должно существовать
/// в двух списаниях.
async fn finish_on(
    tx: &mut sqlx::PgConnection,
    id: Uuid,
    early: bool,
) -> Result<(AuctionRecord, Outcome), TransitionError> {
    {
        let ledger = sqlx::query!(
            "SELECT application_id, amount FROM core.bids WHERE auction_id = $1 ORDER BY seq",
            id
        )
        .fetch_all(&mut *tx)
        .await?;

        let bids: Vec<auction::Bid> = ledger
            .into_iter()
            .map(|row| auction::Bid {
                application_id: ApplicationId::new(row.application_id),
                amount: Money::new(row.amount),
            })
            .collect();
        let outcome = auction::outcome(&bids);

        let updated = sqlx::query_scalar!(
            "UPDATE core.auctions
             SET status = 'finished', finished_at = core.now(), finished_early = $2,
                 winner_application_id = $3, winner_amount = $4,
                 runner_up_application_id = $5, runner_up_amount = $6
             WHERE id = $1 AND status = 'running'
             RETURNING id",
            id,
            early,
            outcome.winner.map(|w| w.application_id.into_uuid()),
            outcome.winner.map(|w| w.amount.amount()),
            outcome.runner_up.map(|r| r.application_id.into_uuid()),
            outcome.runner_up.map(|r| r.amount.amount())
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_rule_violation)?;

        match updated {
            Some(id) => {
                // Торги закончились - пошел срок протокола итогов (п. 73).
                // Обязательство одно на тендер: повторный вызов по другому
                // лоту его не двоит (UNIQUE по предмету)
                let tender_id = sqlx::query_scalar!(
                    "SELECT l.tender_id FROM core.auctions a
                     JOIN core.lots l ON l.id = a.lot_id WHERE a.id = $1",
                    id
                )
                .fetch_one(&mut *tx)
                .await?;
                crate::obligations::schedule(
                    &mut *tx,
                    ObligationAction::ResultsProtocol,
                    crate::obligations::Subject::tender(tender_id),
                )
                .await?;

                Ok((require(&mut *tx, id).await?, outcome))
            }
            None => Err(status_conflict(&mut *tx, id, "завершить можно только идущие торги").await),
        }
    }
}

/// Торги, у которых истек server-authoritative таймер (INV-066, п. 66, 68).
///
/// Комната закрывается сама: ставку после `ends_at` БД и так не примет, но
/// без завершения торги остаются `running` - победитель не определен, срок
/// протокола итогов (п. 73) не открыт, а участники видят таймер на нуле.
/// Раньше это исправляло только нажатие председателя, и если он его не
/// делал, процесс останавливался.
///
/// Актор события - система: время вышло само, без человека (так же
/// устроена эскалация просроченных сроков). Пачкой с `FOR UPDATE SKIP
/// LOCKED`, чтобы два экземпляра воркера (NFR-12) разбирали разные торги.
pub async fn finish_expired(db: &Db, limit: i64) -> Result<Vec<AuctionRecord>, TransitionError> {
    let mut tx = db.begin().await?;
    let finished = finish_expired_on(&mut tx, limit).await?;
    tx.commit().await?;
    Ok(finished)
}

/// Тот же проход в транзакции вызывающего - вариант `*_on` (арх. v3 § 6):
/// тест выполняет сценарий и откатывает его, не засоряя стенд.
pub async fn finish_expired_on(
    tx: &mut sqlx::PgConnection,
    limit: i64,
) -> Result<Vec<AuctionRecord>, TransitionError> {
    let expired = sqlx::query_scalar!(
        "SELECT id FROM core.auctions
          WHERE status = 'running' AND ends_at IS NOT NULL AND ends_at < core.now()
          ORDER BY ends_at
          LIMIT $1
          FOR UPDATE SKIP LOCKED",
        limit
    )
    .fetch_all(&mut *tx)
    .await?;

    let mut finished = Vec::with_capacity(expired.len());
    for id in expired {
        // `early = false`: время вышло, а не «по общему согласию» (п. 67)
        let (record, _outcome) = finish_on(&mut *tx, id, false).await?;
        finished.push(record);
    }

    Ok(finished)
}

#[derive(Debug, thiserror::Error)]
pub enum BidError {
    #[error("торги не найдены")]
    NotFound,
    /// Нет допущенной заявки участника на лот этих торгов (п. 62)
    #[error("торговаться может только допущенный участник этого лота")]
    NotAdmitted,
    /// Отказ БД: шаг (INV-063), истекший таймер (INV-066), статус комнаты
    #[error("{0}")]
    Rejected(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

pub struct NewBid {
    /// Идентификатор ставки генерирует клиент (uuid v7) - повтор после
    /// реконнекта возвращает ту же ставку, а не отказ (NFR-05)
    pub id: Uuid,
    pub auction_id: Uuid,
    pub amount: Decimal,
}

/// Ставка участника (FR-601). Идемпотентна по `id`; правила шага и времени
/// проверяет триггер `enforce_bid_rules` под `FOR UPDATE` на аукционе.
pub async fn place_bid(db: &Db, actor: Uuid, new: NewBid) -> Result<BidRecord, BidError> {
    crate::with_actor(db, actor, async |tx| {
        let lot_id = sqlx::query_scalar!(
            "SELECT lot_id FROM core.auctions WHERE id = $1",
            new.auction_id
        )
        .fetch_optional(&mut *tx)
        .await?;
        let lot_id = lot_id.ok_or(BidError::NotFound)?;

        // Повтор той же ставки (реконнект) - возврат уже записанной
        if let Some(existing) = fetch_bid(&mut *tx, new.id).await? {
            return Ok(existing);
        }

        let application_id = sqlx::query_scalar!(
            "SELECT id FROM core.applications
             WHERE participant_id = $1 AND lot_id = $2 AND status = 'admitted'",
            actor,
            lot_id
        )
        .fetch_optional(&mut *tx)
        .await?;
        let application_id = application_id.ok_or(BidError::NotAdmitted)?;

        let inserted = sqlx::query_scalar!(
            "INSERT INTO core.bids (id, auction_id, application_id, amount)
             VALUES ($1, $2, $3, $4) RETURNING id",
            new.id,
            new.auction_id,
            application_id,
            new.amount
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|err| match err {
            sqlx::Error::Database(db_err)
                if matches!(db_err.code().as_deref(), Some("23514") | Some("23503")) =>
            {
                BidError::Rejected(db_err.message().to_owned())
            }
            other => BidError::Db(other),
        })?;

        // Очередь идет дальше по кругу (FR-604, п. 65)
        crate::auction_turns::advance_after_bid(&mut *tx, new.auction_id, application_id).await?;

        fetch_bid(&mut *tx, inserted)
            .await?
            .ok_or(BidError::NotFound)
    })
    .await
}

async fn fetch_bid(
    conn: &mut sqlx::PgConnection,
    id: Uuid,
) -> Result<Option<BidRecord>, sqlx::Error> {
    bid_query!(" WHERE b.id = $1", id)
        .fetch_optional(conn)
        .await
}

async fn require(
    conn: &mut sqlx::PgConnection,
    id: Uuid,
) -> Result<AuctionRecord, TransitionError> {
    fetch(conn, id).await?.ok_or(TransitionError::NotFound)
}

/// Отличает «не найдено» от «не тот статус»: UPDATE ... WHERE status = ...
/// не говорит, что именно не совпало.
async fn status_conflict(
    conn: &mut sqlx::PgConnection,
    id: Uuid,
    expectation: &str,
) -> TransitionError {
    let current = sqlx::query_scalar!(
        r#"SELECT status::text AS "status!" FROM core.auctions WHERE id = $1"#,
        id
    )
    .fetch_optional(conn)
    .await;
    match current {
        Ok(Some(status)) => TransitionError::Rejected(format!("{expectation} (сейчас {status})")),
        Ok(None) => TransitionError::NotFound,
        Err(err) => TransitionError::Db(err),
    }
}

fn map_rule_violation(error: sqlx::Error) -> TransitionError {
    match error {
        sqlx::Error::Database(db_err)
            if matches!(db_err.code().as_deref(), Some("23514") | Some("23503")) =>
        {
            TransitionError::Rejected(db_err.message().to_owned())
        }
        other => TransitionError::Db(other),
    }
}
