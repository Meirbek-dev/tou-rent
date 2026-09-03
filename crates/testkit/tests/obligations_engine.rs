//! Двигатель обязательств против живой БД (T20, FR-1702).
//!
//! Проверяется жизненный цикл срока: постановка событием (идемпотентная),
//! закрытие исполнением и эскалация просрочки ровно один раз.
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use tou_db::obligations::{self, Subject};
use tou_domain::obligation::ObligationAction;
use tou_domain::role::Role;
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - сроки не проверялись");
                return;
            }
        }
    };
}

/// Тендер-пустышка: обязательству нужен предмет, а не процесс целиком.
async fn tender(tx: &mut sqlx::PgConnection) -> Result<Uuid, sqlx::Error> {
    let organizer = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'Т20 организатор', now()) RETURNING id",
        format!("t20-{}@tou.test", Uuid::now_v7().simple())
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id) VALUES ('Т20 тендер', 'draft', $1)
         RETURNING id",
        organizer
    )
    .fetch_one(tx)
    .await
}

/// Срок ставится событием один раз и закрывается исполнением (п. 54).
#[tokio::test]
async fn fr1702_schedule_is_idempotent_and_completed_by_the_event() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let tender_id = tender(&mut tx).await.expect("тендер");

    let first = obligations::schedule(
        &mut tx,
        ObligationAction::AdmissionProtocol,
        Subject::tender(tender_id),
    )
    .await
    .expect("постановка срока");
    assert!(first.is_some(), "первое событие ставит срок");

    let second = obligations::schedule(
        &mut tx,
        ObligationAction::AdmissionProtocol,
        Subject::tender(tender_id),
    )
    .await
    .expect("повторное событие");
    assert!(second.is_none(), "повторное событие срок не двоит");

    // Срок отсчитан по производственному календарю (FR-1701): не выходной
    let due = sqlx::query!(
        r#"SELECT extract(isodow FROM (o.due_at AT TIME ZONE 'Asia/Almaty'))::int AS "dow!",
                  o.rule_ref, o.assignee_role::text AS "role!"
           FROM core.obligations o WHERE o.tender_id = $1"#,
        tender_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("срок");
    assert!(due.dow < 6, "срок не может истекать в выходной (п. 54)");
    assert_eq!(due.rule_ref, "п. 54");
    assert_eq!(due.role, Role::Secretary.as_str());

    obligations::complete(
        &mut tx,
        ObligationAction::AdmissionProtocol,
        Subject::tender(tender_id),
    )
    .await
    .expect("исполнение");

    let status = sqlx::query_scalar!(
        r#"SELECT status::text AS "status!" FROM core.obligations WHERE tender_id = $1"#,
        tender_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("статус");
    assert_eq!(
        status, "done",
        "исполнение закрывает срок без участия человека"
    );
}

/// Просрочка эскалируется ровно один раз, получатели - носители роли (п. 57).
#[tokio::test]
async fn fr1702_overdue_is_escalated_once() {
    let db = require_db!();

    // Транзакция здесь не годится: воркер ходит своим подключением
    let mut setup = db.begin().await.expect("begin");
    let tender_id = tender(&mut setup).await.expect("тендер");
    obligations::schedule(
        &mut setup,
        ObligationAction::NotifyAdmitted,
        Subject::tender(tender_id),
    )
    .await
    .expect("срок");
    // Получатель эскалации: у роли-исполнителя должен быть хоть один
    // носитель, иначе воронка уведомления пуста и просрочка не приходит
    // никому. Заводится тестом, а не берется из данных стенда - тесты
    // идут против своей БД, а seed на ней не выполнялся (G-021)
    let recipient = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'Т20 секретарь', core.now()) RETURNING id",
        format!("t20s-{}@tou.test", Uuid::now_v7().simple())
    )
    .fetch_one(&mut *setup)
    .await
    .expect("секретарь");
    setup.commit().await.expect("commit");

    tou_db::users::grant_role(&db, recipient, recipient, Role::Secretary)
        .await
        .expect("роль secretary");

    sqlx::query!(
        "UPDATE core.obligations SET due_at = now() - interval '1 day' WHERE tender_id = $1",
        tender_id
    )
    .execute(&db)
    .await
    .expect("срок в прошлом");

    let first = obligations::take_overdue(&db).await.expect("первый проход");
    assert!(
        first.iter().any(|item| item.tender_id == Some(tender_id)),
        "просроченный срок попадает в эскалацию"
    );
    assert!(
        first
            .iter()
            .all(|item| item.rule_ref.starts_with("п. ") && !item.action.is_empty()),
        "в уведомление уходят действие и пункт Правил"
    );

    let second = obligations::take_overdue(&db).await.expect("второй проход");
    assert!(
        !second.iter().any(|item| item.tender_id == Some(tender_id)),
        "повторный проход воркера того же срока не эскалирует"
    );

    let status = sqlx::query_scalar!(
        r#"SELECT status::text AS "status!" FROM core.obligations WHERE tender_id = $1"#,
        tender_id
    )
    .fetch_one(&db)
    .await
    .expect("статус");
    assert_eq!(status, "overdue");

    sqlx::query!("DELETE FROM core.tenders WHERE id = $1", tender_id)
        .execute(&db)
        .await
        .ok();
}

/// Договор-пустышка: половина сроков процесса висит на договоре, а не
/// на тендере (возврат взноса, регистрация, акт приема-передачи).
async fn contract(tx: &mut sqlx::PgConnection) -> Result<Uuid, sqlx::Error> {
    let tenant = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'Т20 наниматель', now()) RETURNING id",
        format!("t20c-{}@tou.test", Uuid::now_v7().simple())
    )
    .fetch_one(&mut *tx)
    .await?;

    let object = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Т20 помещение', 'г. Павлодар, ул. Ломова, 64', 20.00)
         RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query_scalar!(
        "INSERT INTO core.contracts (object_id, tenant_id, monthly_rate)
         VALUES ($1, $2, 36000.00) RETURNING id",
        object,
        tenant
    )
    .fetch_one(tx)
    .await
}

/// Рубеж G-084/G-085: срок, привязанный к договору, называет свой договор -
/// и в дашборде «мои сроки», и в уведомлении о просрочке (FR-1302, FR-1702).
///
/// Пока `contract_id` не выходил наружу, половина сроков процесса вела
/// в никуда: у срока по договору тендера нет, а больше предмета в ответе
/// не было - ни ссылки построить, ни понять, о чем речь.
#[tokio::test]
async fn fr1702_a_contract_bound_deadline_names_its_contract() {
    let db = require_db!();

    let mut setup = db.begin().await.expect("begin");
    let contract_id = contract(&mut setup).await.expect("договор");
    obligations::schedule(
        &mut setup,
        ObligationAction::DepositPayment,
        Subject::contract(contract_id),
    )
    .await
    .expect("срок депозита (п. 132)");
    // Получатель эскалации: у роли-исполнителя должен быть хоть один
    // носитель, иначе воркеру некому адресовать уведомление
    let recipient = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'Т20 финансист', now()) RETURNING id",
        format!("t20f-{}@tou.test", Uuid::now_v7().simple())
    )
    .fetch_one(&mut *setup)
    .await
    .expect("финансист");
    setup.commit().await.expect("commit");

    tou_db::users::grant_role(&db, recipient, recipient, Role::Finance)
        .await
        .expect("роль finance");

    // Дашборд «мои сроки» (FR-1702)
    let page = obligations::for_roles(&db, &[Role::Finance], None, 200)
        .await
        .expect("мои сроки");
    let listed = page
        .iter()
        .find(|item| item.contract_id == Some(contract_id))
        .expect("срок по договору виден в дашборде со своим договором");
    assert!(
        listed.tender_id.is_none(),
        "у срока по договору тендера нет - предмет опознается только договором"
    );

    sqlx::query!(
        "UPDATE core.obligations SET due_at = now() - interval '1 day' WHERE contract_id = $1",
        contract_id
    )
    .execute(&db)
    .await
    .expect("срок в прошлом");

    let escalated = obligations::take_overdue(&db).await.expect("эскалация");
    assert!(
        escalated
            .iter()
            .any(|item| item.contract_id == Some(contract_id)),
        "просроченный срок договора приходит воркеру вместе с договором"
    );

    let payload = sqlx::query_scalar!(
        r#"SELECT payload ->> 'contract_id' AS "contract_id?"
           FROM core.notifications
           WHERE user_id = $1 AND kind = 'obligation_overdue'
           ORDER BY created_at DESC LIMIT 1"#,
        recipient
    )
    .fetch_one(&db)
    .await
    .expect("уведомление о просрочке");
    assert_eq!(
        payload.as_deref(),
        Some(contract_id.to_string().as_str()),
        "в теле уведомления обязан быть договор - иначе ссылку строить не из чего"
    );

    let _ = sqlx::query!(
        "DELETE FROM core.notifications WHERE user_id = $1",
        recipient
    )
    .execute(&db)
    .await;
    let _ = sqlx::query!(
        "DELETE FROM core.obligations WHERE contract_id = $1",
        contract_id
    )
    .execute(&db)
    .await;
    let _ = sqlx::query!("DELETE FROM core.contracts WHERE id = $1", contract_id)
        .execute(&db)
        .await;
}
