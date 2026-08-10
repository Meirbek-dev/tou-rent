//! Справочники (FR-202): выборка версий «на дату». Дату определяет БД -
//! время сервера единственно юридически значимое (NFR-03), Clock в Rust
//! здесь не нужен.

use std::collections::HashMap;

use rust_decimal::Decimal;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::Db;

/// Время сервера - тот же источник, что у триггеров и отметок (`core.now()`,
/// ADR-0005).
///
/// Нужен там, где значение времени попадает в Rust, а не остается в запросе:
/// печатные формы ставят отметку «сформировано». Системные часы процесса для
/// этого не годятся - при сдвинутых часах стенда (T68) документ датировался
/// бы не тем днем, в который его по правилам сформировали.
pub async fn now(db: &Db) -> Result<OffsetDateTime, sqlx::Error> {
    // `as "now!"`: core.now() для планировщика может быть NULL-выражением,
    // а доменных часов без значения не бывает
    sqlx::query_scalar!(r#"SELECT core.now() AS "now!""#)
        .fetch_one(db)
        .await
}

/// МРП текущего года (по часам сервера БД); `None` - год не заведен (A-010).
pub async fn current_mrp(db: &Db) -> Result<Option<(i32, Decimal)>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT year, amount FROM refdata.mrp
         WHERE year = extract(year FROM core.now())::int",
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| (r.year, r.amount)))
}

pub struct RejectionReason {
    pub code: String,
    pub label_ru: String,
    pub label_kk: Option<String>,
    pub label_en: Option<String>,
    pub rule_ref: String,
}

/// Закрытый перечень оснований отклонения (FR-502, п. 52; INV-052).
pub async fn rejection_reasons(db: &Db) -> Result<Vec<RejectionReason>, sqlx::Error> {
    let rows = sqlx::query_as!(
        RejectionReason,
        "SELECT code, label_ru, label_kk, label_en, rule_ref
         FROM refdata.rejection_reasons ORDER BY code",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Значения всех коэффициентов, действующие сегодня: последняя версия
/// каждой пары (коэффициент, опция) с effective_from <= current_date.
pub async fn coefficients_today(
    db: &Db,
) -> Result<HashMap<(String, String), Decimal>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT DISTINCT ON (coefficient, option_code) coefficient, option_code, value
         FROM refdata.rate_coefficients
         WHERE effective_from <= current_date
         ORDER BY coefficient, option_code, effective_from DESC",
    )
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| ((row.coefficient, row.option_code), row.value))
        .collect())
}

// --- Ведение справочников админом (FR-1901, FR-202) --------------------------
//
// Правка справочника не меняет прошлые расчеты: в лоте тендера лежит снимок
// `RateCalculation` (FR-202), а версии коэффициентов различаются по
// `effective_from` и не перезаписываются - новая версия добавляется рядом.

pub struct MrpRecord {
    pub year: i32,
    pub amount: Decimal,
}

/// Все заведенные годы МРП (FR-201, Прил. 4).
pub async fn mrp_all(db: &Db) -> Result<Vec<MrpRecord>, sqlx::Error> {
    let rows = sqlx::query_as!(
        MrpRecord,
        "SELECT year, amount FROM refdata.mrp ORDER BY year DESC"
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// МРП года: у показателя одна величина на год, поэтому это правка значения,
/// а не новая версия. Ограничения года и положительности держит БД.
pub async fn upsert_mrp(
    db: &Db,
    actor: Uuid,
    year: i32,
    amount: Decimal,
) -> Result<MrpRecord, sqlx::Error> {
    crate::with_actor(db, actor, async |tx| upsert_mrp_on(tx, year, amount).await).await
}

/// Та же правка в транзакции вызывающего. Роль приложения не имеет права
/// удалять строки справочника (правка - новая версия, а не переписанная
/// история), поэтому тест выполняет сценарий в откатываемой транзакции.
pub async fn upsert_mrp_on(
    conn: &mut sqlx::PgConnection,
    year: i32,
    amount: Decimal,
) -> Result<MrpRecord, sqlx::Error> {
    sqlx::query_as!(
        MrpRecord,
        "INSERT INTO refdata.mrp (year, amount) VALUES ($1, $2)
         ON CONFLICT (year) DO UPDATE SET amount = EXCLUDED.amount
         RETURNING year, amount",
        year,
        amount
    )
    .fetch_one(conn)
    .await
}

pub struct CoefficientRecord {
    pub id: Uuid,
    pub coefficient: String,
    pub option_code: String,
    pub label_ru: String,
    pub label_kk: Option<String>,
    pub label_en: Option<String>,
    pub value: Decimal,
    pub effective_from: time::Date,
    /// Версия действует сегодня (последняя из вступивших в силу)
    pub current: bool,
}

/// Все версии коэффициентов Прил. 4 - история, а не только действующее:
/// админ должен видеть, что именно применялось к прошлым расчетам.
pub async fn coefficients_all(db: &Db) -> Result<Vec<CoefficientRecord>, sqlx::Error> {
    let rows = sqlx::query_as!(
        CoefficientRecord,
        r#"SELECT id, coefficient, option_code,
                  label_ru, label_kk, label_en, value, effective_from,
                  id = first_value(id) OVER (
                    PARTITION BY coefficient, option_code
                    ORDER BY (effective_from <= current_date) DESC, effective_from DESC
                  ) AND effective_from <= current_date AS "current!"
           FROM refdata.rate_coefficients
           ORDER BY coefficient, option_code, effective_from DESC"#
    )
    .fetch_all(db)
    .await?;

    Ok(rows)
}

#[derive(Clone, Copy)]
pub struct NewCoefficientVersion<'a> {
    pub coefficient: &'a str,
    pub option_code: &'a str,
    pub label_ru: &'a str,
    pub label_kk: Option<&'a str>,
    pub label_en: Option<&'a str>,
    pub value: Decimal,
    pub effective_from: time::Date,
}

/// Новая версия коэффициента (FR-202). Совпадение по дате вступления - правка
/// той же версии: две разные величины с одной датой не имеют смысла, а UNIQUE
/// в БД их и не допустит.
pub async fn upsert_coefficient_version(
    db: &Db,
    actor: Uuid,
    new: NewCoefficientVersion<'_>,
) -> Result<Uuid, sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        upsert_coefficient_version_on(tx, new).await
    })
    .await
}

/// Та же версия в транзакции вызывающего (см. [`upsert_mrp_on`]).
pub async fn upsert_coefficient_version_on(
    conn: &mut sqlx::PgConnection,
    new: NewCoefficientVersion<'_>,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO refdata.rate_coefficients
               (coefficient, option_code, label_ru, label_kk, label_en, value, effective_from)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (coefficient, option_code, effective_from) DO UPDATE
               SET label_ru = EXCLUDED.label_ru,
                   label_kk = EXCLUDED.label_kk,
                   label_en = EXCLUDED.label_en,
                   value    = EXCLUDED.value
             RETURNING id",
        new.coefficient,
        new.option_code,
        new.label_ru,
        new.label_kk,
        new.label_en,
        new.value,
        new.effective_from
    )
    .fetch_one(conn)
    .await
}

/// Пара «множитель × опция» существует в закрытом каталоге Прил. 4
/// (`refdata.rate_options`). Перечень множителей и их опций задан Правилами,
/// поэтому админ версионирует значения, а не изобретает опции: тот же рубеж
/// стоит внешним ключом на `rate_coefficients`.
pub async fn rate_option_exists(
    db: &Db,
    coefficient: &str,
    option_code: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (
           SELECT 1 FROM refdata.rate_options
           WHERE coefficient = $1 AND option_code = $2
         ) AS "exists!""#,
        coefficient,
        option_code
    )
    .fetch_one(db)
    .await
}

// --- Управляемое время стенда (T68, ADR-0005) --------------------------------
//
// Сдвиг часов - возможность подделать юридически значимую отметку, поэтому
// роль приложения права на него не имеет (REVOKE в миграции). Эти функции
// вызываются подкомандой `api time-shift` под владельцем БД и только при
// заданной переменной ALLOW_TIME_SHIFT.

/// Текущий сдвиг часов стенда в секундах; 0 - часы обычные.
pub async fn clock_shift_seconds(db: &Db) -> Result<f64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT extract(epoch FROM shift)::float8 AS "shift!"
           FROM refdata.clock_offset WHERE id"#
    )
    .fetch_one(db)
    .await
}

/// Сдвиг часов стенда. `who` попадает в аудит вместе со значением.
pub async fn set_clock_shift(db: &Db, interval: &str, who: &str) -> Result<f64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"UPDATE refdata.clock_offset
         SET shift = $1::text::interval, set_at = clock_timestamp(), set_by = $2
         WHERE id
         RETURNING extract(epoch FROM shift)::float8 AS "shift!""#,
        interval,
        who
    )
    .fetch_one(db)
    .await
}
