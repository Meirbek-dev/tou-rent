//! Гейт G15: аудит-инварианты (FR-1601, INV-A01, INV-AUDIT).
//!
//! Перечень таблиц с обязательным audit-триггером ведется в
//! `specs/INVENTORY.md` - тест читает его оттуда, а не дублирует список:
//! добавили таблицу в перечень, забыли триггер - пайплайн красный.
//! Заодно проверяется непрерывность hash-цепочки (INV-A01).
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use std::collections::BTreeSet;

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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - G15 не проверялся");
                return;
            }
        }
    };
}

/// Перечень INV-AUDIT из `specs/INVENTORY.md`: таблицы вида `core.xxx`,
/// перечисленные в разделе после заголовка «INV-AUDIT».
fn inventory_tables() -> BTreeSet<String> {
    let inventory = include_str!("../../../specs/INVENTORY.md");
    let section = inventory
        .split_once("## INV-AUDIT")
        .map(|(_, tail)| tail)
        .unwrap_or_default();

    let mut tables = BTreeSet::new();
    let mut rest = section;
    while let Some(start) = rest.find("`core.") {
        rest = &rest[start + 1..];
        if let Some(end) = rest.find('`') {
            tables.insert(rest[..end].to_owned());
            rest = &rest[end + 1..];
        }
    }
    tables
}

/// G15 (FR-1601): каждая таблица перечня INV-AUDIT имеет триггер
/// `audit.record()`.
#[tokio::test]
async fn g15_every_inventory_table_is_audited() {
    let db = require_db!();

    let expected = inventory_tables();
    assert!(
        expected.len() >= 20,
        "перечень INV-AUDIT прочитан из specs/INVENTORY.md: {expected:?}"
    );

    let audited = sqlx::query_scalar!(
        r#"SELECT n.nspname || '.' || c.relname AS "qualified!"
         FROM pg_trigger t
         JOIN pg_class c ON c.oid = t.tgrelid
         JOIN pg_namespace n ON n.oid = c.relnamespace
         JOIN pg_proc p ON p.oid = t.tgfoid
         JOIN pg_namespace pn ON pn.oid = p.pronamespace
         WHERE NOT t.tgisinternal AND pn.nspname = 'audit' AND p.proname = 'record'"#
    )
    .fetch_all(&db)
    .await
    .expect("триггеры аудита");
    let audited: BTreeSet<String> = audited.into_iter().collect();

    let missing: Vec<&String> = expected.difference(&audited).collect();
    assert!(
        missing.is_empty(),
        "таблицы перечня INV-AUDIT без audit-триггера: {missing:?}"
    );
}

/// INV-A01: hash-цепочка аудита непрерывна на всей базе.
#[tokio::test]
async fn inv_a01_audit_chain_is_continuous() {
    let db = require_db!();

    let intact = sqlx::query_scalar!(r#"SELECT audit.verify_chain() AS "intact!""#)
        .fetch_one(&db)
        .await
        .expect("проверка цепочки");
    assert!(intact, "INV-A01: hash-цепочка audit.log разорвана");
}

/// INV-A01: та же сверка, но тем путем, которым ее делает воркер.
///
/// Проверка существовала с первой миграции и вызывалась только из теста
/// выше - в работающей системе цепочку не сверял никто, и разрыв всплыл бы
/// в момент разбирательства. Теперь это делает фоновый проход, и путь до
/// него должен быть под тестом наравне с самой функцией.
#[tokio::test]
async fn audit_chain_status_is_readable_by_the_worker() {
    let db = require_db!();

    let status = tou_db::audit::verify_chain(&db)
        .await
        .expect("сверка цепочки");
    assert!(status.intact, "INV-A01: hash-цепочка audit.log разорвана");
    assert!(
        status.entries > 0,
        "журнал аудита пуст - сверять нечего, проверьте стенд"
    );
}

/// Аудит append-only: запись нельзя ни изменить, ни удалить (FR-1601).
#[tokio::test]
async fn audit_log_is_append_only() {
    let db = require_db!();

    // Правки перечислены отдельными вызовами, а не циклом по массиву:
    // текст запроса макросу нужен литералом - из переменной он его не увидит
    macro_rules! rejected {
        ($statement:literal) => {{
            let error = sqlx::query!($statement)
                .execute(&db)
                .await
                .expect_err("правка журнала аудита обязана быть отклонена")
                .to_string();
            assert!(
                error.contains("append-only")
                    || error.contains("INV-A01")
                    || error.contains("permission denied"),
                "{}: {error}",
                $statement
            );
        }};
    }

    rejected!("UPDATE audit.log SET action = 'INSERT' WHERE id = (SELECT max(id) FROM audit.log)");
    rejected!("DELETE FROM audit.log WHERE id = (SELECT max(id) FROM audit.log)");
}

/// G15, обратное направление: каждая мутируемая таблица `core` аудируется.
///
/// Прежний G15 проверял только «у каждой таблицы перечня есть триггер», то
/// есть таблица, забытая в перечне, оставалась зеленой навсегда. Круг 2
/// гаунтлета показал цену: `core.objects`, `core.auctions` и
/// `core.ledger_accounts` мутировались без единого события, и живой прогон
/// create → update → delete объекта давал ноль строк в `audit.log`.
///
/// Поэтому направление здесь обратное и не зависит от текста INVENTORY.md:
/// перечень берется из прав роли приложения. Если роль может писать в
/// таблицу схемы `core`, у таблицы обязан быть триггер `audit.record*` -
/// либо она названа исключением ниже, с обоснованием.
#[tokio::test]
async fn g15_every_mutable_core_table_is_audited() {
    let db = require_db!();

    /// Осознанные исключения: не факты домена, а внутренняя механика.
    /// `journal_counters` - счетчик номеров записей журнала (значение
    /// выводится из самих записей, которые аудируются);
    /// `account_verifications` - одноразовые коды подтверждения
    /// регистрации, живут минутами и гасятся по `expires_at`.
    const EXEMPT: [&str; 2] = ["account_verifications", "journal_counters"];

    let unaudited = sqlx::query_scalar!(
        r#"SELECT c.relname AS "table!"
           FROM pg_class c
           JOIN pg_namespace n ON n.oid = c.relnamespace
           WHERE n.nspname = 'core' AND c.relkind = 'r'
             AND (has_table_privilege('tou_rent_app', c.oid, 'INSERT')
               OR has_table_privilege('tou_rent_app', c.oid, 'UPDATE')
               OR has_table_privilege('tou_rent_app', c.oid, 'DELETE'))
             AND NOT EXISTS (
               SELECT 1 FROM pg_trigger t
               JOIN pg_proc p       ON p.oid = t.tgfoid
               JOIN pg_namespace pn ON pn.oid = p.pronamespace
               WHERE t.tgrelid = c.oid AND NOT t.tgisinternal
                 AND pn.nspname = 'audit' AND p.proname LIKE 'record%'
             )
           ORDER BY 1"#
    )
    .fetch_all(&db)
    .await
    .expect("таблицы без аудита");

    let missing: Vec<String> = unaudited
        .into_iter()
        .filter(|table| !EXEMPT.contains(&table.as_str()))
        .collect();

    assert!(
        missing.is_empty(),
        "мутируемые таблицы core без audit-триггера: {missing:?}. \
         Регламент А.5: каждая мутация домена пишет audit-событие. \
         Добавьте CREATE TRIGGER audit_record ... EXECUTE FUNCTION audit.record() \
         и внесите таблицу в перечень INV-AUDIT (specs/INVENTORY.md), \
         либо назовите ее исключением в EXEMPT с обоснованием"
    );
}
