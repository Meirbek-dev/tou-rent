//! Наступивший час вскрытия порождает обязанность его провести
//! (круг 1 гаунтлета, G-147; FR-501, FR-1702, п. 41–43) - против живой БД.
//!
//! До этой правки двигатель сроков видел процесс только с первого действия
//! человека. Прием заявок закрывался сам (INV-037), назначенный час
//! вскрытия наступал - и не происходило ничего: тендер оставался в
//! `accepting`, в «моих сроках» секретаря пусто, уведомления нет. Процедура
//! стояла до тех пор, пока кто-нибудь не заглядывал в список тендеров.
//!
//! Тендер при этом не переводится: вскрытие конвертов - действие комиссии
//! на заседании (п. 50), и подменять его отметкой в БД нельзя. Ставится
//! ровно обязанность, срок которой - сам назначенный час, то есть уже
//! наступивший: первый же проход эскалации известит секретаря (FR-1302).
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use tou_domain::obligation::ObligationAction;
use tou_domain::role::Role;
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - сроки вскрытия не проверялись");
                return;
            }
        }
    };
}

/// Организатор прогона.
async fn organizer(tx: &mut sqlx::PgConnection, tag: &str) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'G-147 организатор', core.now()) RETURNING id",
        format!("g147-org-{tag}@tou.test")
    )
    .fetch_one(tx)
    .await
}

/// Тендер стадии демо-стенда: прием закрыт, час вскрытия наступил, конверты
/// не вскрыты. Ровно то состояние, в котором система молчала.
async fn tender_past_its_opening(
    tx: &mut sqlx::PgConnection,
    organizer_id: Uuid,
    title: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO core.tenders
           (title, status, organizer_id, announced_at, submission_deadline, opening_at)
         VALUES ($1, 'accepting', $2, core.now() - interval '20 days',
                 core.now() - interval '1 hour', core.now() - interval '30 minutes')
         RETURNING id",
        title,
        organizer_id
    )
    .fetch_one(tx)
    .await
}

struct Row {
    action: String,
    assignee_role: String,
    rule_ref: String,
    status: String,
    overdue_now: bool,
}

async fn obligation_of(
    tx: &mut sqlx::PgConnection,
    tender_id: Uuid,
) -> Result<Option<Row>, sqlx::Error> {
    sqlx::query_as!(
        Row,
        r#"SELECT o.action, o.assignee_role::text AS "assignee_role!", o.rule_ref,
                  o.status::text AS "status!",
                  o.due_at <= core.now() AS "overdue_now!"
           FROM core.obligations o
           WHERE o.tender_id = $1 AND o.action = $2"#,
        tender_id,
        ObligationAction::HoldOpening.as_str()
    )
    .fetch_optional(tx)
    .await
}

/// G-147: у тендера с наступившим часом вскрытия появляется обязанность
/// его провести - за секретарем, со сроком в тот же час, и повторный проход
/// воркера дубля не делает.
#[tokio::test]
async fn a_due_opening_creates_one_obligation_for_the_secretary() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let tag = Uuid::now_v7().simple().to_string();
    let actor = organizer(&mut tx, &tag).await.expect("организатор");
    let tender_id = tender_past_its_opening(&mut tx, actor, &format!("G-147 тендер {tag}"))
        .await
        .expect("тендер стадии вскрытия");

    assert!(
        obligation_of(&mut tx, tender_id)
            .await
            .expect("чтение сроков")
            .is_none(),
        "до прохода воркера обязанности нет"
    );

    let scheduled = tou_db::obligations::schedule_due_openings(&mut *tx, 100)
        .await
        .expect("проход воркера");
    assert!(
        scheduled.contains(&tender_id),
        "наступивший час вскрытия обязан породить срок"
    );

    let row = obligation_of(&mut tx, tender_id)
        .await
        .expect("чтение сроков")
        .expect("обязанность поставлена");
    assert_eq!(row.action, ObligationAction::HoldOpening.as_str());
    assert_eq!(
        row.assignee_role,
        Role::Secretary.as_str(),
        "вскрытие ведет секретарь комиссии (п. 41–43)"
    );
    assert_eq!(row.rule_ref, "п. 41");
    assert_eq!(row.status, "pending");
    assert!(
        row.overdue_now,
        "срок - сам назначенный час: он уже прошел, и эскалация возьмет его первым же проходом"
    );

    // Тендер остается непереведенным: вскрытие - действие людей на заседании
    let status = sqlx::query_scalar!(
        r#"SELECT status::text AS "status!" FROM core.tenders WHERE id = $1"#,
        tender_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("статус тендера");
    assert_eq!(
        status, "accepting",
        "двигатель ставит срок, а не вскрывает конверты за комиссию"
    );

    // Второй проход - тот же срок, а не второй: воркер запускается как
    // угодно часто и в нескольких экземплярах (NFR-12)
    let again = tou_db::obligations::schedule_due_openings(&mut *tx, 100)
        .await
        .expect("повторный проход");
    assert!(!again.contains(&tender_id), "повтор дубля не создает");
    let count = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM core.obligations
           WHERE tender_id = $1 AND action = $2"#,
        tender_id,
        ObligationAction::HoldOpening.as_str()
    )
    .fetch_one(&mut *tx)
    .await
    .expect("счет сроков");
    assert_eq!(count, 1);

    // Закрывает срок само вскрытие: тот же вызов делает
    // `tou_db::admission::open_tender` в своей транзакции
    tou_db::obligations::complete(
        &mut tx,
        ObligationAction::HoldOpening,
        tou_db::obligations::Subject::tender(tender_id),
    )
    .await
    .expect("вскрытие закрывает срок");
    let row = obligation_of(&mut tx, tender_id)
        .await
        .expect("чтение сроков")
        .expect("срок на месте");
    assert_eq!(row.status, "done", "вскрытие закрывает обязанность фактом");
}

/// G-147: срок ставится только там, где час вскрытия действительно
/// наступил, а конверты еще не вскрыты. Ни назначенное на будущее, ни уже
/// вскрытое обязанности не порождает.
#[tokio::test]
async fn openings_that_are_not_due_stay_untouched() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let tag = Uuid::now_v7().simple().to_string();
    let actor = organizer(&mut tx, &tag).await.expect("организатор");

    let future = sqlx::query_scalar!(
        "INSERT INTO core.tenders
           (title, status, organizer_id, announced_at, submission_deadline, opening_at)
         VALUES ($1, 'accepting', $2, core.now() - interval '2 days',
                 core.now() + interval '12 days', core.now() + interval '13 days')
         RETURNING id",
        format!("G-147 будущее вскрытие {tag}"),
        actor
    )
    .fetch_one(&mut *tx)
    .await
    .expect("тендер с приемом заявок");

    let opened = sqlx::query_scalar!(
        "INSERT INTO core.tenders
           (title, status, organizer_id, announced_at, submission_deadline, opening_at,
            opened_at)
         VALUES ($1, 'qualification', $2, core.now() - interval '20 days',
                 core.now() - interval '8 days', core.now() - interval '7 days',
                 core.now() - interval '7 days')
         RETURNING id",
        format!("G-147 вскрытый {tag}"),
        actor
    )
    .fetch_one(&mut *tx)
    .await
    .expect("вскрытый тендер");

    tou_db::obligations::schedule_due_openings(&mut *tx, 100)
        .await
        .expect("проход воркера");

    for (tender_id, why) in [
        (future, "час вскрытия еще не наступил"),
        (opened, "конверты уже вскрыты"),
    ] {
        assert!(
            obligation_of(&mut tx, tender_id)
                .await
                .expect("чтение сроков")
                .is_none(),
            "срок поставлен там, где его быть не должно: {why}"
        );
    }
}
