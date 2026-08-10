//! Пользователи и роли (М15): запросы к `core.users` / `core.role_grants`.

use tou_domain::redacted::Hidden;
use tou_domain::role::Role;
use uuid::Uuid;

use crate::Db;

/// Строка `core.users`. NFR-07: `Debug` написан руками - email и имя это
/// персональные данные, а `password_hash` - секрет; производный `Debug`
/// вывел бы все три в первый же `tracing::debug!(?record)`. Поля остаются
/// обычными строками: они нужны почти каждому вызывающему, а закрывает их
/// именно отсутствие пути в лог.
#[derive(Clone)]
pub struct UserRecord {
    pub id: Uuid,
    pub email: String,
    pub password_hash: Option<String>,
    pub full_name: String,
    pub locale: String,
    pub is_active: bool,
}

impl std::fmt::Debug for UserRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Виден только идентификатор и то, что не является ПДн: по id запись
        // находится в БД и в аудите, где фиксация полная (NFR-07)
        f.debug_struct("UserRecord")
            .field("id", &self.id)
            .field("email", &Hidden)
            .field("password_hash", &Hidden)
            .field("full_name", &Hidden)
            .field("locale", &self.locale)
            .field("is_active", &self.is_active)
            .finish()
    }
}

/// Выборка пользователя: общий список столбцов + хвост запроса (см. `acts.rs`).
///
/// `!` у `email`: `::text` от citext планировщик считает потенциально NULL,
/// хотя столбец NOT NULL.
macro_rules! user_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            UserRecord,
            r#"SELECT id, email::text AS "email!", password_hash,
                      full_name, locale, is_active
               FROM core.users"# + $tail
            $(, $arg)*
        )
    };
}
/// То же для `RETURNING`: столбцы идут в конце запроса (см. `identities.rs`).
macro_rules! user_query_returning {
    ($head:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            UserRecord,
            $head + r#" RETURNING id, email::text AS "email!", password_hash,
                                  full_name, locale, is_active"#
            $(, $arg)*
        )
    };
}

pub async fn find_by_email(db: &Db, email: &str) -> Result<Option<UserRecord>, sqlx::Error> {
    user_query!(" WHERE email = $1::citext", email)
        .fetch_optional(db)
        .await
}

pub async fn find_by_id(db: &Db, id: Uuid) -> Result<Option<UserRecord>, sqlx::Error> {
    user_query!(" WHERE id = $1", id).fetch_optional(db).await
}

/// Роли пользователя из `core.role_grants`. Неизвестное значение роли в БД -
/// ошибка декодирования (рассинхрон enum'ов должен падать громко).
pub async fn roles_of(db: &Db, user_id: Uuid) -> Result<Vec<Role>, sqlx::Error> {
    let rows = sqlx::query_scalar!(
        // ORDER BY по номеру столбца: под псевдонимом `role!` сортировка по
        // имени попала бы на перечисление, а не на его текст
        r#"SELECT role::text AS "role!" FROM core.role_grants
           WHERE user_id = $1 ORDER BY 1"#,
        user_id
    )
    .fetch_all(db)
    .await?;

    rows.into_iter()
        .map(|s| {
            s.parse::<Role>()
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))
        })
        .collect()
}

/// Роли страницы пользователей одним запросом (админка без N+1).
pub async fn roles_for(
    db: &Db,
    user_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, Vec<Role>>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"SELECT user_id, role::text AS "role!" FROM core.role_grants
           WHERE user_id = ANY($1) ORDER BY user_id, 2"#,
        user_ids
    )
    .fetch_all(db)
    .await?;

    let mut by_user = std::collections::HashMap::<Uuid, Vec<Role>>::new();
    for row in rows {
        let (user_id, raw) = (row.user_id, row.role);
        let role = raw
            .parse::<Role>()
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        by_user.entry(user_id).or_default().push(role);
    }
    Ok(by_user)
}

#[derive(Debug, thiserror::Error)]
pub enum InsertUserError {
    #[error("email уже занят")]
    EmailTaken,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Регистрация участника (FR-1501): пользователь + роль `participant`
/// одной транзакцией; актор аудита - сам пользователь. Контур 1 -
/// авто-подтверждение email (`email_confirmed_at = core.now()`).
pub async fn insert_participant(
    db: &Db,
    email: &str,
    password_hash: &str,
    full_name: &str,
    locale: &str,
) -> Result<UserRecord, InsertUserError> {
    let mut tx = db.begin().await?;

    let inserted = user_query_returning!(
        "INSERT INTO core.users (email, password_hash, full_name, locale, email_confirmed_at)
         VALUES ($1::citext, $2, $3, $4, core.now())
         ON CONFLICT (email) DO NOTHING",
        email,
        password_hash,
        full_name,
        locale
    )
    .fetch_optional(&mut *tx)
    .await?;

    let Some(user) = inserted else {
        return Err(InsertUserError::EmailTaken);
    };

    // Актор аудита для триггера role_grants (FR-1503, INV-AUDIT).
    // `set_config` возвращает столбец, поэтому не `execute`, а `fetch_one`
    sqlx::query!(
        "SELECT set_config('app.user_id', $1, true)",
        user.id.to_string()
    )
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query!(
        "INSERT INTO core.role_grants (user_id, role, granted_by)
         VALUES ($1, 'participant', $1)",
        user.id
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(user)
}

/// Назначение роли админом (FR-1503); изменение фиксирует audit-триггер.
pub async fn grant_role(
    db: &Db,
    actor: Uuid,
    user_id: Uuid,
    role: Role,
) -> Result<(), sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        // `$2::text::core.role`: роль приходит строкой доменного типа,
        // приведение к перечислению делает БД
        sqlx::query!(
            "INSERT INTO core.role_grants (user_id, role, granted_by)
             VALUES ($1, $2::text::core.role, $3)
             ON CONFLICT (user_id, role) DO NOTHING",
            user_id,
            role.as_str(),
            actor
        )
        .execute(&mut *tx)
        .await?;
        Ok(())
    })
    .await
}

pub async fn revoke_role(
    db: &Db,
    actor: Uuid,
    user_id: Uuid,
    role: Role,
) -> Result<(), sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query!(
            "DELETE FROM core.role_grants WHERE user_id = $1 AND role = $2::text::core.role",
            user_id,
            role.as_str()
        )
        .execute(&mut *tx)
        .await?;
        Ok(())
    })
    .await
}

/// Список пользователей для админки (cursor-пагинация по uuid v7, ТЗ § 7).
pub async fn list_users(
    db: &Db,
    after: Option<Uuid>,
    limit: i64,
) -> Result<Vec<UserRecord>, sqlx::Error> {
    user_query!(
        " WHERE ($1::uuid IS NULL OR id > $1)
          ORDER BY id
          LIMIT $2",
        after,
        limit
    )
    .fetch_all(db)
    .await
}
