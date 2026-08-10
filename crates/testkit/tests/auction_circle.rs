//! Регламент торгов по кругу против живой БД (T22, FR-604–605).
//!
//! Проверяется последний рубеж - триггер `enforce_bid_rules`: ставка вне
//! очереди, ставка выбывшего и неявившегося, а также оглашение
//! первоначального предложения, которое шагу торгов не подчиняется.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use rust_decimal::Decimal;
use sqlx::Acquire as _;
use uuid::Uuid;

async fn try_pool() -> Result<Option<tou_db::Db>, sqlx::Error> {
    // Пропуск без адреса допустим локально, но не в пайплайне (G2/G15):
    // молча пройденный интеграционный тест ничего не проверяет
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - круг торгов не проверялся");
                return;
            }
        }
    };
}

/// Отказ БД внутри savepoint: транзакция теста продолжает жить.
macro_rules! rejected {
    ($tx:expr, $query:expr, $why:expr) => {{
        let mut sp = $tx.begin().await.expect("savepoint");
        let error = $query.execute(&mut *sp).await.expect_err($why);
        sp.rollback().await.expect("rollback savepoint");
        error.to_string()
    }};
}

/// Ставка в ленту: параметры типизированы, чтобы отказ приходил от правила,
/// а не от несовпадения типов.
///
/// Макрос, а не функция: проверенный по схеме запрос имеет безымянный тип,
/// вернуть его из функции нельзя.
macro_rules! bid {
    ($auction_id:expr, $application_id:expr, $amount:expr) => {
        sqlx::query!(
            "INSERT INTO core.bids (id, auction_id, application_id, amount)
             VALUES (uuidv7(), $1, $2, $3)",
            $auction_id,
            $application_id,
            Decimal::from($amount as i64)
        )
    };
}

struct Fixture {
    auction_id: Uuid,
    /// Заявки в порядке очередности круга
    first: Uuid,
    second: Uuid,
    third: Uuid,
}

/// Торги с тремя допущенными участниками: круг собран, ход у первого.
async fn fixture(tx: &mut sqlx::PgConnection) -> Result<Fixture, sqlx::Error> {
    let organizer = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'Т22 организатор', now()) RETURNING id",
        format!("t22-org-{}@tou.test", Uuid::now_v7().simple())
    )
    .fetch_one(&mut *tx)
    .await?;

    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id)
         VALUES ('Т22 тендер', 'trading', $1) RETURNING id",
        organizer
    )
    .fetch_one(&mut *tx)
    .await?;

    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Т22 объект', 'адрес', 10.00) RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await?;

    let lot_id = sqlx::query_scalar!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                base_rate_monthly, guarantee_fee, rate_calculation)
         VALUES ($1, 1, $2, 'офис', 12, 50000.00, 50000.00, '{}'::jsonb) RETURNING id",
        tender_id,
        object_id
    )
    .fetch_one(&mut *tx)
    .await?;

    // Три допущенные заявки с разными первоначальными предложениями
    let mut application = async |tag: &str, price: &str| -> Result<Uuid, sqlx::Error> {
        let participant = sqlx::query_scalar!(
            "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
             VALUES ($1::citext, 'x', $2, now()) RETURNING id",
            format!("t22-{tag}-{}@tou.test", Uuid::now_v7().simple()),
            format!("Т22 участник {tag}")
        )
        .fetch_one(&mut *tx)
        .await?;

        let application_id = sqlx::query_scalar!(
            "INSERT INTO core.applications
               (tender_id, lot_id, participant_id, status, applicant_kind, applicant_details)
             VALUES ($1, $2, $3, 'admitted', 'legal_entity', '{}'::jsonb) RETURNING id",
            tender_id,
            lot_id,
            participant
        )
        .fetch_one(&mut *tx)
        .await?;

        // Цена пишется под своим участником: RLS INV-040 разрешает вставку
        // только владельцу заявки
        sqlx::query!(
            "SELECT set_config('app.user_id', $1, true)",
            participant.to_string()
        )
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query!(
            "INSERT INTO core.price_proposals (application_id, amount) VALUES ($1, $2)",
            application_id,
            price.parse::<Decimal>().unwrap_or_default()
        )
        .execute(&mut *tx)
        .await?;

        Ok(application_id)
    };

    let first = application("1", "55000").await?;
    let second = application("2", "52000").await?;
    let third = application("3", "48000").await?;

    // Комната: старт = максимум предложений (INV-062), шаг 5 % (п. 63)
    let auction_id = sqlx::query_scalar!(
        "INSERT INTO core.auctions (lot_id, starting_bid, bid_step, status, started_at, ends_at,
                                    current_turn_application_id)
         VALUES ($1, 55000.00, 2750.00, 'running', now(), now() + interval '1 hour', $2)
         RETURNING id",
        lot_id,
        first
    )
    .fetch_one(&mut *tx)
    .await?;

    for (order, application_id, price) in [
        (1, first, "55000"),
        (2, second, "52000"),
        (3, third, "48000"),
    ] {
        sqlx::query!(
            "INSERT INTO core.auction_participants
               (auction_id, application_id, turn_order, initial_price)
             VALUES ($1, $2, $3, $4)",
            auction_id,
            application_id,
            order,
            price.parse::<Decimal>().unwrap_or_default()
        )
        .execute(&mut *tx)
        .await?;
    }

    Ok(Fixture {
        auction_id,
        first,
        second,
        third,
    })
}

/// FR-604 (п. 65): ходит тот, чья очередь; остальные получают отказ.
#[tokio::test]
async fn fr604_bid_out_of_turn_is_rejected() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    let error = rejected!(
        tx,
        bid!(f.auction_id, f.second, 57_750),
        "ставка вне очереди обязана быть отклонена"
    );
    assert!(error.contains("FR-604"), "ожидали отказ FR-604: {error}");

    // Тот, чей ход, ставит без препятствий
    bid!(f.auction_id, f.first, 57_750)
        .execute(&mut *tx)
        .await
        .expect("ставка участника, чей ход");
}

/// FR-604: выбывший больше не повышает; FR-605: не явившийся тоже.
#[tokio::test]
async fn passed_and_absent_participants_cannot_bid() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    sqlx::query!(
        "UPDATE core.auction_participants SET status = 'passed' WHERE application_id = $1",
        f.second
    )
    .execute(&mut *tx)
    .await
    .expect("выбытие");
    sqlx::query!(
        "UPDATE core.auction_participants SET status = 'absent' WHERE application_id = $1",
        f.third
    )
    .execute(&mut *tx)
    .await
    .expect("неявка");
    // Ход снимаем, чтобы проверялось именно состояние участника
    sqlx::query!(
        "UPDATE core.auctions SET current_turn_application_id = NULL WHERE id = $1",
        f.auction_id
    )
    .execute(&mut *tx)
    .await
    .expect("ход снят");

    let passed = rejected!(
        tx,
        bid!(f.auction_id, f.second, 57_750),
        "выбывший не повышает"
    );
    assert!(passed.contains("FR-604"), "{passed}");

    let absent = rejected!(
        tx,
        bid!(f.auction_id, f.third, 57_750),
        "не явившийся не повышает"
    );
    assert!(absent.contains("FR-605"), "{absent}");
}

/// FR-605 (п. 70): оглашенное первоначальное предложение попадает в ленту,
/// хотя оно ниже стартовой ставки, - это не повышение, а оглашение.
/// Планка следующей ставки от него не опускается (INV-062–063).
#[tokio::test]
async fn fr605_announced_offer_bypasses_the_step_but_not_the_floor() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    sqlx::query!(
        "INSERT INTO core.bids (id, auction_id, application_id, amount, announced)
         VALUES (uuidv7(), $1, $2, 48000.00, true)",
        f.auction_id,
        f.third
    )
    .execute(&mut *tx)
    .await
    .expect("оглашение предложения отсутствующего");

    // Ставка ниже «старт + шаг» по-прежнему отклоняется: оглашение планку
    // не опустило
    let error = rejected!(
        tx,
        bid!(f.auction_id, f.first, 50_750),
        "оглашение не опускает минимальную ставку"
    );
    assert!(error.contains("INV-063"), "{error}");

    bid!(f.auction_id, f.first, 57_750)
        .execute(&mut *tx)
        .await
        .expect("ставка от старта плюс шаг");
}

/// Круг и его состояния читаются доменом без расхождений с БД.
#[tokio::test]
async fn circle_states_match_the_database_enum() {
    let db = require_db!();

    let states = sqlx::query_scalar!(
        r#"SELECT unnest(enum_range(NULL::core.auction_participant_status))::text AS "state!"
           ORDER BY 1"#
    )
    .fetch_all(&db)
    .await
    .expect("enum БД");

    let mut expected: Vec<String> = tou_domain::turn::ParticipantState::ALL
        .iter()
        .map(|state| state.as_str().to_owned())
        .collect();
    expected.sort();
    assert_eq!(states, expected);
}
