//! Заявка называет свой предмет (FR-402, G-083) - против живой БД.
//!
//! Кабинет участника и рабочий список секретаря показывают заявки списком,
//! а выборка заявки отдавала только идентификаторы тендера, лота и объекта.
//! Назвать предмет было нечем: интерфейсу оставалось либо печатать uuid,
//! либо ходить за каждым тендером отдельным запросом (N+1 на список).
//!
//! Проверяется именно выборка db-слоя: соединения с тендером, лотом и
//! объектом живут в макросе `application_query!`, и ошибка в них видна
//! только против настоящей схемы. Подключение - TESTKIT_DATABASE_URL (A-021).

use uuid::Uuid;

async fn try_pool() -> Result<Option<tou_db::Db>, sqlx::Error> {
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - предмет заявки не проверялся");
                return;
            }
        }
    };
}

const TENDER_TITLE: &str = "G-083 аренда буфета учебного корпуса";
const OBJECT_NAME: &str = "G-083 помещение буфета";
/// Номер лота нарочно не первый: единица сошлась бы с любым значением
/// по умолчанию и ошибку в соединении не показала бы.
const LOT_SEQ: i32 = 3;

struct Fixture {
    organizer: Uuid,
    participant: Uuid,
    object: Uuid,
    tender: Uuid,
    application: Uuid,
}

async fn user(db: &tou_db::Db, tag: &str) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'G-083 участник процедуры', core.now()) RETURNING id",
        format!("g083-{tag}-{}@tou.test", Uuid::now_v7().simple())
    )
    .fetch_one(db)
    .await
}

/// Тендер на приеме заявок с одним лотом и одной поданной заявкой.
///
/// Строки кладутся напрямую: проверяется выборка, а не путь подачи, -
/// у него есть свой тест, и он тянет за собой цену, журнал и пакет
/// документов, к предмету заявки отношения не имеющие.
async fn fixture(db: &tou_db::Db) -> Result<Fixture, sqlx::Error> {
    let organizer = user(db, "org").await?;
    let participant = user(db, "part").await?;

    // `name_kk` и `title_kk` дописывают триггеры `fill_object_kk`
    // и `fill_tender_kk` - о них тест и не должен знать
    let object = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', $1, 'г. Павлодар, ул. Ломова, 64', 42.00) RETURNING id",
        OBJECT_NAME
    )
    .fetch_one(db)
    .await?;

    let tender = sqlx::query_scalar!(
        "INSERT INTO core.tenders
           (title, status, organizer_id, announced_at, submission_deadline, opening_at)
         VALUES ($1, 'accepting', $2, core.now() - interval '2 days',
                 core.now() + interval '12 days', core.now() + interval '13 days')
         RETURNING id",
        TENDER_TITLE,
        organizer
    )
    .fetch_one(db)
    .await?;

    let lot = sqlx::query_scalar!(
        "INSERT INTO core.lots
           (tender_id, seq, object_id, purpose, lease_months,
            base_rate_monthly, guarantee_fee, rate_calculation)
         VALUES ($1, $2, $3, 'буфет', 12, 36000.00, 36000.00, '{}'::jsonb)
         RETURNING id",
        tender,
        LOT_SEQ,
        object
    )
    .fetch_one(db)
    .await?;

    let application = sqlx::query_scalar!(
        "INSERT INTO core.applications
           (tender_id, lot_id, participant_id, applicant_kind, applicant_details)
         VALUES ($1, $2, $3, 'legal_entity', '{}'::jsonb) RETURNING id",
        tender,
        lot,
        participant
    )
    .fetch_one(db)
    .await?;

    Ok(Fixture {
        organizer,
        participant,
        object,
        tender,
        application,
    })
}

/// Уборка через ту же процедуру, что и вкладка «Данные» админа: заявка
/// тянет за собой материал досье, а он append-only (FR-1602) - обычным
/// DELETE тендер не убрать.
async fn cleanup(db: &tou_db::Db, f: &Fixture) {
    let _ = tou_db::purge::purge(
        db,
        f.organizer,
        tou_db::purge::PurgeScope::Tenders,
        Some(&[f.tender]),
    )
    .await;
    let _ = sqlx::query!("DELETE FROM core.objects WHERE id = $1", f.object)
        .execute(db)
        .await;
    let _ = sqlx::query!(
        "DELETE FROM core.users WHERE id = ANY($1)",
        &[f.organizer, f.participant][..]
    )
    .execute(db)
    .await;
}

/// Рубеж G-083: заявка приходит вместе с тем, к чему она подана.
#[tokio::test]
async fn an_application_carries_the_name_of_its_subject() {
    let db = require_db!();
    let f = fixture(&db).await.expect("тендер с заявкой");

    let record = tou_db::applications::get(&db, f.participant, f.application)
        .await
        .expect("чтение заявки")
        .expect("заявка на месте");

    assert_eq!(record.tender_title, TENDER_TITLE);
    assert!(
        !record.tender_title_kk.trim().is_empty(),
        "казахский заголовок тендера обязан приходить вместе с русским (NFR-01)"
    );
    assert_eq!(record.lot_seq, LOT_SEQ, "номер лота - тот, что у лота");
    assert_eq!(record.object_name, OBJECT_NAME);

    // Список кабинета читает ту же выборку - предмет обязан быть и в нем
    let own = tou_db::applications::list_own(&db, f.participant)
        .await
        .expect("список заявок участника");
    let listed = own
        .iter()
        .find(|item| item.id == f.application)
        .expect("своя заявка в списке");
    assert_eq!(listed.tender_title, TENDER_TITLE);
    assert_eq!(listed.object_name, OBJECT_NAME);
    assert_eq!(listed.lot_seq, LOT_SEQ);

    cleanup(&db, &f).await;
}
