//! Ссылка на запись торгов (T52, FR-306, FR-1903, п. 72) против живой БД.
//!
//! Проверяется правило, а не поле: запись существует только после того, как
//! итоги подведены, поэтому ссылка принимается в `summed_up` и `contracted`
//! и отклоняется раньше. Правка тендера - мутация домена, значит она обязана
//! оставить след в аудите (регламент А.5, FR-1601).
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use tou_db::tenders;
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - запись торгов не проверялась");
                return;
            }
        }
    };
}

const RECORDING: &str = "https://zoom.example.test/rec/t52";

struct Fixture {
    organizer: Uuid,
    tender: Uuid,
}

/// Тендер в заданном статусе. Статус выставляется при вставке: переходы
/// сторожит триггер INV-021, а тесту нужна не цепочка переходов, а состояние.
async fn tender_in(db: &tou_db::Db, status: &str) -> Result<Fixture, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let organizer = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'argon2-заглушка', 'Т52 секретарь', now()) RETURNING id",
        format!("t52-secretary-{tag}@tou.test")
    )
    .fetch_one(db)
    .await?;

    let tender = sqlx::query_scalar!(
        "INSERT INTO core.tenders (status, title, organizer_id)
         VALUES ($1::text::core.tender_status, $2, $3) RETURNING id",
        status,
        format!("Т52 тендер {tag}"),
        organizer
    )
    .fetch_one(db)
    .await?;

    Ok(Fixture { organizer, tender })
}

/// Уборка: функция сама коммитит транзакцию (иначе не отработали бы
/// аудит-триггеры), поэтому строки удаляются явно.
async fn cleanup(db: &tou_db::Db, f: &Fixture) {
    let _ = sqlx::query!("DELETE FROM core.tenders WHERE id = $1", f.tender)
        .execute(db)
        .await;
    let _ = sqlx::query!("DELETE FROM core.users WHERE id = $1", f.organizer)
        .execute(db)
        .await;
}

/// FR-306, FR-1903 (п. 72): у подведенного тендера ссылка на запись
/// сохраняется - вносит ее секретарь после итогов.
#[tokio::test]
async fn recording_is_stored_after_results() {
    let db = require_db!();
    let f = tender_in(&db, "summed_up").await.expect("тендер");

    let updated = tenders::set_recording_url(&db, f.organizer, f.tender, Some(RECORDING))
        .await
        .expect("правка ссылки")
        .expect("подведенный тендер принимает ссылку");
    assert_eq!(updated.zoom_recording_url.as_deref(), Some(RECORDING));

    // Ошибочную ссылку можно снять - это тоже правка карточки, а не пустая ссылка
    let cleared = tenders::set_recording_url(&db, f.organizer, f.tender, None)
        .await
        .expect("снятие ссылки")
        .expect("тендер найден");
    assert_eq!(cleared.zoom_recording_url, None);

    cleanup(&db, &f).await;
}

/// FR-306: до подведения итогов записи не существует - правка отклоняется.
#[tokio::test]
async fn recording_is_rejected_before_results() {
    let db = require_db!();

    for status in [
        "draft",
        "announced",
        "accepting",
        "qualification",
        "trading",
    ] {
        let f = tender_in(&db, status).await.expect("тендер");

        let outcome = tenders::set_recording_url(&db, f.organizer, f.tender, Some(RECORDING))
            .await
            .expect("запрос выполнен");
        assert!(
            outcome.is_none(),
            "в статусе {status} ссылка на запись торгов не принимается"
        );

        let stored = sqlx::query_scalar!(
            "SELECT zoom_recording_url FROM core.tenders WHERE id = $1",
            f.tender
        )
        .fetch_one(&db)
        .await
        .expect("чтение тендера");
        assert_eq!(stored, None, "в статусе {status} поле осталось пустым");

        cleanup(&db, &f).await;
    }
}

/// FR-1601: правка карточки торгов оставляет след в аудите.
#[tokio::test]
async fn recording_change_is_audited() {
    let db = require_db!();
    let f = tender_in(&db, "contracted").await.expect("тендер");

    tenders::set_recording_url(&db, f.organizer, f.tender, Some(RECORDING))
        .await
        .expect("правка ссылки")
        .expect("договорный тендер принимает ссылку");

    let audited = sqlx::query_scalar!(
        r#"SELECT count(*) AS "audited!" FROM audit.log
         WHERE table_name = 'core.tenders' AND row_id = $1 AND actor_id = $2
           AND action = 'UPDATE'"#,
        f.tender,
        f.organizer
    )
    .fetch_one(&db)
    .await
    .expect("чтение аудита");
    assert!(audited > 0, "правка тендера не попала в audit.log");

    cleanup(&db, &f).await;
}
