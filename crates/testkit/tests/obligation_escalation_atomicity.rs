//! Эскалация просрочки не теряет уведомления при сбое (W-11, FR-1702, FR-1302).
//!
//! Раньше отметка `escalated_at` коммитилась своей транзакцией, а уведомления
//! писались уже после нее, по одному вызову на получателя. Падение процесса
//! или ошибка на середине цикла оставляли часть получателей без уведомления
//! навсегда: выборка следующего прохода фильтрует по `escalated_at IS NULL`
//! и такой срок больше не видит. Проверяется именно это свойство: отказ на
//! шаге записи уведомлений обязан оставить обязательство неэскалированным.
//!
//! Отказ подстраивается по-настоящему, а не имитацией слоя данных: на
//! `core.notifications` временно вешается триггер, который отбивает вставку.
//! Триггер заводится отдельным подключением - роль приложения (`tou_rent_app`,
//! A-011) владельцем таблицы не является и таких прав не имеет.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use tou_db::obligations::{self, Subject};
use tou_domain::obligation::ObligationAction;
use uuid::Uuid;

async fn try_pool() -> Result<Option<tou_db::Db>, sqlx::Error> {
    // Пропуск без адреса допустим локально, но не в пайплайне (G2/G15):
    // молча пройденный интеграционный тест ничего не проверяет
    match tou_testkit::database_url().map_err(|e| sqlx::Error::Configuration(Box::new(e)))? {
        Some(url) => tou_db::connect(&url).await.map(Some),
        None => Ok(None),
    }
}

/// Подключение владельца схемы: тем же адресом, но без `SET ROLE`.
/// Нужно ровно для того, чтобы завести и убрать триггер-ломалку.
async fn try_owner_pool() -> Result<Option<tou_db::Db>, sqlx::Error> {
    match tou_testkit::database_url().map_err(|e| sqlx::Error::Configuration(Box::new(e)))? {
        Some(url) => sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .map(Some),
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - атомарность эскалации не проверялась");
                return;
            }
        }
    };
}

/// Тендер и носитель роли-исполнителя: эскалации нужен и предмет,
/// и хотя бы один получатель.
async fn fixture(db: &tou_db::Db) -> Result<(Uuid, Uuid), sqlx::Error> {
    let mut tx = db.begin().await?;

    let organizer = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'W-11 организатор', core.now()) RETURNING id",
        format!("w11-org-{}@tou.test", Uuid::now_v7().simple())
    )
    .fetch_one(&mut *tx)
    .await?;

    let secretary = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'W-11 секретарь', core.now()) RETURNING id",
        format!("w11-sec-{}@tou.test", Uuid::now_v7().simple())
    )
    .fetch_one(&mut *tx)
    .await?;

    // Роль-исполнитель срока «уведомить допущенных» - секретарь (п. 57)
    sqlx::query!(
        "INSERT INTO core.role_grants (user_id, role) VALUES ($1, 'secretary')
         ON CONFLICT DO NOTHING",
        secretary
    )
    .execute(&mut *tx)
    .await?;

    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id)
         VALUES ('W-11 тендер', 'draft', $1) RETURNING id",
        organizer
    )
    .fetch_one(&mut *tx)
    .await?;

    obligations::schedule(
        &mut tx,
        ObligationAction::NotifyAdmitted,
        Subject::tender(tender_id),
    )
    .await?;
    tx.commit().await?;

    sqlx::query!(
        "UPDATE core.obligations SET due_at = core.now() - interval '1 day' WHERE tender_id = $1",
        tender_id
    )
    .execute(db)
    .await?;

    Ok((tender_id, secretary))
}

/// FR-1702/FR-1302: отказ на записи уведомлений не съедает эскалацию.
#[tokio::test]
async fn fr1702_failed_notification_leaves_the_obligation_for_the_next_pass() {
    let db = require_db!();
    let owner = match try_owner_pool()
        .await
        .expect("TESTKIT_DATABASE_URL: подключение владельца не удалось")
    {
        Some(pool) => pool,
        None => return,
    };

    let (tender_id, recipient) = fixture(&db).await.expect("подготовка срока");

    // Ломалка: вставка уведомления отбивается так же, как отбилась бы
    // при обрыве соединения посреди рассылки
    sqlx::query(
        "CREATE FUNCTION core.w11_break_notifications() RETURNS trigger
         LANGUAGE plpgsql AS $fn$
         BEGIN
           RAISE EXCEPTION 'W-11: запись уведомления не удалась';
         END $fn$",
    )
    .execute(&owner)
    .await
    .expect("функция-ломалка");
    sqlx::query(
        "CREATE TRIGGER w11_break_notifications BEFORE INSERT ON core.notifications
         FOR EACH ROW EXECUTE FUNCTION core.w11_break_notifications()",
    )
    .execute(&owner)
    .await
    .expect("триггер-ломалка");

    let failed = obligations::take_overdue(&db).await;

    // Ломалка снимается до проверок: упавшее ожидание не должно оставить
    // после себя базу, в которую нельзя записать ни одного уведомления
    sqlx::query("DROP TRIGGER w11_break_notifications ON core.notifications")
        .execute(&owner)
        .await
        .expect("снятие триггера");
    sqlx::query("DROP FUNCTION core.w11_break_notifications()")
        .execute(&owner)
        .await
        .expect("снятие функции");

    assert!(
        failed.is_err(),
        "отказ записи уведомления обязан дойти до вызывающего, а не пройти молча"
    );

    let after_failure = sqlx::query!(
        r#"SELECT status::text AS "status!", escalated_at
           FROM core.obligations WHERE tender_id = $1"#,
        tender_id
    )
    .fetch_one(&db)
    .await
    .expect("состояние срока");
    assert_eq!(
        after_failure.status, "pending",
        "срок не эскалирован: уведомления не записаны"
    );
    assert!(
        after_failure.escalated_at.is_none(),
        "без записанных уведомлений отметки об эскалации быть не должно - \
         иначе следующий проход этот срок уже не увидит"
    );

    // Тот же срок подбирается следующим проходом - повторная попытка
    // получается сама собой, без реестра неудач
    let retried = obligations::take_overdue(&db).await.expect("второй проход");
    assert!(
        retried
            .iter()
            .any(|item| item.tender_id == Some(tender_id) && item.recipient_id == recipient),
        "следующий проход обязан подобрать тот же срок и того же получателя"
    );

    let notified = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM core.notifications
           WHERE user_id = $1 AND payload->>'tender_id' = $2::uuid::text"#,
        recipient,
        tender_id
    )
    .fetch_one(&db)
    .await
    .expect("счетчик уведомлений");
    assert_eq!(notified, 1, "получатель уведомлен ровно один раз");

    let after_retry = sqlx::query!(
        r#"SELECT status::text AS "status!", escalated_at
           FROM core.obligations WHERE tender_id = $1"#,
        tender_id
    )
    .fetch_one(&db)
    .await
    .expect("состояние срока");
    assert_eq!(after_retry.status, "overdue");
    assert!(
        after_retry.escalated_at.is_some(),
        "успешная рассылка закрывает эскалацию отметкой"
    );

    // Уборка стенда: тест оставляет за собой только аудит
    sqlx::query!(
        "DELETE FROM core.notifications WHERE payload->>'tender_id' = $1::uuid::text",
        tender_id
    )
    .execute(&db)
    .await
    .ok();
    sqlx::query!("DELETE FROM core.tenders WHERE id = $1", tender_id)
        .execute(&db)
        .await
        .ok();
    sqlx::query!("DELETE FROM core.role_grants WHERE user_id = $1", recipient)
        .execute(&db)
        .await
        .ok();
}
