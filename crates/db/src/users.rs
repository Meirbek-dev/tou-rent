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
    pub applicant_kind: Option<String>,
    pub id_number: Option<String>,
    pub phone: Option<String>,
    pub email_confirmed_at: Option<time::OffsetDateTime>,
    pub phone_confirmed_at: Option<time::OffsetDateTime>,
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
            .field("id_number", &Hidden)
            .field("phone", &Hidden)
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
                      full_name, locale,
                      applicant_kind::text AS applicant_kind, id_number, phone,
                      email_confirmed_at, phone_confirmed_at, is_active
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
                                  full_name, locale,
                                  applicant_kind::text AS applicant_kind,
                                  id_number, phone, email_confirmed_at,
                                  phone_confirmed_at, is_active"#
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
    #[error("ИИН/БИН уже зарегистрирован")]
    IdNumberTaken,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Регистрация участника (FR-1501): пользователь + роль `participant`
/// одной транзакцией; актор аудита - сам пользователь. До подтверждения
/// выбранного канала вход закрыт.
pub struct NewParticipant<'a> {
    pub email: &'a str,
    pub password_hash: &'a str,
    pub full_name: &'a str,
    pub locale: &'a str,
    pub applicant_kind: &'a str,
    pub id_number: &'a str,
    pub phone: &'a str,
    pub verification_channel: &'a str,
    pub code_hash: &'a str,
}

pub async fn insert_participant(
    db: &Db,
    new: NewParticipant<'_>,
) -> Result<UserRecord, InsertUserError> {
    let mut tx = db.begin().await?;

    let inserted = user_query_returning!(
        "INSERT INTO core.users
           (email, password_hash, full_name, locale, applicant_kind, id_number, phone)
         VALUES ($1::citext, $2, $3, $4, $5::text::core.applicant_kind, $6, $7)
         ON CONFLICT DO NOTHING",
        new.email,
        new.password_hash,
        new.full_name,
        new.locale,
        new.applicant_kind,
        new.id_number,
        new.phone
    )
    .fetch_optional(&mut *tx)
    .await?;

    let Some(user) = inserted else {
        let email_exists = sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM core.users WHERE email = $1::citext) AS "exists!""#,
            new.email
        )
        .fetch_one(&mut *tx)
        .await?;
        return Err(if email_exists {
            InsertUserError::EmailTaken
        } else {
            InsertUserError::IdNumberTaken
        });
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

    sqlx::query!(
        "INSERT INTO core.account_verifications (user_id, channel, code_hash, expires_at)
         VALUES ($1, $2::text::core.verification_channel, $3, core.now() + interval '15 minutes')",
        user.id,
        new.verification_channel,
        new.code_hash
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(user)
}

pub struct VerificationRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub code_hash: String,
}

pub async fn active_verification(
    db: &Db,
    email: &str,
    channel: &str,
) -> Result<Option<VerificationRecord>, sqlx::Error> {
    sqlx::query_as!(
        VerificationRecord,
        "SELECT v.id, v.user_id, v.code_hash
         FROM core.account_verifications v
         JOIN core.users u ON u.id = v.user_id
         WHERE u.email = $1::citext
           AND v.channel = $2::text::core.verification_channel
           AND v.consumed_at IS NULL AND v.expires_at >= core.now()
           AND v.attempts < 5
         ORDER BY v.created_at DESC LIMIT 1",
        email,
        channel
    )
    .fetch_optional(db)
    .await
}

pub async fn record_verification_failure(db: &Db, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE core.account_verifications SET attempts = attempts + 1
         WHERE id = $1 AND consumed_at IS NULL AND attempts < 5",
        id
    )
    .execute(db)
    .await?;
    Ok(())
}

pub async fn confirm_verification(
    db: &Db,
    verification_id: Uuid,
    user_id: Uuid,
    channel: &str,
) -> Result<bool, sqlx::Error> {
    crate::with_actor(db, user_id, async |tx| {
        let consumed = sqlx::query_scalar!(
            "UPDATE core.account_verifications
             SET consumed_at = core.now()
             WHERE id = $1 AND user_id = $2 AND consumed_at IS NULL
               AND expires_at >= core.now() AND attempts < 5
             RETURNING id",
            verification_id,
            user_id
        )
        .fetch_optional(&mut *tx)
        .await?;
        if consumed.is_none() {
            return Ok(false);
        }

        sqlx::query!(
            "UPDATE core.users
             SET email_confirmed_at = CASE WHEN $2 = 'email' THEN core.now() ELSE email_confirmed_at END,
                 phone_confirmed_at = CASE WHEN $2 = 'sms' THEN core.now() ELSE phone_confirmed_at END
             WHERE id = $1",
            user_id,
            channel
        )
        .execute(&mut *tx)
        .await?;
        Ok(true)
    })
    .await
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

/// Новый пароль учетной записи (W-07): и смена своего из сессии, и сброс
/// админом. Актор - тот, кто выполняет действие, поэтому в журнале видно,
/// сам ли человек сменил пароль или его сбросили извне.
///
/// Сам хеш в аудит не уезжает: триггер `audit.record_user()` кладет в payload
/// его отпечаток (см. миграцию `20260811000000_user_account_audit.sql`).
///
/// `false` - такой учетной записи нет; вызывающий отвечает 404, а не «готово».
pub async fn set_password(
    db: &Db,
    actor: Uuid,
    user_id: Uuid,
    password_hash: &str,
) -> Result<bool, sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        let updated = sqlx::query_scalar!(
            "UPDATE core.users SET password_hash = $2 WHERE id = $1 RETURNING id",
            user_id,
            password_hash
        )
        .fetch_optional(&mut *tx)
        .await?;
        Ok(updated.is_some())
    })
    .await
}

/// Деактивация и возврат учетной записи (W-07).
///
/// `is_active = false` закрывает оба пути сразу: [`find_by_email`] отдает
/// строку, но вход ее отвергает, а экстрактор `CurrentUser` перестает
/// признавать уже открытую сессию - живая вкладка уволившегося умирает на
/// первом же запросе, а не через восемь часов бездействия.
///
/// `false` в ответе - такой учетной записи нет.
pub async fn set_active(
    db: &Db,
    actor: Uuid,
    user_id: Uuid,
    is_active: bool,
) -> Result<bool, sqlx::Error> {
    crate::with_actor(db, actor, async |tx| {
        let updated = sqlx::query_scalar!(
            "UPDATE core.users SET is_active = $2 WHERE id = $1 RETURNING id",
            user_id,
            is_active
        )
        .fetch_optional(&mut *tx)
        .await?;
        Ok(updated.is_some())
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
