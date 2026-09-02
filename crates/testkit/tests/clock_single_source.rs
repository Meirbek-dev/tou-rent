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
//! Третья проверка смотрит не в БД, а в исходники. Умолчания колонок и тела
//! функций `core` были закрыты, а SQL, который пишет сам сервис, - нет: пять
//! запросов слоя данных считали деловую дату через `current_date`. При
//! сдвинутых часах стенда `refdata::coefficients_today` давал коэффициент
//! прошлой версии, `commission::active` - комиссию с истекшими полномочиями;
//! и без всякого сдвига `current_date` в сессии `Etc/UTC` ежедневно с 00:00
//! до 05:00 по Алматы отставал на сутки. Сторожа ровно там, где жил дефект,
//! не было.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021); проверка исходников идет
//! всегда, БД ей не нужна.

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

// --- Время в SQL, который пишет сам сервис ----------------------------------

/// Что считается временем мимо доменных часов. `now()` ловится только без
/// префикса: `core.now()` - и есть источник, а `Instant::now()`,
/// `clock.now()` и `Uuid::now_v7()` - не SQL вовсе.
const SUSPECT: [&str; 5] = [
    "current_date",
    "current_timestamp",
    "localtimestamp",
    "clock_timestamp",
    "now()",
];

/// Намеренное исключение помечается в той же строке (регламент А.4).
const EXEMPTION: &str = "ALLOWED-BY-ENGINEER:";

/// Крейты, чьи исходники проверяются: SQL живет в `crates/*/src`.
fn crate_sources() -> Vec<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|crates| crates.parent())
        .map(|repo| repo.join("crates"))
        .unwrap_or_default();

    let mut files = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Только `src`: тесты вправе звать реальные часы, чтобы
                // отличить их от доменных
                if path.file_name().is_some_and(|name| name == "target") {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs")
                && path.components().any(|c| c.as_os_str() == "src")
            {
                files.push(path);
            }
        }
    }
    files
}

/// Код строки без комментария: в комментариях `now()` упоминается по делу.
fn code_of(line: &str) -> &str {
    match line.find("//") {
        Some(at) => &line[..at],
        None => line,
    }
}

/// Позиции токена как отдельного слова, а не куска идентификатора.
fn hits(code: &str, token: &str) -> bool {
    let lower = code.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find(token) {
        let start = from + at;
        let end = start + token.len();
        let before = lower[..start].chars().next_back();
        let after = lower[end..].chars().next();
        // `core.now()` и `Instant::now()` - не SQL; `add_business_days` не
        // должен ловиться на куске имени
        let boundary_before =
            before.is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '.' || c == ':'));
        let boundary_after = after.is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        if boundary_before && boundary_after {
            return true;
        }
        from = end;
    }
    false
}

/// SQL сервиса считает деловую дату теми же часами, что и схема.
///
/// Проверка по исходникам, а не по БД: запрос собирается в Rust, и в
/// каталоге PostgreSQL его нет. Исключение помечается токеном
/// `ALLOWED-BY-ENGINEER:` в той же строке - там, где реальные часы нужны
/// по существу (отметка о самом сдвиге, `refdata::set_clock_shift`).
#[test]
fn service_sql_uses_the_domain_clock() {
    let sources = crate_sources();
    assert!(
        sources.len() > 10,
        "исходники крейтов не найдены - проверка ничего не проверяет"
    );

    let mut unexpected = Vec::new();
    for path in sources {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (number, line) in text.lines().enumerate() {
            if line.contains(EXEMPTION) {
                continue;
            }
            let code = code_of(line);
            for token in SUSPECT {
                if hits(code, token) {
                    unexpected.push(format!(
                        "{}:{}: {}",
                        path.display(),
                        number + 1,
                        line.trim()
                    ));
                    break;
                }
            }
        }
    }

    assert!(
        unexpected.is_empty(),
        "SQL сервиса считает время мимо core.now() (NFR-03, ADR-0005): {unexpected:#?}"
    );
}

/// Сам разбор строки: без него проверка выше молча ловила бы не то.
#[test]
fn the_scan_tells_sql_clocks_from_rust_calls() {
    assert!(hits("WHERE effective_from <= current_date", "current_date"));
    assert!(hits("SELECT now()", "now()"));
    assert!(hits("set_at = clock_timestamp()", "clock_timestamp"));

    assert!(!hits("SELECT core.now()", "now()"));
    assert!(!hits("Instant::now()", "now()"));
    assert!(!hits("clock.now()", "now()"));
    assert!(!hits("Uuid::now_v7()", "now()"));
    assert!(!hits(
        "refdata.add_business_days($2::date, 2)",
        "current_date"
    ));

    assert_eq!(code_of("let x = 1; // now() в комментарии"), "let x = 1; ");
    assert_eq!(code_of("//! current_date в доке"), "");
}
