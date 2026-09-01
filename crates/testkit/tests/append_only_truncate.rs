//! Сторожа append-only переживают TRUNCATE и смену replication_role
//! (INV-A01 и соседние инварианты).
//!
//! Построчный запрет UPDATE/DELETE стоял с первых миграций, но `TRUNCATE`
//! не вызывает строковых триггеров вовсе: журнал аудита, история ставок и
//! книга проводок стирались одним оператором, после чего
//! `audit.verify_chain()` объявлял пустую цепочку целой. Запрет добавлен
//! миграцией `20260901150000_append_only_truncate_guard.sql`.
//!
//! Сама миграция обходит каталог один раз, а sqlx не накатывает ее повторно -
//! значит, следующая append-only таблица приедет со строковым сторожем и без
//! TRUNCATE-сторожа, и никто этого не заметит. Держит парность этот тест:
//! у каждого построчного `core.forbid_mutation` обязан быть брат на TRUNCATE.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - парность сторожей не проверялась");
                return;
            }
        }
    };
}

/// Таблицы с построчным запретом, у которых нет запрета на TRUNCATE.
///
/// Бит 32 в `tgtype` - TRIGGER_TYPE_TRUNCATE: им и различаются братья.
#[tokio::test]
async fn every_append_only_table_also_forbids_truncate() {
    let db = require_db!();

    let unguarded = sqlx::query_scalar!(
        r#"SELECT n.nspname || '.' || c.relname AS "qualified!"
           FROM pg_trigger t
           JOIN pg_class c     ON c.oid = t.tgrelid
           JOIN pg_namespace n ON n.oid = c.relnamespace
           JOIN pg_proc p      ON p.oid = t.tgfoid
           WHERE NOT t.tgisinternal
             AND p.proname = 'forbid_mutation'
             AND (t.tgtype & 32) = 0
             AND NOT EXISTS (
               SELECT 1
               FROM pg_trigger tt
               JOIN pg_class ct ON ct.oid = tt.tgrelid
               JOIN pg_proc pt  ON pt.oid = tt.tgfoid
               WHERE ct.oid = c.oid
                 AND NOT tt.tgisinternal
                 AND pt.proname = 'forbid_mutation'
                 AND (tt.tgtype & 32) <> 0
             )"#
    )
    .fetch_all(&db)
    .await
    .expect("сверка сторожей");

    assert!(
        unguarded.is_empty(),
        "append-only таблицы без запрета TRUNCATE: {unguarded:?}. \
         Строковый триггер на TRUNCATE не срабатывает - таблица стирается \
         одним оператором. Добавьте BEFORE TRUNCATE ... FOR EACH STATEMENT \
         EXECUTE FUNCTION core.forbid_mutation('<код>')"
    );
}

/// Сторожа должны быть ALWAYS, а не ORIGIN.
///
/// Триггер по умолчанию создается с `tgenabled = 'O'`, а такой молчит при
/// `session_replication_role = 'replica'`. Это не экзотика: ровно этот режим
/// включает `pg_restore --disable-triggers`, то есть append-only таблицу
/// можно было бы очистить, даже не ставя такой цели.
#[tokio::test]
async fn append_only_guards_survive_replica_mode() {
    let db = require_db!();

    let origin_only = sqlx::query_scalar!(
        r#"SELECT n.nspname || '.' || c.relname || ' / ' || t.tgname AS "qualified!"
           FROM pg_trigger t
           JOIN pg_class c     ON c.oid = t.tgrelid
           JOIN pg_namespace n ON n.oid = c.relnamespace
           JOIN pg_proc p      ON p.oid = t.tgfoid
           WHERE NOT t.tgisinternal
             AND p.proname = 'forbid_mutation'
             AND t.tgenabled <> 'A'"#
    )
    .fetch_all(&db)
    .await
    .expect("режим сторожей");

    assert!(
        origin_only.is_empty(),
        "сторожа append-only не в режиме ALWAYS: {origin_only:?}. \
         При session_replication_role='replica' они не сработают - \
         ALTER TABLE ... ENABLE ALWAYS TRIGGER ..."
    );
}
