//! Внешние идентичности (FR-1502, ADR-0003): субъект провайдера ↔ `core.users`.
//!
//! Пользователь остается один и тот же независимо от способа входа: локальный
//! пароль (FR-1501) и внешний провайдер приводят к одному `user_id`, поэтому
//! все построенное на `CurrentUser` работает без изменений.

use tou_domain::role::Role;
use uuid::Uuid;

use crate::Db;
use crate::users::UserRecord;

/// Сведения о субъекте из проверенного `id_token`.
#[derive(Debug, Clone)]
pub struct ExternalIdentity {
    pub issuer: String,
    pub subject: String,
    pub email: String,
    pub full_name: String,
    pub locale: String,
    pub provider_login: Option<String>,
    /// Роли из claim провайдера (уже отфильтрованы по enum `Role`)
    pub roles: Vec<Role>,
}

/// Результат входа: пользователь и то, чем этот вход для него стал.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkOutcome {
    /// Учетная запись заведена по первому входу через провайдера
    Created,
    /// Существующая локальная запись (совпал email) связана с провайдером
    Linked,
    /// Связь уже была - обычный повторный вход
    Reused,
}

/// Выборка `UserRecord` под алиасом `u` (JOIN) и без него (RETURNING).
///
/// `!` у `email`: `::text` от citext планировщик считает потенциально NULL,
/// хотя столбец NOT NULL.
macro_rules! user_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            UserRecord,
            r#"SELECT u.id, u.email::text AS "email!", u.password_hash,
                      u.full_name, u.locale, u.is_active"# + $tail
            $(, $arg)*
        )
    };
}
/// То же для `RETURNING`: там столбцы идут без алиаса и в конце запроса,
/// поэтому макрос принимает не хвост, а голову.
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

/// Вход через внешнего провайдера одной транзакцией: связь → пользователь →
/// синхронизация ролей из claim. Актором аудита выступает сам вошедший.
///
/// Порядок поиска: сначала `(issuer, subject)` - `sub` стабилен при смене email
/// в AD; затем email - так сотрудник с локальной записью контура 1 не получает
/// дубль, а связывается с ней.
pub async fn login_external(
    db: &Db,
    identity: &ExternalIdentity,
) -> Result<(UserRecord, LinkOutcome), sqlx::Error> {
    let mut tx = db.begin().await?;

    let existing = user_query!(
        " FROM core.user_identities i
         JOIN core.users u ON u.id = i.user_id
         WHERE i.issuer = $1 AND i.subject = $2",
        identity.issuer,
        identity.subject
    )
    .fetch_optional(&mut *tx)
    .await?;

    let (user, outcome) = match existing {
        Some(user) => (user, LinkOutcome::Reused),
        None => {
            let by_email = user_query!(
                " FROM core.users u WHERE u.email = $1::citext",
                identity.email
            )
            .fetch_optional(&mut *tx)
            .await?;

            match by_email {
                Some(user) => (user, LinkOutcome::Linked),
                None => {
                    // Пароля у такой записи нет: вход только через провайдера.
                    // Email подтвержден провайдером - повторное подтверждение не нужно.
                    let created = user_query_returning!(
                        "INSERT INTO core.users (email, full_name, locale, email_confirmed_at)
                         VALUES ($1::citext, $2, $3, core.now())",
                        identity.email,
                        identity.full_name,
                        identity.locale
                    )
                    .fetch_one(&mut *tx)
                    .await?;
                    (created, LinkOutcome::Created)
                }
            }
        }
    };

    if !user.is_active {
        // Отключенная запись не входит ни одним способом (совпадает с FR-1501)
        tx.rollback().await?;
        return Err(sqlx::Error::RowNotFound);
    }

    // Актор аудита для триггеров user_identities и role_grants
    sqlx::query!(
        "SELECT set_config('app.user_id', $1, true)",
        user.id.to_string()
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query!(
        "INSERT INTO core.user_identities (user_id, issuer, subject, provider_login, last_login_at)
         VALUES ($1, $2, $3, $4, core.now())
         ON CONFLICT (issuer, subject)
         DO UPDATE SET last_login_at = core.now(), provider_login = EXCLUDED.provider_login",
        user.id,
        identity.issuer,
        identity.subject,
        identity.provider_login.as_deref()
    )
    .execute(&mut *tx)
    .await?;

    sync_roles(&mut tx, user.id, &identity.roles).await?;

    tx.commit().await?;
    Ok((user, outcome))
}

/// Приведение ролей провайдера к состоянию claim'а (FR-1502): роли источника
/// `oidc` появляются и снимаются вслед за AD, роли, выданные админом вручную
/// (`local`, FR-1503), не трогаются - иначе внешний провайдер молча отбирал бы
/// права, назначенные внутри системы. Обе операции пишут аудит триггером.
async fn sync_roles(
    tx: &mut sqlx::PgConnection,
    user_id: Uuid,
    roles: &[Role],
) -> Result<(), sqlx::Error> {
    let names: Vec<String> = roles.iter().map(|r| r.as_str().to_owned()).collect();

    sqlx::query!(
        "DELETE FROM core.role_grants
         WHERE user_id = $1 AND source = 'oidc'
           AND role <> ALL($2::text[]::core.role[])",
        user_id,
        &names
    )
    .execute(&mut *tx)
    .await?;

    if names.is_empty() {
        return Ok(());
    }

    // Роль, уже выданная админом вручную, остается локальной: DO NOTHING
    sqlx::query!(
        "INSERT INTO core.role_grants (user_id, role, granted_by, source)
         SELECT $1, role, $1, 'oidc' FROM unnest($2::text[]::core.role[]) AS role
         ON CONFLICT (user_id, role) DO NOTHING",
        user_id,
        &names
    )
    .execute(&mut *tx)
    .await?;

    Ok(())
}

/// Связанные с пользователем провайдеры (кабинет и админка: видно, чем входит).
pub async fn identities_of(db: &Db, user_id: Uuid) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT issuer FROM core.user_identities WHERE user_id = $1 ORDER BY linked_at",
        user_id
    )
    .fetch_all(db)
    .await
}
