//! Ключ шифрования ценовых предложений (М4, INV-040, п. 40).
//!
//! Мастер-ключ живет в окружении приложения (NFR-09) и приходит в базу
//! соединением - в самой базе его нет. Здесь остаются две служебные
//! операции: проверка, что ключ на месте, и дошифровка записей, сделанных
//! до перехода на шифрование.

use crate::Db;

/// Переменная окружения с мастер-ключом (арх. § 6).
pub const KEY_ENV: &str = "PRICE_ENCRYPTION_KEY";

/// Задан ли ключ шифрования у этого соединения (INV-040): без него цены
/// не читаются и не записываются.
pub async fn key_configured(db: &Db) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT core.price_key() IS NOT NULL AS "configured!""#)
        .fetch_one(db)
        .await
}

/// Дошифровка цен, записанных до перехода (п. 40). Открытое значение
/// переписывается триггером и стирается; операция идемпотентна - второй
/// проход не находит открытых цен.
///
/// Выполняется под владельцем схемы (шаг `api migrate`): у роли приложения
/// прав на изменение ценового предложения нет - оно неизменяемо (INV-040).
pub async fn encrypt_pending(db: &Db) -> Result<u64, sqlx::Error> {
    let Ok(key) = std::env::var(KEY_ENV) else {
        return Ok(0);
    };
    if key.trim().is_empty() {
        return Ok(0);
    }

    let mut tx = db.begin().await?;
    sqlx::query!("SELECT set_config('app.price_key', $1, true)", key)
        .fetch_one(&mut *tx)
        .await?;
    let updated =
        sqlx::query!("UPDATE core.price_proposals SET amount = amount WHERE amount IS NOT NULL")
            .execute(&mut *tx)
            .await?;
    tx.commit().await?;

    Ok(updated.rows_affected())
}
