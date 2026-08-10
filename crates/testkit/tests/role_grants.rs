//! Выдача и снятие ролей админом (FR-1503, FR-1902) против живой БД.
//!
//! Роль - это доступ ко всему, что политика ей разрешает (INV-POL-01), а
//! выдает и снимает ее человек. Поэтому проверяется не только результат,
//! но и след: история изменений ролей в кабинете админа (FR-1902) берется
//! из `audit.log`, и если мутация проходит мимо аудита, история молча
//! пустеет. Тестов на этот путь не было вовсе.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use tou_domain::role::Role;
use uuid::Uuid;

async fn try_pool() -> Result<Option<tou_db::Db>, sqlx::Error> {
    match tou_testkit::database_url().map_err(|e| sqlx::Error::Configuration(Box::new(e)))? {
        Some(url) => tou_db::connect(&url).await.map(Some),
        None => Ok(None),
    }
}

macro_rules! require_db {
    () => {
        match try_pool().await.expect("TESTKIT_DATABASE_URL: подключение не удалось") {
            Some(db) => db,
            None => {
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - роли не проверялись");
                return;
            }
        }
    };
}

/// Пользователь стенда: почта уникальна на прогон, чтобы тесты не мешали
/// друг другу (мутации ролей идут своими транзакциями и не откатываются).
async fn user(db: &tou_db::Db, tag: &str) -> Result<Uuid, sqlx::Error> {
    let email = format!("{tag}-{}@tou.test", Uuid::now_v7());
    sqlx::query_scalar!(
        "INSERT INTO core.users (email, full_name, password_hash, is_active)
         VALUES ($1, 'Тест ролей', 'x', true) RETURNING id",
        email
    )
    .fetch_one(db)
    .await
}

async fn cleanup(db: &tou_db::Db, ids: &[Uuid]) -> Result<(), sqlx::Error> {
    // Роли снимаются до пользователей, и не только свои: `user_id` уходит
    // каскадом, а `granted_by` - нет, поэтому удаление админа раньше
    // выданной им записи упирается во внешний ключ. Порядок внутри
    // `ANY($1)` не задан, так что без этого шага уборка была лотереей.
    sqlx::query!(
        "DELETE FROM core.role_grants WHERE user_id = ANY($1) OR granted_by = ANY($1)",
        ids
    )
    .execute(db)
    .await?;
    // Аудит append-only и остается: он и есть доказательная база (FR-1601)
    sqlx::query!("DELETE FROM core.users WHERE id = ANY($1)", ids)
        .execute(db)
        .await?;
    Ok(())
}

async fn roles_of(db: &tou_db::Db, user_id: Uuid) -> Result<Vec<Role>, sqlx::Error> {
    tou_db::users::roles_of(db, user_id).await
}

async fn audit_events(db: &tou_db::Db, row_id: Uuid) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT action FROM audit.log
          WHERE table_name = 'core.role_grants' AND row_id = $1
          ORDER BY occurred_at",
        row_id
    )
    .fetch_all(db)
    .await
}

/// FR-1503, FR-1902: выдача роли дает доступ и оставляет след в аудите;
/// повторная выдача идемпотентна и второй записи не порождает.
#[tokio::test]
async fn granting_a_role_is_idempotent_and_audited() {
    let db = require_db!();
    let admin = user(&db, "admin").await.expect("админ");
    let subject = user(&db, "subject").await.expect("пользователь");

    tou_db::users::grant_role(&db, admin, subject, Role::Finance)
        .await
        .expect("выдача роли");
    assert_eq!(
        roles_of(&db, subject).await.expect("роли"),
        vec![Role::Finance],
        "роль выдана"
    );

    tou_db::users::grant_role(&db, admin, subject, Role::Finance)
        .await
        .expect("повторная выдача не ошибка");
    assert_eq!(
        roles_of(&db, subject).await.expect("роли").len(),
        1,
        "повторная выдача не задваивает роль"
    );

    let grant_id = sqlx::query_scalar!(
        "SELECT id FROM core.role_grants WHERE user_id = $1 AND role = 'finance'",
        subject
    )
    .fetch_one(&db)
    .await
    .expect("запись о роли");
    assert_eq!(
        audit_events(&db, grant_id).await.expect("аудит"),
        vec!["INSERT".to_owned()],
        "выдача роли обязана быть в аудите ровно один раз (FR-1902)"
    );

    cleanup(&db, &[admin, subject]).await.expect("уборка");
}

/// FR-1503, FR-1902: снятие роли забирает доступ и тоже попадает в аудит -
/// иначе история кабинета админа показывает выдачу без возврата.
#[tokio::test]
async fn revoking_a_role_is_audited() {
    let db = require_db!();
    let admin = user(&db, "admin").await.expect("админ");
    let subject = user(&db, "subject").await.expect("пользователь");

    tou_db::users::grant_role(&db, admin, subject, Role::Commission)
        .await
        .expect("выдача");
    let grant_id = sqlx::query_scalar!(
        "SELECT id FROM core.role_grants WHERE user_id = $1 AND role = 'commission'",
        subject
    )
    .fetch_one(&db)
    .await
    .expect("запись о роли");

    tou_db::users::revoke_role(&db, admin, subject, Role::Commission)
        .await
        .expect("снятие");
    assert!(
        roles_of(&db, subject).await.expect("роли").is_empty(),
        "снятая роль не действует"
    );
    assert_eq!(
        audit_events(&db, grant_id).await.expect("аудит"),
        vec!["INSERT".to_owned(), "DELETE".to_owned()],
        "снятие роли обязано быть в аудите (FR-1902)"
    );

    cleanup(&db, &[admin, subject]).await.expect("уборка");
}
