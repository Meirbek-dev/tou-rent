//! Жизненный цикл учетной записи (W-07) против живой БД.
//!
//! До этой задачи учетную запись можно было только завести: сбросить ей
//! пароль, отключить ее и вернуть было нечем, и внешний участник, забывший
//! пароль, терял вместе с ним поданные заявки, договоры и депозит. Здесь
//! проверяется не результат вызова (его видно и по типу), а то, что нельзя
//! закрепить типом: состояние строки после мутации и след в аудите.
//!
//! След важен отдельно: сброс чужого пароля - действие, которое обязано быть
//! доказуемым, а `core.users` попала под аудит только этой задачей. Заодно
//! проверяется, что в журнал не уехал сам Argon2id-хеш: журнал append-only,
//! чистить его нельзя, и попавший туда секрет остается там навсегда.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - учетные записи не проверялись");
                return;
            }
        }
    };
}

/// Хеш пароля здесь не настоящий: Argon2id живет в `tou-http`, а проверяется
/// не он, а то, что строка меняется, а в журнал уходит только отпечаток.
const OLD_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c3RhcnlqLXNhbHQtMTIz$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const NEW_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$bm92eWotc2FsdC0xMjM0$BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";

/// Пользователь стенда: почта уникальна на прогон - мутации идут своими
/// транзакциями и не откатываются.
async fn user(db: &tou_db::Db, tag: &str) -> Result<(Uuid, String), sqlx::Error> {
    let email = format!("w07-{tag}-{}@tou.test", Uuid::now_v7().simple());
    let id = sqlx::query_scalar!(
        "INSERT INTO core.users (email, full_name, password_hash, is_active)
         VALUES ($1::citext, 'Тест жизненного цикла', $2, true) RETURNING id",
        email,
        OLD_HASH
    )
    .fetch_one(db)
    .await?;
    Ok((id, email))
}

async fn cleanup(db: &tou_db::Db, ids: &[Uuid]) {
    // Аудит append-only и остается: он и есть доказательная база (FR-1601)
    let _ = sqlx::query!("DELETE FROM core.users WHERE id = ANY($1)", ids)
        .execute(db)
        .await;
}

struct AuditEvent {
    actor: Option<Uuid>,
    body: String,
    old_fingerprint: Option<String>,
    new_fingerprint: Option<String>,
}

async fn audit_updates(db: &tou_db::Db, user_id: Uuid) -> Result<Vec<AuditEvent>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT actor_id,
                  payload::text                              AS "body!",
                  payload -> 'old' ->> 'password_fingerprint' AS old_fingerprint,
                  payload -> 'new' ->> 'password_fingerprint' AS new_fingerprint
             FROM audit.log
            WHERE table_name = 'core.users' AND row_id = $1 AND action = 'UPDATE'
            ORDER BY id"#,
        user_id
    )
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| AuditEvent {
            actor: row.actor_id,
            body: row.body,
            old_fingerprint: row.old_fingerprint,
            new_fingerprint: row.new_fingerprint,
        })
        .collect())
}

/// W-07: сброс пароля админом меняет строку и оставляет след с актором -
/// иначе «кто сбросил пароль победителю тендера» ответа не имеет.
///
/// Одновременно - рубеж по секрету: PHC-строки в журнале быть не должно
/// ни старой, ни новой, а отпечаток обязан смениться. Отпечаток без смены
/// означал бы, что аудит не отличает смену пароля от правки `updated_at`.
#[tokio::test]
async fn admin_password_reset_is_audited_without_the_hash() {
    let db = require_db!();
    let (admin, _) = user(&db, "admin").await.expect("админ");
    let (subject, _) = user(&db, "subject").await.expect("пользователь");

    let reset = tou_db::users::set_password(&db, admin, subject, NEW_HASH)
        .await
        .expect("сброс пароля");
    assert!(reset, "сброс выполнен по существующей записи");

    let record = tou_db::users::find_by_id(&db, subject)
        .await
        .expect("чтение записи")
        .expect("запись на месте");
    assert_eq!(
        record.password_hash.as_deref(),
        Some(NEW_HASH),
        "пароль заменен"
    );

    let events = audit_updates(&db, subject).await.expect("аудит");
    let event = events.last().expect("сброс пароля обязан быть в аудите");
    assert_eq!(
        event.actor,
        Some(admin),
        "актором события обязан быть админ, а не сама запись"
    );
    assert!(
        !event.body.contains(OLD_HASH) && !event.body.contains(NEW_HASH),
        "Argon2id-хеш уехал в append-only журнал: {}",
        event.body
    );
    assert_ne!(
        event.old_fingerprint, event.new_fingerprint,
        "отпечаток пароля не изменился - по журналу смена пароля неотличима от правки записи"
    );
    assert!(
        event.new_fingerprint.is_some(),
        "отпечаток нового пароля обязан быть в журнале"
    );

    // Несуществующая запись - не «готово», а отказ вызывающему
    assert!(
        !tou_db::users::set_password(&db, admin, Uuid::now_v7(), NEW_HASH)
            .await
            .expect("сброс по чужому id"),
        "сброс по несуществующему id обязан сообщать об отсутствии записи"
    );

    cleanup(&db, &[admin, subject]).await;
}

/// W-07: отключенная запись перестает быть входной точкой и возвращается
/// тем же путем; оба перехода - в аудите.
///
/// Проверяется именно то, на что смотрит вход (`is_active` в строке, которую
/// отдает `find_by_email`) и экстрактор сессии: снятие ролей это состояние
/// не меняло, и уволившийся продолжал входить.
#[tokio::test]
async fn deactivated_account_is_rejected_and_can_be_restored() {
    let db = require_db!();
    let (admin, _) = user(&db, "admin").await.expect("админ");
    let (subject, email) = user(&db, "subject").await.expect("пользователь");

    assert!(
        tou_db::users::set_active(&db, admin, subject, false)
            .await
            .expect("отключение"),
    );
    let record = tou_db::users::find_by_email(&db, &email)
        .await
        .expect("чтение по email")
        .expect("запись на месте");
    assert!(
        !record.is_active,
        "отключенная запись обязана приходить входу отключенной - вход сверяет именно это поле"
    );

    assert!(
        tou_db::users::set_active(&db, admin, subject, true)
            .await
            .expect("возврат"),
    );
    let record = tou_db::users::find_by_email(&db, &email)
        .await
        .expect("чтение по email")
        .expect("запись на месте");
    assert!(record.is_active, "возврат записи обязан работать");

    let events = audit_updates(&db, subject).await.expect("аудит");
    assert!(
        events.len() >= 2,
        "оба перехода состояния обязаны быть в аудите, найдено {}",
        events.len()
    );
    assert!(
        events.iter().all(|event| event.actor == Some(admin)),
        "актором отключения и возврата обязан быть тот, кто их выполнил"
    );

    assert!(
        !tou_db::users::set_active(&db, admin, Uuid::now_v7(), false)
            .await
            .expect("отключение чужого id"),
        "переключение по несуществующему id обязано сообщать об отсутствии записи"
    );

    cleanup(&db, &[admin, subject]).await;
}
