//! Управляемое время стенда (T68, ADR-0005) против живой БД.
//!
//! Проверяются два разных свойства. Первое — что сдвиг вообще действует на
//! правила: срок, до которого «еще можно», после сдвига становится сроком,
//! после которого «уже нельзя». Второе, не менее важное, — что сдвинуть
//! часы обычным путем невозможно: возможность подвинуть время это
//! возможность подделать юридически значимую отметку (NFR-03).
//!
//! Сдвиг выполняется внутри откатываемой транзакции владельца БД: значение
//! видно только этой транзакции, поэтому параллельные тесты и открытый
//! дев-стенд живут в обычных часах. Глобальный сдвиг в тесте сломал бы
//! всё, что рядом считает сроки.
//!
//! Подключение — TESTKIT_DATABASE_URL (A-021).

use tokio::sync::Mutex;
use uuid::Uuid;

/// Сдвигающие тесты идут по одному.
///
/// Строка сдвига на стенде одна, а ее правка берет две блокировки: строку
/// и advisory-lock hash-цепочки аудита. Два одновременных сдвига берут их
/// в разном порядке и получают дедлок. В работе этого не бывает - часы
/// двигает одна подкоманда `api time-shift`, - но тесты запускаются
/// параллельно, и очередь тут не украшение, а условие воспроизводимости.
static CLOCK: Mutex<()> = Mutex::const_new(());

async fn try_pools() -> Result<Option<(tou_db::Db, sqlx::PgPool)>, sqlx::Error> {
    let required =
        tou_testkit::database_url().map_err(|e| sqlx::Error::Configuration(Box::new(e)))?;
    let Some(url) = required else {
        return Ok(None);
    };
    let app = tou_db::connect(&url).await?;
    // Отдельный пул без `SET ROLE`: сдвиг часов доступен только владельцу,
    // и это ровно тот рубеж, который проверяет `app_role_cannot_shift`
    let owner = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await?;
    Ok(Some((app, owner)))
}

macro_rules! require_db {
    () => {
        match try_pools()
            .await
            .expect("TESTKIT_DATABASE_URL: подключение не удалось")
        {
            Some(pools) => pools,
            None => {
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан — время не проверялось");
                return;
            }
        }
    };
}

/// Запись журнала регистрации: дедлайн стережет триггер (INV-037).
async fn add_entry(conn: &mut sqlx::PgConnection, tender: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO core.journal_entries (tender_id, entry_kind, note)
         VALUES ($1, 'application_submitted', 'Т68')",
        tender
    )
    .execute(conn)
    .await
    .map(|_| ())
}

/// Публикация объявления: окно в десять дней стережет триггер (FR-303).
async fn publish(conn: &mut sqlx::PgConnection, tender: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE core.tenders SET status = 'announced' WHERE id = $1",
        tender
    )
    .execute(conn)
    .await
    .map(|_| ())
}

/// Без сдвига `core.now()` — это обычное серверное время (NFR-03).
#[tokio::test]
async fn core_now_equals_now_without_shift() {
    let (app, _owner) = require_db!();

    let drift =
        sqlx::query_scalar!(r#"SELECT extract(epoch FROM core.now() - now())::float8 AS "drift!""#)
            .fetch_one(&app)
            .await
            .expect("разница часов");

    assert!(
        drift.abs() < 1.0,
        "стенд оставлен со сдвигом {drift} с — на проде это подделка отметок"
    );
}

/// Рубеж 1 (ADR-0005): роль приложения сдвинуть часы не может в принципе.
/// Это отсутствие привилегии, а не проверка, которую можно забыть.
#[tokio::test]
async fn app_role_cannot_shift_the_clock() {
    let (app, _owner) = require_db!();

    let denied = sqlx::query!("UPDATE refdata.clock_offset SET shift = '11 days' WHERE id")
        .execute(&app)
        .await;

    let error = denied.expect_err("роль приложения не должна двигать часы");
    let message = error.to_string();
    assert!(
        message.contains("permission denied") || message.contains("нет прав"),
        "ожидался отказ по правам, получено: {message}"
    );
}

/// Главное свойство: сдвиг двигает границу, по которой правило решает
/// «еще можно» или «уже нельзя». Здесь это дедлайн приема заявок (INV-037,
/// п. 37–39) — та самая граница, из-за которой сквозной сценарий не мог
/// пройти путь целиком и стартовал с середины.
#[tokio::test]
async fn shift_moves_the_deadline_gate() {
    let _serialized = CLOCK.lock().await;
    let (_app, owner) = require_db!();
    let mut tx = owner.begin().await.expect("begin");
    let tag = Uuid::now_v7().simple().to_string();

    let organizer = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'argon2-заглушка', 'Т68 организатор', now()) RETURNING id",
        format!("t68-organizer-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await
    .expect("организатор");

    // Прием открыт еще пять дней
    let tender = sqlx::query_scalar!(
        "INSERT INTO core.tenders (status, title, organizer_id, submission_deadline, opening_at)
         VALUES ('accepting', $1, $2, now() + interval '5 days', now() + interval '12 days')
         RETURNING id",
        format!("Т68 тендер {tag}"),
        organizer
    )
    .fetch_one(&mut *tx)
    .await
    .expect("тендер");

    add_entry(&mut tx, tender)
        .await
        .expect("до дедлайна запись принимается");

    // Сдвиг виден только этой транзакции: соседние прогоны остаются
    // в обычных часах
    sqlx::query!("UPDATE refdata.clock_offset SET shift = interval '10 days' WHERE id")
        .execute(&mut *tx)
        .await
        .expect("сдвиг часов");

    let shifted = sqlx::query_scalar!(
        r#"SELECT extract(epoch FROM core.now() - now())::float8 AS "shifted!""#
    )
    .fetch_one(&mut *tx)
    .await
    .expect("разница часов");
    assert!(shifted > 9.0 * 86_400.0, "сдвиг не применился: {shifted} с");

    let refused = add_entry(&mut tx, tender)
        .await
        .expect_err("после дедлайна прием закрыт");
    assert!(
        refused.to_string().contains("INV-037"),
        "ожидался отказ INV-037, получено: {refused}"
    );

    // Транзакция откатывается: ни тендера, ни сдвига не остается
}

/// FR-303: то же для окна публикации — именно из-за него сквозной сценарий
/// не мог довести свежий тендер до вскрытия внутри прогона.
#[tokio::test]
async fn shift_moves_the_publication_window() {
    let _serialized = CLOCK.lock().await;
    let (_app, owner) = require_db!();
    let mut tx = owner.begin().await.expect("begin");
    let tag = Uuid::now_v7().simple().to_string();

    let organizer = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'argon2-заглушка', 'Т68 организатор', now()) RETURNING id",
        format!("t68-publisher-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await
    .expect("организатор");

    let object = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Т68 помещение', 'г. Павлодар, ул. Тестовая, 68', 42)
         RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await
    .expect("объект");

    let tender = sqlx::query_scalar!(
        "INSERT INTO core.tenders (status, title, organizer_id, submission_deadline, opening_at)
         VALUES ('draft', $1, $2, now() + interval '9 days', now() + interval '9 days')
         RETURNING id",
        format!("Т68 тендер {tag}"),
        organizer
    )
    .fetch_one(&mut *tx)
    .await
    .expect("тендер");

    sqlx::query!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                base_rate_monthly, guarantee_fee, rate_calculation)
         VALUES ($1, 1, $2, 'Т68 назначение', 12, 21000, 21000, '{}'::jsonb)",
        tender,
        object
    )
    .execute(&mut *tx)
    .await
    .expect("лот");

    // Отказ обрывает транзакцию целиком, поэтому ожидаемая неудача идет
    // под точкой сохранения — иначе следующий шаг проверять было бы нечем
    sqlx::query!("SAVEPOINT before_publish")
        .execute(&mut *tx)
        .await
        .expect("точка сохранения");

    // Девяти дней до вскрытия мало (FR-303)
    let refused = publish(&mut tx, tender)
        .await
        .expect_err("публикация раньше срока");
    assert!(
        refused.to_string().contains("FR-303"),
        "ожидался отказ FR-303, получено: {refused}"
    );

    sqlx::query!("ROLLBACK TO SAVEPOINT before_publish")
        .execute(&mut *tx)
        .await
        .expect("возврат к точке сохранения");

    // Сдвиг назад отодвигает «сегодня» от даты вскрытия, и окно набирается
    sqlx::query!("UPDATE refdata.clock_offset SET shift = interval '-3 days' WHERE id")
        .execute(&mut *tx)
        .await
        .expect("сдвиг часов");

    publish(&mut tx, tender)
        .await
        .expect("после сдвига окно набирается");

    let announced = sqlx::query_scalar!(
        "SELECT announced_at FROM core.tenders WHERE id = $1",
        tender
    )
    .fetch_one(&mut *tx)
    .await
    .expect("отметка публикации");
    assert!(
        announced.is_some(),
        "отметка публикации ставится теми же часами, что и проверка"
    );
}

/// Рубеж 3 (ADR-0005): сдвиг часов не прячется - он попадает в аудит
/// наравне с мутацией домена (FR-1601).
#[tokio::test]
async fn shift_is_audited() {
    let _serialized = CLOCK.lock().await;
    let (_app, owner) = require_db!();
    let mut tx = owner.begin().await.expect("begin");

    // Заведомо узнаваемая величина: счетчик записей не годится - строка
    // сдвига одна на стенд, и соседние тесты берут на нее блокировку
    sqlx::query!("UPDATE refdata.clock_offset SET shift = interval '4321 seconds' WHERE id")
        .execute(&mut *tx)
        .await
        .expect("сдвиг часов");

    let logged = sqlx::query_scalar!(
        r#"SELECT count(*) AS "logged!" FROM audit.log
         WHERE table_name = 'refdata.clock_offset'
           AND action = 'UPDATE'
           AND payload->'new'->>'shift' LIKE '%01:12:01%'"#
    )
    .fetch_one(&mut *tx)
    .await
    .expect("чтение аудита");

    assert_eq!(logged, 1, "сдвиг часов не попал в audit.log");
}
