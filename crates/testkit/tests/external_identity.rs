//! Интеграционные тесты внешнего входа (FR-1502, T18) против живой БД.
//!
//! Проверяется то, что нельзя закрепить типом: связывание субъекта провайдера
//! с существующей учетной записью, происхождение ролей (`local` не снимается
//! синхронизацией с провайдером) и запись событий в аудит.
//!
//! Подключение - TESTKIT_DATABASE_URL; без переменной тесты пропускаются
//! (A-021, как и db_invariants). Каждый тест - транзакция с откатом.

use tou_db::identities::{ExternalIdentity, LinkOutcome};
use tou_domain::role::Role;
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - внешний вход не проверялся");
                return;
            }
        }
    };
}

const ISSUER: &str = "https://id.test.tou/oidc";

fn identity(subject: &str, email: &str, roles: Vec<Role>) -> ExternalIdentity {
    ExternalIdentity {
        issuer: ISSUER.to_owned(),
        subject: subject.to_owned(),
        email: email.to_owned(),
        full_name: "Тестовый сотрудник".to_owned(),
        locale: "ru".to_owned(),
        provider_login: Some(format!("{subject}@tou.local")),
        roles,
    }
}

fn unique_email(tag: &str) -> String {
    format!("t18-{tag}-{}@tou.test", Uuid::now_v7().simple())
}

/// Уборка за тестом: транзакцию `login_external` держит сам (нужен commit,
/// иначе триггеры аудита не отработают), поэтому строки удаляются явно.
async fn cleanup(db: &tou_db::Db, user_id: Uuid) {
    let _ = sqlx::query!("DELETE FROM core.users WHERE id = $1", user_id)
        .execute(db)
        .await;
}

/// Первый вход заводит учетную запись, повторный - находит ее по субъекту,
/// даже если email в провайдере сменился (`sub` стабилен, email - нет).
#[tokio::test]
async fn subject_identifies_user_across_email_change() {
    let db = require_db!();
    let subject = Uuid::now_v7().simple().to_string();

    let (created, outcome) =
        tou_db::identities::login_external(&db, &identity(&subject, &unique_email("new"), vec![]))
            .await
            .expect("первый вход");
    assert_eq!(outcome, LinkOutcome::Created);

    let renamed = unique_email("renamed");
    let (same, outcome) =
        tou_db::identities::login_external(&db, &identity(&subject, &renamed, vec![]))
            .await
            .expect("повторный вход");
    assert_eq!(outcome, LinkOutcome::Reused);
    assert_eq!(
        same.id, created.id,
        "субъект провайдера - один пользователь"
    );

    cleanup(&db, created.id).await;
}

/// Сотрудник с локальной учетной записью контура 1 не получает дубля:
/// совпавший email связывается с субъектом провайдера.
#[tokio::test]
async fn existing_local_account_is_linked_not_duplicated() {
    let db = require_db!();
    let email = unique_email("local");

    let local_id = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'argon2-заглушка', 'Локальный сотрудник', now()) RETURNING id",
        email
    )
    .fetch_one(&db)
    .await
    .expect("локальная запись");

    let (user, outcome) = tou_db::identities::login_external(
        &db,
        &identity(&Uuid::now_v7().simple().to_string(), &email, vec![]),
    )
    .await
    .expect("вход через провайдера");

    assert_eq!(outcome, LinkOutcome::Linked);
    assert_eq!(user.id, local_id);
    assert!(
        user.password_hash.is_some(),
        "локальный пароль остается рабочим - вход двумя способами"
    );

    cleanup(&db, local_id).await;
}

/// Роль, пришедшая claim'ом, снимается вслед за провайдером; роль, выданную
/// админом вручную (FR-1503), синхронизация не трогает.
#[tokio::test]
async fn provider_roles_sync_but_local_grants_survive() {
    let db = require_db!();
    let subject = Uuid::now_v7().simple().to_string();
    let email = unique_email("roles");

    let (user, _) = tou_db::identities::login_external(
        &db,
        &identity(&subject, &email, vec![Role::Secretary, Role::Commission]),
    )
    .await
    .expect("вход с ролями провайдера");

    // Роль, назначенная внутри системы
    tou_db::users::grant_role(&db, user.id, user.id, Role::Finance)
        .await
        .expect("локальная роль");

    let (_, _) =
        tou_db::identities::login_external(&db, &identity(&subject, &email, vec![Role::Secretary]))
            .await
            .expect("вход после отзыва роли в провайдере");

    let mut roles = tou_db::users::roles_of(&db, user.id).await.expect("роли");
    roles.sort_by_key(|role| role.as_str());
    assert_eq!(
        roles,
        vec![Role::Finance, Role::Secretary],
        "commission снят вслед за провайдером, finance остался локальным"
    );

    let audited = sqlx::query_scalar!(
        r#"SELECT count(*) AS "audited!" FROM audit.log
           WHERE table_name = 'core.user_identities' AND row_id IN
             (SELECT id FROM core.user_identities WHERE user_id = $1)"#,
        user.id
    )
    .fetch_one(&db)
    .await
    .expect("аудит связывания");
    assert!(
        audited > 0,
        "привязка внешней учетной записи пишется в аудит"
    );

    cleanup(&db, user.id).await;
}
