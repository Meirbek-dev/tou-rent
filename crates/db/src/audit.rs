//! Проверка цепочки аудита (INV-A01).
//!
//! `audit.log` - append-only с hash-цепочкой: `row_hash = sha256(prev_hash
//! || payload)`. Цепочка и есть доказательство того, что историю не
//! переписывали, но само по себе ее наличие ничего не значит: разрыв
//! обнаруживается только проверкой. Функция `audit.verify_chain()`
//! существовала с первой миграции и вызывалась ровно из одного теста -
//! в работающей системе целостность не проверял никто, и разрыв всплыл бы
//! в момент разбирательства, то есть позже всего.
//!
//! Проверять мало - результат проверки должен где-то оставаться. Экспорта
//! логов и метрик у бэкенда нет (арх. v3 § 8, Q-018), журнал контейнера
//! ротируется, поэтому единственная запись о разрыве уходила из ротации
//! раньше, чем ее кто-нибудь читал. Итог каждой сверки пишется в
//! `audit.chain_checks` - той же функцией, которая его считает.

use time::OffsetDateTime;

use crate::Db;

/// Итог сверки цепочки.
#[derive(Debug, Clone, Copy)]
pub struct ChainStatus {
    /// Цепочка непрерывна: каждый `row_hash` сходится с пересчитанным.
    pub intact: bool,
    /// Сколько записей в журнале на момент проверки.
    pub entries: i64,
    /// `audit.log.id` первой разошедшейся записи; `None` - цепочка цела.
    pub broken_at: Option<i64>,
}

/// Сверить цепочку целиком, ничего не записывая (INV-A01).
///
/// Полный проход по журналу - операция читающая и потому безопасная в любой
/// момент; расписание регулярной сверки задает вызывающий, и она идет через
/// [`run_chain_check`], который оставляет след.
pub async fn verify_chain(db: &Db) -> Result<ChainStatus, sqlx::Error> {
    // `!` - столбцы приходят из функции, а не из таблицы: происхождение
    // планировщик не сообщает и считает их потенциально NULL
    let row = sqlx::query!(
        r#"SELECT intact AS "intact!", entries AS "entries!", broken_at
           FROM audit.verify_chain_report()"#
    )
    .fetch_one(db)
    .await?;

    Ok(ChainStatus {
        intact: row.intact,
        entries: row.entries,
        broken_at: row.broken_at,
    })
}

/// Запись журнала сверок `audit.chain_checks`.
#[derive(Debug, Clone, Copy)]
pub struct ChainCheck {
    pub checked_at: OffsetDateTime,
    pub intact: bool,
    pub entries: i64,
    pub broken_at: Option<i64>,
}

/// Сверить цепочку и записать результат (INV-A01).
///
/// Пересчет и запись выполняет одна SECURITY DEFINER-функция БД: права
/// INSERT на `audit.chain_checks` у роли приложения нет, поэтому отметка
/// «цепочка цела» не может появиться иначе, чем от настоящего пересчета.
pub async fn run_chain_check(db: &Db) -> Result<ChainCheck, sqlx::Error> {
    sqlx::query_as!(
        ChainCheck,
        r#"SELECT checked_at AS "checked_at!", intact AS "intact!",
                  entries AS "entries!", broken_at
           FROM audit.run_chain_check()"#
    )
    .fetch_one(db)
    .await
}

/// Состояние цепочки для кабинета админа (FR-1601).
pub struct ChainState {
    /// Последняя сверка - какой бы она ни была.
    pub last: Option<ChainCheck>,
    /// Момент последней сверки, на которой цепочка сходилась. Отдельно от
    /// `last`: при разрыве важно не только то, что он есть, но и то, когда
    /// система в последний раз была заведомо целой.
    pub last_intact_at: Option<OffsetDateTime>,
}

/// Последняя сверка и последняя успешная (FR-1601).
///
/// Порядок - по `id`, а не по `checked_at`: время стенда управляемо
/// (ADR-0005) и может уйти назад, порядок записи - нет.
pub async fn chain_state(db: &Db) -> Result<ChainState, sqlx::Error> {
    let last = sqlx::query_as!(
        ChainCheck,
        "SELECT checked_at, intact, entries, broken_at
         FROM audit.chain_checks ORDER BY id DESC LIMIT 1"
    )
    .fetch_optional(db)
    .await?;

    let last_intact_at = sqlx::query_scalar!(
        "SELECT checked_at FROM audit.chain_checks
         WHERE intact ORDER BY id DESC LIMIT 1"
    )
    .fetch_optional(db)
    .await?;

    Ok(ChainState {
        last,
        last_intact_at,
    })
}
