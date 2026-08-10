//! Тендерная комиссия (М11, FR-1101–1104): состав, явка и кворум заседания,
//! декларации конфликта интересов, отводы, личные голоса членов.
//!
//! Правила п. 9–15 стерегут триггеры миграции `commission_rules` - здесь
//! только запросы; отказ БД возвращается наверх текстом правила, чтобы
//! пользователь видел причину, а не «ошибку сервера».

use time::OffsetDateTime;
use tou_domain::commission::{Attendance, Composition, MemberRole, Tally, Vote};
use tou_domain::obligation::ObligationAction;
use uuid::Uuid;

use crate::Db;

/// Отказ правила комиссии (триггер БД) против прочих ошибок.
#[derive(Debug, thiserror::Error)]
pub enum CommissionError {
    #[error("не найдено")]
    NotFound,
    /// Текст правила из триггера (FR-1101–1104)
    #[error("{0}")]
    Rejected(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Триггеры комиссии сигналят `raise_exception` (P0001); FK/CHECK - 23xxx.
pub(crate) fn map_rule(err: sqlx::Error) -> CommissionError {
    if let sqlx::Error::Database(db_err) = &err
        && matches!(
            db_err.code().as_deref(),
            Some("P0001") | Some("23514") | Some("23503") | Some("23505")
        )
    {
        return CommissionError::Rejected(db_err.message().to_owned());
    }
    CommissionError::Db(err)
}

pub struct CommissionRecord {
    pub id: Uuid,
    pub name: String,
    pub valid_from: time::Date,
    pub valid_until: time::Date,
    pub approved_at: Option<OffsetDateTime>,
}

/// Действующая комиссия на сегодня (срок полномочий - п. 9–11).
pub async fn active(db: &Db) -> Result<Option<CommissionRecord>, sqlx::Error> {
    sqlx::query_as!(
        CommissionRecord,
        "SELECT id, name, valid_from, valid_until, approved_at
         FROM core.commissions
         WHERE valid_from <= current_date AND current_date < valid_until
         ORDER BY approved_at DESC NULLS LAST, valid_from DESC
         LIMIT 1"
    )
    .fetch_optional(db)
    .await
}

pub struct MemberRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub full_name: String,
    pub member_role: MemberRole,
}

/// Строка выборки: `member_role` - еще текст из БД. Отдельный тип, потому что
/// `query_as!` кладет столбцы в поля как есть, а `MemberRole` - доменный тип:
/// чтобы макрос разбирал его сам, домену пришлось бы зависеть от sqlx (арх. § 5).
struct MemberRaw {
    id: Uuid,
    user_id: Uuid,
    full_name: String,
    member_role: String,
}

impl TryFrom<MemberRaw> for MemberRow {
    type Error = sqlx::Error;

    fn try_from(row: MemberRaw) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            user_id: row.user_id,
            full_name: row.full_name,
            member_role: row
                .member_role
                .parse()
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
        })
    }
}

/// Выборка члена комиссии: общий список столбцов + хвост запроса.
/// `!` у `member_role` - это `::text`, который планировщик считает
/// потенциально NULL, хотя столбец NOT NULL.
macro_rules! member_query {
    ($tail:literal $(, $arg:expr)*) => {
        sqlx::query_as!(
            MemberRaw,
            r#"SELECT cm.id, cm.user_id, u.full_name,
                      cm.member_role::text AS "member_role!"
               FROM core.commission_members cm
               JOIN core.users u ON u.id = cm.user_id"# + $tail
            $(, $arg)*
        )
    };
}

pub async fn members(db: &Db, commission_id: Uuid) -> Result<Vec<MemberRow>, sqlx::Error> {
    member_query!(
        " WHERE cm.commission_id = $1 ORDER BY cm.member_role, u.full_name",
        commission_id
    )
    .fetch_all(db)
    .await?
    .into_iter()
    .map(MemberRow::try_from)
    .collect()
}

/// Состав в разрезе ролей - вход проверки FR-1101 в домене.
pub fn composition_of(members: &[MemberRow]) -> Composition {
    Composition::of(members.iter().map(|m| m.member_role))
}

/// Членство пользователя в комиссии (кабинет члена комиссии).
pub async fn member_of(
    db: &Db,
    commission_id: Uuid,
    user_id: Uuid,
) -> Result<Option<MemberRow>, sqlx::Error> {
    member_query!(
        " WHERE cm.commission_id = $1 AND cm.user_id = $2",
        commission_id,
        user_id
    )
    .fetch_optional(db)
    .await?
    .map(MemberRow::try_from)
    .transpose()
}

/// Утверждение состава (FR-1101): проверку выполняет триггер БД.
pub async fn approve(db: &Db, actor: Uuid, commission_id: Uuid) -> Result<(), CommissionError> {
    let updated = crate::with_actor(db, actor, async |tx| {
        sqlx::query!(
            "UPDATE core.commissions SET approved_at = core.now(), approved_by = $2 WHERE id = $1",
            commission_id,
            actor
        )
        .execute(&mut *tx)
        .await
        .map(|done| done.rows_affected())
        .map_err(map_rule)
    })
    .await?;

    if updated == 0 {
        return Err(CommissionError::NotFound);
    }
    Ok(())
}

// ------------------------------------------------------------- заседание ---

pub struct AttendanceRow {
    pub member_id: Uuid,
    pub full_name: String,
    pub member_role: MemberRole,
    pub present: bool,
    pub chairing: bool,
}

/// Строка выборки явки: `member_role` - еще текст из БД (см. [`MemberRaw`]).
struct AttendanceRaw {
    member_id: Uuid,
    full_name: String,
    member_role: String,
    present: bool,
    chairing: bool,
}

impl TryFrom<AttendanceRaw> for AttendanceRow {
    type Error = sqlx::Error;

    fn try_from(row: AttendanceRaw) -> Result<Self, Self::Error> {
        Ok(Self {
            member_id: row.member_id,
            full_name: row.full_name,
            member_role: row
                .member_role
                .parse()
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            present: row.present,
            chairing: row.chairing,
        })
    }
}

pub struct NewAttendance {
    pub member_id: Uuid,
    pub present: bool,
    pub chairing: bool,
}

/// Отметка явки (FR-1102, п. 12): секретарь ведет ее до открытия заседания;
/// после открытия состав присутствующих зафиксирован - иначе кворум,
/// с которым заседание открыли, можно было бы «переписать» задним числом.
pub async fn record_attendance(
    db: &Db,
    actor: Uuid,
    meeting_id: Uuid,
    rows: &[NewAttendance],
) -> Result<Vec<AttendanceRow>, CommissionError> {
    crate::with_actor(db, actor, async |tx| {
        let opened = sqlx::query_scalar!(
            "SELECT opened_at FROM core.sessions_meetings WHERE id = $1",
            meeting_id
        )
        .fetch_optional(&mut *tx)
        .await?;
        match opened {
            None => return Err(CommissionError::NotFound),
            Some(Some(_)) => {
                return Err(CommissionError::Rejected(
                    "FR-1102: заседание уже открыто - явка зафиксирована (п. 12)".to_owned(),
                ));
            }
            Some(None) => {}
        }

        for row in rows {
            sqlx::query!(
                "INSERT INTO core.meeting_attendance (meeting_id, member_id, present, chairing)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (meeting_id, member_id)
                 DO UPDATE SET present = EXCLUDED.present, chairing = EXCLUDED.chairing,
                               recorded_at = core.now()",
                meeting_id,
                row.member_id,
                row.present,
                row.chairing
            )
            .execute(&mut *tx)
            .await
            .map_err(map_rule)?;
        }

        attendance_in(&mut *tx, meeting_id).await.map_err(map_rule)
    })
    .await
}

async fn attendance_in(
    tx: &mut sqlx::PgConnection,
    meeting_id: Uuid,
) -> Result<Vec<AttendanceRow>, sqlx::Error> {
    sqlx::query_as!(
        AttendanceRaw,
        r#"SELECT a.member_id, u.full_name, cm.member_role::text AS "member_role!",
                a.present, a.chairing
         FROM core.meeting_attendance a
         JOIN core.commission_members cm ON cm.id = a.member_id
         JOIN core.users u ON u.id = cm.user_id
         WHERE a.meeting_id = $1
         ORDER BY cm.member_role, u.full_name"#,
        meeting_id
    )
    .fetch_all(tx)
    .await?
    .into_iter()
    .map(AttendanceRow::try_from)
    .collect()
}

pub async fn attendance(db: &Db, meeting_id: Uuid) -> Result<Vec<AttendanceRow>, sqlx::Error> {
    let mut conn = db.acquire().await?;
    attendance_in(&mut conn, meeting_id).await
}

/// Явка в терминах домена: кворум считает [`Attendance::check`], но тот же
/// расчет независимо выполняет триггер БД - расхождение невозможно.
pub fn attendance_summary(members: &[MemberRow], rows: &[AttendanceRow]) -> Attendance {
    let voting_total = composition_of(members).voting();
    let present = rows
        .iter()
        .filter(|row| row.present && row.member_role.votes())
        .count();
    let chair_present = rows
        .iter()
        .any(|row| row.present && row.member_role.may_chair());

    Attendance {
        voting_total,
        present,
        chair_present,
    }
}

/// Открытие заседания (FR-1102): кворум проверяет триггер БД, он же
/// записывает `quorum_present` / `quorum_required` в протокольную часть.
pub async fn open_meeting(db: &Db, actor: Uuid, meeting_id: Uuid) -> Result<(), CommissionError> {
    let updated = crate::with_actor(db, actor, async |tx| {
        let updated = sqlx::query!(
            "UPDATE core.sessions_meetings
             SET opened_at = core.now(), held_at = COALESCE(held_at, core.now())
             WHERE id = $1 AND opened_at IS NULL",
            meeting_id
        )
        .execute(&mut *tx)
        .await
        .map(|done| done.rows_affected())
        .map_err(map_rule)?;

        // Заседание состоялось - пошел срок протокола допуска (FR-1702, п. 54)
        if updated > 0 {
            let tender_id = sqlx::query_scalar!(
                "SELECT tender_id FROM core.sessions_meetings WHERE id = $1",
                meeting_id
            )
            .fetch_one(&mut *tx)
            .await?;
            crate::obligations::schedule(
                &mut *tx,
                ObligationAction::AdmissionProtocol,
                crate::obligations::Subject::tender(tender_id),
            )
            .await?;
        }
        Ok::<u64, CommissionError>(updated)
    })
    .await?;

    if updated == 0 {
        return Err(CommissionError::Rejected(
            "заседание не найдено или уже открыто".to_owned(),
        ));
    }
    Ok(())
}

// ---------------------------------------- конфликт интересов и отвод ---

/// Декларация об отсутствии конфликта интересов до заседания (FR-1104, п. 15).
/// Подает сам член комиссии; повторная подача обновляет ее.
pub async fn declare_conflict(
    db: &Db,
    actor: Uuid,
    member_id: Uuid,
    tender_id: Uuid,
    has_conflict: bool,
    details: Option<&str>,
) -> Result<(), CommissionError> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query!(
            "INSERT INTO core.coi_declarations (member_id, tender_id, has_conflict, details)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (member_id, tender_id)
             DO UPDATE SET has_conflict = EXCLUDED.has_conflict,
                           details = EXCLUDED.details, declared_at = core.now()",
            member_id,
            tender_id,
            has_conflict,
            details
        )
        .execute(&mut *tx)
        .await
        .map(|_| ())
        .map_err(map_rule)
    })
    .await
}

pub struct DeclarationRow {
    pub member_id: Uuid,
    pub full_name: String,
    pub has_conflict: bool,
    pub details: Option<String>,
    pub declared_at: OffsetDateTime,
}

pub async fn declarations(db: &Db, tender_id: Uuid) -> Result<Vec<DeclarationRow>, sqlx::Error> {
    sqlx::query_as!(
        DeclarationRow,
        "SELECT d.member_id, u.full_name, d.has_conflict, d.details, d.declared_at
         FROM core.coi_declarations d
         JOIN core.commission_members cm ON cm.id = d.member_id
         JOIN core.users u ON u.id = cm.user_id
         WHERE d.tender_id = $1
         ORDER BY u.full_name",
        tender_id
    )
    .fetch_all(db)
    .await
}

pub struct NewRecusal<'a> {
    pub tender_id: Uuid,
    pub member_id: Uuid,
    /// `None` - отвод по всему тендеру
    pub lot_id: Option<Uuid>,
    pub reason: &'a str,
    /// Резервный член, заменяющий отведенного (п. 15)
    pub replacement_member_id: Option<Uuid>,
}

/// Отвод члена комиссии (FR-1104): решение большинства фиксирует секретарь,
/// отведенный теряет доступ к материалам лота (RLS) и право голоса (триггер).
pub async fn recuse(db: &Db, actor: Uuid, new: NewRecusal<'_>) -> Result<(), CommissionError> {
    crate::with_actor(db, actor, async |tx| {
        sqlx::query!(
            "INSERT INTO core.member_recusals
               (tender_id, member_id, lot_id, reason, replacement_member_id, decided_by)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (tender_id, member_id)
             DO UPDATE SET lot_id = EXCLUDED.lot_id, reason = EXCLUDED.reason,
                           replacement_member_id = EXCLUDED.replacement_member_id,
                           decided_at = core.now(), decided_by = EXCLUDED.decided_by",
            new.tender_id,
            new.member_id,
            new.lot_id,
            new.reason,
            new.replacement_member_id,
            actor
        )
        .execute(&mut *tx)
        .await
        .map(|_| ())
        .map_err(map_rule)
    })
    .await
}

pub struct RecusalRow {
    pub member_id: Uuid,
    pub full_name: String,
    pub lot_id: Option<Uuid>,
    pub reason: String,
    pub replacement_member_id: Option<Uuid>,
    pub replacement_name: Option<String>,
    pub decided_at: OffsetDateTime,
}

/// Отводы по тендеру. `replacement_name` получает `?`: замена приходит
/// `LEFT JOIN`'ом, а `core.users.full_name` - NOT NULL, и без аннотации sqlx
/// вывел бы non-null по самому столбцу.
pub async fn recusals(db: &Db, tender_id: Uuid) -> Result<Vec<RecusalRow>, sqlx::Error> {
    sqlx::query_as!(
        RecusalRow,
        r#"SELECT r.member_id, u.full_name, r.lot_id, r.reason,
                r.replacement_member_id, ru.full_name AS "replacement_name?", r.decided_at
         FROM core.member_recusals r
         JOIN core.commission_members cm ON cm.id = r.member_id
         JOIN core.users u ON u.id = cm.user_id
         LEFT JOIN core.commission_members rcm ON rcm.id = r.replacement_member_id
         LEFT JOIN core.users ru ON ru.id = rcm.user_id
         WHERE r.tender_id = $1
         ORDER BY u.full_name"#,
        tender_id
    )
    .fetch_all(db)
    .await
}

// ------------------------------------------------------------ голосование ---

/// Личный голос члена комиссии (FR-1103). Право голоса стерегут триггеры:
/// открытое заседание, присутствие, отсутствие отвода, состав комиссии.
/// Переголосовать можно, пока решение по заявке не принято.
pub async fn cast_vote(
    db: &Db,
    actor: Uuid,
    meeting_id: Uuid,
    application_id: Uuid,
    member_id: Uuid,
    value: Vote,
    dissent: Option<&str>,
) -> Result<(), CommissionError> {
    crate::with_actor(db, actor, async |tx| {
        let decided = sqlx::query_scalar!(
            r#"SELECT status::text AS "status!" FROM core.applications WHERE id = $1"#,
            application_id
        )
        .fetch_optional(&mut *tx)
        .await?;
        match decided.as_deref() {
            None => return Err(CommissionError::NotFound),
            Some("admitted") | Some("rejected") => {
                return Err(CommissionError::Rejected(
                    "FR-1103: решение по заявке уже принято - голос не меняется".to_owned(),
                ));
            }
            Some(_) => {}
        }

        // `$4::text::core.vote_value`: голос приходит строкой доменного типа,
        // а приведение к перечислению делает БД
        sqlx::query!(
            "INSERT INTO core.votes (meeting_id, application_id, member_id, value, dissent)
             VALUES ($1, $2, $3, $4::text::core.vote_value, $5)
             ON CONFLICT (meeting_id, application_id, member_id)
             DO UPDATE SET value = EXCLUDED.value, dissent = EXCLUDED.dissent, cast_at = core.now()",
            meeting_id,
            application_id,
            member_id,
            value.as_str(),
            dissent
        )
        .execute(&mut *tx)
        .await
        .map(|_| ())
        .map_err(map_rule)
    })
    .await
}

/// Подсчет голосов по заявке (FR-1103): база большинства - присутствующие
/// с правом голоса по этому лоту (отведенные и резервные без замены - нет).
pub async fn tally(db: &Db, meeting_id: Uuid, application_id: Uuid) -> Result<Tally, sqlx::Error> {
    // `!` у счетчиков и `::bigint`: агрегаты и приведения планировщик считает
    // потенциально NULL. `chair_vote` нулевой быть может - председатель мог
    // и не голосовать
    let row = sqlx::query!(
        r#"WITH app AS (
           SELECT a.id, a.tender_id, a.lot_id FROM core.applications a WHERE a.id = $2
         ),
         eligible AS (
           SELECT cm.id AS member_id, cm.member_role
           FROM core.meeting_attendance att
           JOIN core.commission_members cm ON cm.id = att.member_id
           CROSS JOIN app
           WHERE att.meeting_id = $1 AND att.present
             AND NOT core.member_recused(cm.id, app.tender_id, app.lot_id)
             AND (cm.member_role <> 'reserve'
                  OR EXISTS (SELECT 1 FROM core.member_recusals r
                             WHERE r.replacement_member_id = cm.id
                               AND r.tender_id = app.tender_id
                               AND (r.lot_id IS NULL OR r.lot_id = app.lot_id)))
         )
         SELECT
           (SELECT count(*) FROM eligible)::bigint AS "eligible!",
           count(*) FILTER (WHERE v.value = 'for')::bigint AS "votes_for!",
           count(*) FILTER (WHERE v.value = 'against')::bigint AS "votes_against!",
           max(CASE WHEN att.chairing THEN v.value::text END) AS chair_vote
         FROM eligible e
         LEFT JOIN core.votes v
           ON v.member_id = e.member_id AND v.meeting_id = $1 AND v.application_id = $2
         LEFT JOIN core.meeting_attendance att
           ON att.meeting_id = $1 AND att.member_id = e.member_id"#,
        meeting_id,
        application_id
    )
    .fetch_one(db)
    .await?;

    Ok(Tally {
        eligible: row.eligible.max(0) as usize,
        votes_for: row.votes_for.max(0) as usize,
        votes_against: row.votes_against.max(0) as usize,
        chair_vote: row
            .chair_vote
            .as_deref()
            .and_then(|value| value.parse::<Vote>().ok()),
    })
}
