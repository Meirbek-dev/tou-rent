//! Отказы, которые до правки уходили наружу пятисоткой или, хуже, ответом
//! чужой операции (круг 2 гаунтлета: AC-3, AC-5, RC-4).
//!
//! Общее у трех проверок одно: правило существует, но слой данных о нем
//! молчал - `sqlx::Error` доходил до обработчика голым, и `From<sqlx::Error>`
//! превращал его в `internal`. Здесь проверяется именно перевод отказа в
//! типизированную причину, а не сам факт отказа БД.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021). Каждая проверка живет в
//! транзакции с откатом - отсюда варианты `*_on` слоя данных.

use rust_decimal::Decimal;
use sqlx::Acquire as _;
use tou_domain::rule::RuleViolation;
use uuid::Uuid;

async fn try_pool() -> Result<Option<tou_db::Db>, sqlx::Error> {
    match tou_testkit::database_url().map_err(|e| sqlx::Error::Configuration(Box::new(e)))? {
        Some(url) => tou_db::connect(&url).await.map(Some),
        None => Ok(None),
    }
}

macro_rules! require_db {
    () => {
        match try_pool()
            .await
            .expect("TESTKIT_DATABASE_URL: подключение не удалось")
        {
            Some(db) => db,
            None => {
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - отказы не проверялись");
                return;
            }
        }
    };
}

async fn user(tx: &mut sqlx::PgConnection, tag: &str) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', $2, core.now()) RETURNING id",
        format!("g2-{tag}-{}@tou.test", Uuid::now_v7().simple()),
        format!("Гаунтлет {tag}")
    )
    .fetch_one(tx)
    .await
}

/// Момент по доменным часам (`core.now()`, ADR-0005) - тот же, которым
/// считают сроки и триггеры БД.
async fn domain_now(tx: &mut sqlx::PgConnection) -> Result<time::OffsetDateTime, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT core.now() AS "now!""#)
        .fetch_one(tx)
        .await
}

/// Деловая дата: она считается по Алматы, а сессия БД живет в UTC.
async fn business_date(tx: &mut sqlx::PgConnection) -> Result<time::Date, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT (core.now() AT TIME ZONE 'Asia/Almaty')::date AS "today!""#)
        .fetch_one(tx)
        .await
}

async fn object(tx: &mut sqlx::PgConnection) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Гаунтлет объект', 'адрес', 10.00) RETURNING id"
    )
    .fetch_one(tx)
    .await
}

/// Рубеж AC-3: организатор, поставивший вскрытие раньше окончания приема
/// заявок, получал `{"status":500,"code":"internal"}` - CHECK
/// `deadline_before_opening` уезжал наружу голым `sqlx::Error`.
#[tokio::test]
async fn swapped_tender_dates_are_a_rule_violation_not_a_crash() {
    use tou_db::tenders::{DraftFields, TransitionError, update_draft_on};

    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let organizer = user(&mut tx, "организатор").await.expect("организатор");
    tou_db::set_actor(&mut tx, organizer).await.expect("актор");

    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id)
         VALUES ('Гаунтлет черновик', 'draft', $1) RETURNING id",
        organizer
    )
    .fetch_one(&mut *tx)
    .await
    .expect("черновик");

    let now = domain_now(&mut tx).await.expect("часы домена");
    let deadline = now + time::Duration::days(20);
    let opening = now + time::Duration::days(10);

    // Отказ по CHECK обрывает транзакцию: проба идет в точке сохранения,
    // иначе проверка «правильный порядок принимается» ниже уперлась бы в
    // «current transaction is aborted»
    let mut sp = tx.begin().await.expect("savepoint");
    let rejected = update_draft_on(
        &mut sp,
        tender_id,
        DraftFields {
            title: "Гаунтлет черновик",
            title_kk: "Гаунтлет жобасы",
            submission_deadline: Some(deadline),
            opening_at: Some(opening),
            trading_at: None,
            zoom_url: None,
        },
    )
    .await
    .expect_err("вскрытие раньше окончания приема заявок обязано быть отклонено");
    sp.rollback().await.expect("откат точки сохранения");

    match rejected {
        TransitionError::Rejected(reason) => assert_eq!(
            reason.rule(),
            RuleViolation::TenderPublicationTerms,
            "причина отказа названа не тем правилом"
        ),
        other => panic!("отказ ушел наружу как поломка: {other:?}"),
    }

    // Правильный порядок дат по-прежнему принимается
    update_draft_on(
        &mut tx,
        tender_id,
        DraftFields {
            title: "Гаунтлет черновик",
            title_kk: "Гаунтлет жобасы",
            submission_deadline: Some(opening),
            opening_at: Some(deadline),
            trading_at: None,
            zoom_url: None,
        },
    )
    .await
    .expect("даты в правильном порядке")
    .expect("черновик найден");

    tx.rollback().await.expect("откат");
}

/// Рубеж AC-5: дата поступления денег принималась любая - и `0001-01-01`,
/// и завтрашняя. Проводка книги append-only, исправить ее нечем.
#[tokio::test]
async fn a_fee_cannot_arrive_in_the_future_or_before_the_tender() {
    use tou_db::ledger::{LedgerError, confirm_fee_on};

    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let participant = user(&mut tx, "участник").await.expect("участник");
    let organizer = user(&mut tx, "организатор").await.expect("организатор");
    let object_id = object(&mut tx).await.expect("объект");
    tou_db::set_actor(&mut tx, organizer).await.expect("актор");

    let now = domain_now(&mut tx).await.expect("часы домена");
    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id, announced_at, opening_at)
         VALUES ('Гаунтлет взнос', 'accepting', $1, $2, $3) RETURNING id",
        organizer,
        now - time::Duration::days(5),
        now + time::Duration::days(30)
    )
    .fetch_one(&mut *tx)
    .await
    .expect("тендер");

    let fee = Decimal::new(36000, 0);
    let lot_id = sqlx::query_scalar!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                base_rate_monthly, guarantee_fee, rate_calculation)
         VALUES ($1, 1, $2, 'офис', 12, $3, $3, '{}'::jsonb) RETURNING id",
        tender_id,
        object_id,
        fee
    )
    .fetch_one(&mut *tx)
    .await
    .expect("лот");

    let application_id = sqlx::query_scalar!(
        "INSERT INTO core.applications
           (tender_id, lot_id, participant_id, status, applicant_kind, applicant_details)
         VALUES ($1, $2, $3, 'submitted', 'individual', '{}'::jsonb) RETURNING id",
        tender_id,
        lot_id,
        participant
    )
    .fetch_one(&mut *tx)
    .await
    .expect("заявка");

    let today = business_date(&mut tx).await.expect("деловая дата");
    for (paid_at, why) in [
        (today + time::Duration::days(22), "будущая дата"),
        (
            time::Date::from_calendar_date(1, time::Month::January, 1).expect("дата"),
            "дата до нашей эры делопроизводства",
        ),
        (
            today - time::Duration::days(6),
            "дата до объявления тендера",
        ),
    ] {
        let rejected = match confirm_fee_on(&mut tx, organizer, application_id, fee, paid_at).await
        {
            Err(error) => error,
            Ok(_) => panic!("{why}: поступление принято, хотя не могло состояться"),
        };
        match rejected {
            LedgerError::Rejected(reason) => assert_eq!(
                reason.rule(),
                RuleViolation::GuaranteeDeposit,
                "{why}: причина отказа названа не тем правилом"
            ),
            other => panic!("{why}: отказ ушел наружу как {other:?}"),
        }
    }

    // Сегодняшнее поступление по-прежнему принимается
    confirm_fee_on(&mut tx, organizer, application_id, fee, today)
        .await
        .expect("поступление сегодняшним днем");

    tx.rollback().await.expect("откат");
}

/// Рубеж RC-4: идемпотентность ставки держалась на одном `id`, а он публичен
/// (лента `GET /auctions/{id}/bids`). Подставив чужой идентификатор, участник
/// получал чужую ставку - с именем заявителя и суммой - и рассылку фантомной
/// ставки в комнату.
#[tokio::test]
async fn bid_idempotency_is_bound_to_the_room_and_the_bidder() {
    use tou_db::auctions::{BidError, NewBid, place_bid_on};

    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let organizer = user(&mut tx, "секретарь").await.expect("секретарь");
    let mine = user(&mut tx, "участник-1").await.expect("участник-1");
    let stranger = user(&mut tx, "участник-2").await.expect("участник-2");
    let object_id = object(&mut tx).await.expect("объект");
    tou_db::set_actor(&mut tx, organizer).await.expect("актор");

    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id)
         VALUES ('Гаунтлет торги', 'trading', $1) RETURNING id",
        organizer
    )
    .fetch_one(&mut *tx)
    .await
    .expect("тендер");

    let lot_id = sqlx::query_scalar!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                base_rate_monthly, guarantee_fee, rate_calculation)
         VALUES ($1, 1, $2, 'офис', 12, 55000, 55000, '{}'::jsonb) RETURNING id",
        tender_id,
        object_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("лот");

    let mut admitted = async |participant: Uuid| -> Uuid {
        sqlx::query_scalar!(
            "INSERT INTO core.applications
               (tender_id, lot_id, participant_id, status, applicant_kind, applicant_details)
             VALUES ($1, $2, $3, 'admitted', 'individual', '{}'::jsonb) RETURNING id",
            tender_id,
            lot_id,
            participant
        )
        .fetch_one(&mut *tx)
        .await
        .expect("допущенная заявка")
    };
    let _my_application = admitted(mine).await;
    let _their_application = admitted(stranger).await;

    let auction_id = sqlx::query_scalar!(
        "INSERT INTO core.auctions (lot_id, starting_bid, bid_step_percent, bid_step)
         VALUES ($1, 55000, 5, 2750) RETURNING id",
        lot_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("комната");
    sqlx::query!(
        "UPDATE core.auctions
         SET status = 'running', started_at = core.now(),
             ends_at = core.now() + interval '1 hour'
         WHERE id = $1",
        auction_id
    )
    .execute(&mut *tx)
    .await
    .expect("торги идут");

    let bid_id = Uuid::now_v7();
    let placed = place_bid_on(
        &mut tx,
        mine,
        NewBid {
            id: bid_id,
            auction_id,
            amount: Decimal::new(5775000, 2),
        },
    )
    .await
    .expect("своя ставка");

    // Повтор своей ставки идемпотентен - ровно та же запись, без второй
    let repeated = place_bid_on(
        &mut tx,
        mine,
        NewBid {
            id: bid_id,
            auction_id,
            amount: Decimal::new(5775000, 2),
        },
    )
    .await
    .expect("повтор своей ставки");
    assert_eq!(repeated.id, placed.id);
    assert_eq!(repeated.seq, placed.seq, "повтор создал вторую ставку");

    // Чужой идентификатор не возвращает чужую ставку. Отказ идет через БД и
    // обрывает транзакцию, поэтому проба - в точке сохранения: иначе подсчет
    // ленты ниже уперся бы в «current transaction is aborted»
    let mut sp = tx.begin().await.expect("savepoint");
    let foreign = match place_bid_on(
        &mut sp,
        stranger,
        NewBid {
            id: bid_id,
            auction_id,
            amount: Decimal::new(6050000, 2),
        },
    )
    .await
    {
        Err(error) => error,
        Ok(_) => panic!("чужой идентификатор ставки не должен приниматься"),
    };
    sp.rollback().await.expect("откат точки сохранения");
    match foreign {
        BidError::Rejected(reason) => assert_eq!(reason.rule(), RuleViolation::DuplicateRecord),
        other => panic!("чужой идентификатор ушел как {other:?}"),
    }

    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM core.bids WHERE auction_id = $1"#,
        auction_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("лента ставок");
    assert_eq!(total, 1, "в ленте оказалась лишняя ставка");

    tx.rollback().await.expect("откат");
}
