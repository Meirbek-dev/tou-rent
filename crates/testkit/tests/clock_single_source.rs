//! Единый источник времени (NFR-03, ADR-0005, арх. v3 § 6.2) против живой БД.
//!
//! `core.now()` объявлен единственным источником времени для правил сроков
//! и юридически значимых отметок. Правила перевела миграция T68, отметки -
//! `20260809140000_clock_defaults.sql`: до нее 57 колонок ставили время
//! процесса СУБД, и при сдвинутых часах хронология одного тендера
//! расходилась - журнал в одном времени, `submitted_at` заявки в другом.
//!
//! Тест закрепляет правило на том уровне, где его можно нарушить молча:
//! новая таблица с `DEFAULT now()` проходит и линт, и типы, и все прочие
//! тесты. Здесь она не пройдет.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

/// Отметка о самом сдвиге часов обязана быть реальной: иначе сдвиг прячет
/// себя, и по таблице не понять, когда его поставили (ADR-0005).
const REAL_CLOCK_ALLOWED: [(&str, &str, &str); 1] = [("refdata", "clock_offset", "set_at")];

async fn pool() -> Result<Option<sqlx::PgPool>, sqlx::Error> {
    let required =
        tou_testkit::database_url().map_err(|e| sqlx::Error::Configuration(Box::new(e)))?;
    let Some(url) = required else {
        return Ok(None);
    };
    Ok(Some(tou_db::connect(&url).await?))
}

/// Умолчания колонок берут время из `core.now()`, а не из часов процесса.
#[tokio::test]
async fn column_defaults_use_the_domain_clock() -> Result<(), sqlx::Error> {
    let Some(pool) = pool().await? else {
        return Ok(());
    };

    // `::text` у каждого столбца: information_schema отдает их доменами
    // (`sql_identifier`, `character_data`), для которых у sqlx нет отображения;
    // отсюда же `!` - приведение планировщик считает потенциально NULL
    let rows = sqlx::query!(
        r#"SELECT table_schema::text AS "schema!", table_name::text AS "table!",
                  column_name::text AS "column!", column_default::text AS "value!"
           FROM information_schema.columns
          WHERE table_schema IN ('core', 'audit', 'refdata')
            AND column_default IS NOT NULL
            AND column_default LIKE '%now()%'
            AND column_default NOT LIKE '%core.now()%'
          ORDER BY 1, 2, 3"#
    )
    .fetch_all(&pool)
    .await?;

    let unexpected: Vec<String> = rows
        .into_iter()
        .filter(|row| {
            !REAL_CLOCK_ALLOWED.contains(&(
                row.schema.as_str(),
                row.table.as_str(),
                row.column.as_str(),
            ))
        })
        .map(|row| {
            format!(
                "{}.{}.{} = {}",
                row.schema, row.table, row.column, row.value
            )
        })
        .collect();

    assert!(
        unexpected.is_empty(),
        "отметки времени мимо core.now() (NFR-03): {unexpected:?}"
    );
    Ok(())
}

/// Функции схемы `core` считают сроки тем же временем. Исключение одно -
/// сама `core.now()`, которая на `now()` и построена.
#[tokio::test]
async fn core_functions_use_the_domain_clock() -> Result<(), sqlx::Error> {
    let Some(pool) = pool().await? else {
        return Ok(());
    };

    let names = sqlx::query_scalar!(
        r#"SELECT p.proname::text AS "name!"
           FROM pg_proc p
           JOIN pg_namespace n ON n.oid = p.pronamespace
          WHERE n.nspname = 'core'
            AND p.proname <> 'now'
            AND p.prosrc ~ '(^|[^.[:alnum:]_])now\(\)'
          ORDER BY 1"#
    )
    .fetch_all(&pool)
    .await?;

    assert!(
        names.is_empty(),
        "функции core считают время мимо core.now() (NFR-03): {names:?}"
    );
    Ok(())
}
