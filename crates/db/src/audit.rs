//! Проверка цепочки аудита (INV-A01).
//!
//! `audit.log` - append-only с hash-цепочкой: `row_hash = sha256(prev_hash
//! || payload)`. Цепочка и есть доказательство того, что историю не
//! переписывали, но само по себе ее наличие ничего не значит: разрыв
//! обнаруживается только проверкой. Функция `audit.verify_chain()`
//! существовала с первой миграции и вызывалась ровно из одного теста -
//! в работающей системе целостность не проверял никто, и разрыв всплыл бы
//! в момент разбирательства, то есть позже всего.

use crate::Db;

/// Итог сверки цепочки.
#[derive(Debug, Clone, Copy)]
pub struct ChainStatus {
    /// Цепочка непрерывна: каждый `row_hash` сходится с пересчитанным.
    pub intact: bool,
    /// Сколько записей в журнале на момент проверки.
    pub entries: i64,
}

/// Сверить цепочку целиком (INV-A01).
///
/// Полный проход по журналу - операция читающая и потому безопасная в любой
/// момент; вызывать ее часто незачем, расписание задает вызывающий.
pub async fn verify_chain(db: &Db) -> Result<ChainStatus, sqlx::Error> {
    // `!` у обоих столбцов: планировщик считает результат функции и count()
    // потенциально NULL-выражением, хотя ни то, ни другое NULL не бывает
    let intact = sqlx::query_scalar!(r#"SELECT audit.verify_chain() AS "intact!""#)
        .fetch_one(db)
        .await?;
    let entries = sqlx::query_scalar!(r#"SELECT count(*) AS "entries!" FROM audit.log"#)
        .fetch_one(db)
        .await?;
    Ok(ChainStatus { intact, entries })
}
