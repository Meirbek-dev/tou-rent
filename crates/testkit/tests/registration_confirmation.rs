//! Повторная отправка кода подтверждения регистрации (FR-1501, G-073).
//!
//! До этой задачи неподтвержденная регистрация была тупиком: код живет
//! пятнадцать минут, после пяти неверных попыток перестает подходить вовсе,
//! а взять новый было негде - повторная регистрация упиралась в занятый
//! email, вход отвечал отказом. Здесь проверяется то, на чем держится выход
//! из этого состояния: кого маршрут `POST /auth/resend-confirmation` считает
//! ждущим подтверждения и становится ли новый код действующим.
//!
//! Против живой БД (A-021): «действующий код» - это условие выборки
//! [`tou_db::users::active_verification`] (срок, попытки, погашение), а не
//! утверждение приложения. Подключение - TESTKIT_DATABASE_URL.

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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - подтверждение не проверялось");
                return;
            }
        }
    };
}

/// Хеши здесь не настоящие: Argon2id живет в `tou-http`, а проверяется не он,
/// а то, какую строку выборка считает действующим кодом.
const PASSWORD_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZS1zYWx0LTEyMzQ$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const FIRST_CODE_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$cGVydnlqLXNhbHQtMTIz$BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
const SECOND_CODE_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$dnRvcm9qLXNhbHQtMTIzNA$CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC";

/// Учетная запись стенда: почта уникальна на прогон - вставки идут своими
/// транзакциями и не откатываются.
async fn user(
    db: &tou_db::Db,
    tag: &str,
    confirmed: bool,
    active: bool,
) -> Result<(Uuid, String), sqlx::Error> {
    let email = format!("g073-{tag}-{}@tou.test", Uuid::now_v7().simple());
    let id = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at, is_active)
         VALUES ($1::citext, $2, 'Проверка подтверждения',
                 CASE WHEN $3 THEN core.now() ELSE NULL END, $4)
         RETURNING id",
        email,
        PASSWORD_HASH,
        confirmed,
        active
    )
    .fetch_one(db)
    .await?;
    Ok((id, email))
}

async fn cleanup(db: &tou_db::Db, ids: &[Uuid]) {
    // Коды уходят вместе с учетной записью (ON DELETE CASCADE); аудит
    // append-only и остается - он и есть доказательная база (FR-1601)
    let _ = sqlx::query!("DELETE FROM core.users WHERE id = ANY($1)", ids)
        .execute(db)
        .await;
}

/// Рубеж G-073: новый код становится действующим даже там, где прежний
/// уже ничем не помогает, - попытки исчерпаны.
///
/// Исчерпанные попытки выбраны намеренно: это то самое состояние, из
/// которого до появления маршрута выхода не было совсем. Просроченный код
/// хотя бы истекал сам; код с пятью неудачами не оживал никогда.
#[tokio::test]
async fn a_reissued_code_revives_an_account_whose_attempts_ran_out() {
    let db = require_db!();
    let (user_id, email) = user(&db, "reissue", false, true).await.expect("участник");

    tou_db::users::issue_verification(&db, user_id, "email", FIRST_CODE_HASH)
        .await
        .expect("первый код");
    let first = tou_db::users::active_verification(&db, &email, "email")
        .await
        .expect("выборка кода")
        .expect("выданный код действует");
    assert_eq!(first.code_hash, FIRST_CODE_HASH);
    assert_eq!(first.user_id, user_id);

    // Пять неверных попыток - код перестает подходить (тот же счетчик,
    // что и у `confirm-registration`)
    for _ in 0..5 {
        tou_db::users::record_verification_failure(&db, first.id)
            .await
            .expect("неудачная попытка");
    }
    assert!(
        tou_db::users::active_verification(&db, &email, "email")
            .await
            .expect("выборка кода")
            .is_none(),
        "код с исчерпанными попытками действующим быть не должен"
    );

    tou_db::users::issue_verification(&db, user_id, "email", SECOND_CODE_HASH)
        .await
        .expect("новый код");
    let second = tou_db::users::active_verification(&db, &email, "email")
        .await
        .expect("выборка кода")
        .expect("новый код действует");
    assert_eq!(
        second.code_hash, SECOND_CODE_HASH,
        "действующим стал именно новый код, а не прежний"
    );
    assert_ne!(second.id, first.id);

    cleanup(&db, &[user_id]).await;
}

/// Кому маршрут вообще отправляет код: только живой и неподтвержденной
/// учетной записи.
///
/// Ответ у маршрута один на все случаи (204) - иначе он перечисляет
/// учетные записи (NFR-07), - поэтому решение принимается здесь, и здесь
/// же его видно: подтвержденной, отключенной и несуществующей записи
/// отправлять нечего.
#[tokio::test]
async fn only_a_live_unconfirmed_account_is_offered_a_new_code() {
    let db = require_db!();
    let (waiting, waiting_email) = user(&db, "waiting", false, true).await.expect("ждет");
    let (confirmed, confirmed_email) = user(&db, "done", true, true).await.expect("подтвержден");
    let (disabled, disabled_email) = user(&db, "off", false, false).await.expect("отключен");

    assert_eq!(
        tou_db::users::pending_confirmation(&db, &waiting_email, "email")
            .await
            .expect("поиск"),
        Some(waiting),
        "неподтвержденная живая запись ждет кода"
    );
    assert_eq!(
        tou_db::users::pending_confirmation(&db, &confirmed_email, "email")
            .await
            .expect("поиск"),
        None,
        "подтвержденной записи новый код не нужен"
    );
    assert_eq!(
        tou_db::users::pending_confirmation(&db, &disabled_email, "email")
            .await
            .expect("поиск"),
        None,
        "отключенной записи (W-07) код не отправляется"
    );
    assert_eq!(
        tou_db::users::pending_confirmation(&db, "g073-nobody@tou.test", "email")
            .await
            .expect("поиск"),
        None,
        "неизвестного адреса маршрут не знает"
    );

    // Канал считается отдельно: подтвержденный email не закрывает SMS
    assert_eq!(
        tou_db::users::pending_confirmation(&db, &confirmed_email, "sms")
            .await
            .expect("поиск"),
        Some(confirmed),
        "подтвержден email, а не телефон"
    );

    cleanup(&db, &[waiting, confirmed, disabled]).await;
}
