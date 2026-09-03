//! Тендер доходит до торгов и итогов силами приложения (G-110; FR-302,
//! FR-701, INV-021, п. 60, 73).
//!
//! Два последних перехода статусной модели стояли в
//! `refdata.tender_status_transitions`, но выполнял их один только seed
//! демо-стенда: живая процедура доходила до допуска и там останавливалась.
//! Торги шли, победитель определялся, протокол итогов формировался - а
//! тендер оставался `qualification`, и ссылка на запись торгов (FR-306,
//! п. 72), которая принимается только после итогов, была недостижима.
//!
//! Здесь проверяется то, что делает приложение против живой БД: старт
//! первой комнаты переводит тендер в `trading`, протокол итогов - в
//! `summed_up`, после чего ссылка на запись принимается; отмененный тендер
//! те же действия не трогают.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use tou_db::admission::NewProtocol;
use tou_db::purge::PurgeScope;
use uuid::Uuid;

async fn try_pool() -> Result<Option<tou_db::Db>, sqlx::Error> {
    // Пропуск без адреса допустим локально, но не в пайплайне (G2/G15):
    // молча пройденный интеграционный тест ничего не проверяет
    match tou_testkit::database_ready()
        .await
        .map_err(|e| sqlx::Error::Configuration(Box::new(e)))?
    {
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - статусы тендера не проверялись");
                return;
            }
        }
    };
}

const RECORDING: &str = "https://zoom.example.test/rec/g110";

struct Fixture {
    actor: Uuid,
    tender: Uuid,
    auction: Uuid,
    meeting: Uuid,
}

/// Тендер в заданном статусе с одним лотом, назначенной комнатой торгов и
/// заседанием комиссии. Статус выставляется при вставке: переходы сторожит
/// триггер INV-021, а тесту нужна не цепочка переходов, а состояние.
async fn fixture(db: &tou_db::Db, status: &str) -> Result<Fixture, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let actor = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'argon2-заглушка', 'G110 секретарь', core.now()) RETURNING id",
        format!("g110-secretary-{tag}@tou.test")
    )
    .fetch_one(db)
    .await?;

    let object = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', $1, 'адрес', 12.00) RETURNING id",
        format!("G110 объект {tag}")
    )
    .fetch_one(db)
    .await?;

    let tender = sqlx::query_scalar!(
        "INSERT INTO core.tenders
           (title, status, organizer_id, announced_at, submission_deadline,
            opening_at, opened_at, trading_at)
         VALUES ($1, $2::text::core.tender_status, $3,
                 core.now() - interval '12 days', core.now() - interval '2 hours',
                 core.now() - interval '1 hour', core.now() - interval '1 hour',
                 core.now() - interval '30 minutes')
         RETURNING id",
        format!("G110 тендер {tag}"),
        status,
        actor
    )
    .fetch_one(db)
    .await?;

    let lot = sqlx::query_scalar!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                base_rate_monthly, guarantee_fee, rate_calculation)
         VALUES ($1, 1, $2, 'офис', 12, 50000.00, 50000.00, '{}'::jsonb) RETURNING id",
        tender,
        object
    )
    .fetch_one(db)
    .await?;

    let auction = sqlx::query_scalar!(
        "INSERT INTO core.auctions (lot_id, status, starting_bid, bid_step_percent, bid_step)
         VALUES ($1, 'scheduled', 55000.00, 5, 2750.00) RETURNING id",
        lot
    )
    .fetch_one(db)
    .await?;

    // Заседание нужно протоколу реквизитами (п. 55, 73): состав комиссии
    // к переходу статуса отношения не имеет, и члены здесь не заводятся
    let commission = sqlx::query_scalar!(
        "INSERT INTO core.commissions (name, valid_from, valid_until)
         VALUES ($1, current_date, current_date + interval '1 year') RETURNING id",
        format!("G110 комиссия {tag}")
    )
    .fetch_one(db)
    .await?;

    let meeting = sqlx::query_scalar!(
        "INSERT INTO core.sessions_meetings (tender_id, commission_id, kind, scheduled_at)
         VALUES ($1, $2, 'results', core.now() - interval '10 minutes') RETURNING id",
        tender,
        commission
    )
    .fetch_one(db)
    .await?;

    Ok(Fixture {
        actor,
        tender,
        auction,
        meeting,
    })
}

/// Уборка: функции слоя данных коммитят свои транзакции (иначе не
/// отработали бы аудит-триггеры), поэтому записи убираются той же
/// процедурой, что и вкладка «Данные» кабинета админа (М15): протокол
/// и материал досье обычным DELETE не удаляются.
async fn cleanup(db: &tou_db::Db, f: &Fixture) {
    let _ = tou_db::purge::purge(db, f.actor, PurgeScope::Tenders, Some(&[f.tender])).await;
}

async fn status_of(db: &tou_db::Db, tender: Uuid) -> Result<String, sqlx::Error> {
    // `!` у status - это `::text`, который планировщик считает потенциально NULL
    sqlx::query_scalar!(
        r#"SELECT status::text AS "status!" FROM core.tenders WHERE id = $1"#,
        tender
    )
    .fetch_one(db)
    .await
}

/// Снимок протокола итогов: содержимое печатной формы к статусу отношения
/// не имеет, и здесь оно намеренно минимально.
fn protocol_content() -> serde_json::Value {
    serde_json::json!({ "number": "И-g110" })
}

/// FR-302, INV-021, п. 60, 73: старт первой комнаты переводит тендер на
/// торги, протокол итогов подводит их, и только после этого принимается
/// ссылка на запись торгов (FR-306, FR-1903, п. 72).
#[tokio::test]
async fn the_procedure_walks_the_tender_from_qualification_to_summed_up() {
    let db = require_db!();
    let f = fixture(&db, "qualification").await.expect("фикстура");

    assert_eq!(
        status_of(&db, f.tender).await.expect("статус тендера"),
        "qualification"
    );

    // Пока итоги не подведены, ссылка на запись не принимается (FR-306)
    let early = tou_db::tenders::set_recording_url(&db, f.actor, f.tender, Some(RECORDING))
        .await
        .expect("запрос ссылки до итогов");
    assert!(
        early.is_none(),
        "до подведения итогов ссылка на запись торгов не вносится (FR-306, п. 72)"
    );

    let started = tou_db::auctions::start(&db, f.actor, f.auction, 60)
        .await
        .expect("старт торгов");
    assert_eq!(started.status, "running");
    assert_eq!(
        status_of(&db, f.tender).await.expect("статус тендера"),
        "trading",
        "старт первой комнаты переводит тендер на торги (INV-021, п. 60)"
    );

    // След перехода - событие аудита по строке тендера под актором (А.5)
    let audited = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM audit.log
           WHERE table_name = 'core.tenders' AND row_id = $1 AND action = 'UPDATE'
             AND actor_id = $2 AND payload -> 'new' ->> 'status' = 'trading'"#,
        f.tender,
        f.actor
    )
    .fetch_one(&db)
    .await
    .expect("события аудита");
    assert_eq!(audited, 1, "переход на торги записан в аудит");

    let (finished, _) = tou_db::auctions::finish(&db, f.actor, f.auction, false)
        .await
        .expect("завершение торгов");
    assert_eq!(finished.status, "finished");
    assert_eq!(
        status_of(&db, f.tender).await.expect("статус тендера"),
        "trading",
        "завершение комнаты итогов еще не подводит - их подводит протокол (п. 73)"
    );

    let content = protocol_content();
    let protocol = tou_db::admission::insert_protocol(
        &db,
        f.actor,
        NewProtocol {
            tender_id: f.tender,
            kind: "results",
            meeting_id: f.meeting,
            number: "И-g110",
            content: &content,
            pdf_key: "protocols/g110/results.pdf",
        },
    )
    .await
    .expect("протокол итогов");
    assert!(protocol.is_some(), "протокол итогов сформирован");
    assert_eq!(
        status_of(&db, f.tender).await.expect("статус тендера"),
        "summed_up",
        "протокол итогов подводит итоги тендера (FR-701, п. 73)"
    );

    // Итоги подведены - ссылка на запись торгов принимается (FR-1903)
    let saved = tou_db::tenders::set_recording_url(&db, f.actor, f.tender, Some(RECORDING))
        .await
        .expect("ссылка на запись")
        .expect("тендер подведен");
    assert_eq!(saved.zoom_recording_url.as_deref(), Some(RECORDING));

    cleanup(&db, &f).await;
}

/// FR-305, INV-021: отмененный тендер теми же действиями не трогается -
/// переход идет условием запроса, а не проверкой в коде, и `cancelled`
/// в него просто не попадает.
#[tokio::test]
async fn a_cancelled_tender_is_left_alone() {
    let db = require_db!();
    let f = fixture(&db, "cancelled").await.expect("фикстура");

    tou_db::auctions::start(&db, f.actor, f.auction, 60)
        .await
        .expect("старт торгов");
    assert_eq!(
        status_of(&db, f.tender).await.expect("статус тендера"),
        "cancelled",
        "отмененный тендер стартом комнаты не оживает (FR-305, п. 78)"
    );

    tou_db::auctions::finish(&db, f.actor, f.auction, false)
        .await
        .expect("завершение торгов");

    let content = protocol_content();
    tou_db::admission::insert_protocol(
        &db,
        f.actor,
        NewProtocol {
            tender_id: f.tender,
            kind: "results",
            meeting_id: f.meeting,
            number: "И-g110c",
            content: &content,
            pdf_key: "protocols/g110c/results.pdf",
        },
    )
    .await
    .expect("протокол итогов");
    assert_eq!(
        status_of(&db, f.tender).await.expect("статус тендера"),
        "cancelled",
        "итоги отмененного тендера не подводятся (INV-021)"
    );

    cleanup(&db, &f).await;
}
