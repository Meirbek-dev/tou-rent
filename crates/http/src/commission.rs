//! Тендерная комиссия (М11, FR-1101–1104): состав и его утверждение, явка
//! и кворум заседания, декларации конфликта интересов и отводы, личные
//! голоса членов.
//!
//! Правила п. 9–15 закреплены триггерами БД и типами домена; здесь - только
//! доступ (INV-POL-01) и перевод отказов в problem+json.

use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tou_db::commission::{self, CommissionError, NewAttendance, NewRecusal};
use tou_domain::commission::{Decision, MemberRole, Vote};
use tou_domain::policy::Action;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::request::{Json, Path};
use crate::state::AppState;
use tou_domain::rule::RuleViolation;

fn rule_error(err: CommissionError) -> ApiError {
    match err {
        CommissionError::NotFound => ApiError::NotFound,
        CommissionError::Rejected(reason) => ApiError::RuleViolation(reason),
        CommissionError::Db(db) => db.into(),
    }
}

/// Роль в составе (паритет с enum БД `core.commission_member_role`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemberRoleDto {
    Chairman,
    Deputy,
    Member,
    Reserve,
}

impl From<MemberRole> for MemberRoleDto {
    fn from(role: MemberRole) -> Self {
        match role {
            MemberRole::Chairman => MemberRoleDto::Chairman,
            MemberRole::Deputy => MemberRoleDto::Deputy,
            MemberRole::Member => MemberRoleDto::Member,
            MemberRole::Reserve => MemberRoleDto::Reserve,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MemberDto {
    pub member_id: Uuid,
    pub user_id: Uuid,
    pub full_name: String,
    pub member_role: MemberRoleDto,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CommissionDto {
    pub id: Uuid,
    pub name: String,
    #[serde(with = "crate::dto::iso_date")]
    #[schema(value_type = String, format = Date)]
    pub valid_from: time::Date,
    #[serde(with = "crate::dto::iso_date")]
    #[schema(value_type = String, format = Date)]
    pub valid_until: time::Date,
    /// Состав утвержден и проверен по п. 9–11 (FR-1101)
    pub approved: bool,
    /// Голосующий состав: председатель, заместитель и члены (без резервных)
    pub voting_total: usize,
    /// Кворум ⅔ голосующего состава (п. 12)
    pub quorum_required: usize,
    /// Почему состав нельзя утвердить (если нельзя)
    pub composition_error: Option<String>,
    pub members: Vec<MemberDto>,
}

/// Действующая комиссия и ее состав (FR-1101).
#[utoipa::path(
    get,
    path = "/api/v1/commissions/active",
    tag = "commission",
    responses(
        (status = 200, description = "Действующая комиссия", body = CommissionDto),
        (status = 404, description = "Действующей комиссии нет", body = crate::error::Problem),
    )
)]
pub async fn active_commission(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<CommissionDto>, ApiError> {
    user.require(Action::TenderRead)?;

    let record = commission::active(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let members = commission::members(&state.db, record.id).await?;
    Ok(Json(commission_dto(record, members)))
}

fn commission_dto(
    record: commission::CommissionRecord,
    members: Vec<commission::MemberRow>,
) -> CommissionDto {
    let composition = commission::composition_of(&members);
    CommissionDto {
        id: record.id,
        name: record.name,
        valid_from: record.valid_from,
        valid_until: record.valid_until,
        approved: record.approved_at.is_some(),
        voting_total: composition.voting(),
        quorum_required: tou_domain::commission::quorum_required(composition.voting()),
        composition_error: composition.validate().err().map(|e| e.to_string()),
        members: members
            .into_iter()
            .map(|m| MemberDto {
                member_id: m.id,
                user_id: m.user_id,
                full_name: m.full_name,
                member_role: m.member_role.into(),
            })
            .collect(),
    }
}

/// Утверждение состава (FR-1101): проверку п. 9–11 выполняет БД.
#[utoipa::path(
    post,
    path = "/api/v1/commissions/{id}/approve",
    tag = "commission",
    params(("id" = Uuid, Path, description = "Комиссия")),
    responses(
        (status = 200, description = "Состав утвержден", body = CommissionDto),
        (status = 409, description = "Состав не отвечает п. 9–11", body = crate::error::Problem),
    )
)]
pub async fn approve_commission(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<CommissionDto>, ApiError> {
    user.require(Action::CommissionManage)?;

    commission::approve(&state.db, user.id(), id)
        .await
        .map_err(rule_error)?;

    let record = commission::active(&state.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let members = commission::members(&state.db, record.id).await?;
    Ok(Json(commission_dto(record, members)))
}

// -------------------------------------------------- конфликт интересов ---

#[derive(Debug, Deserialize, ToSchema)]
pub struct CoiRequest {
    /// true - конфликт есть, комиссия решает вопрос об отводе (п. 15)
    pub has_conflict: bool,
    pub details: Option<String>,
}

/// Декларация об отсутствии конфликта интересов до заседания (FR-1104).
/// Подает член комиссии лично - за него это сделать нельзя.
#[utoipa::path(
    post,
    path = "/api/v1/tenders/{id}/conflict-of-interest",
    tag = "commission",
    params(("id" = Uuid, Path, description = "Тендер")),
    request_body = CoiRequest,
    responses(
        (status = 204, description = "Декларация принята"),
        (status = 403, description = "Пользователь не член действующей комиссии", body = crate::error::Problem),
    )
)]
pub async fn declare_coi(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<CoiRequest>,
) -> Result<StatusCode, ApiError> {
    user.require(Action::CoiDeclare)?;
    let member = current_member(&state, &user).await?;

    commission::declare_conflict(
        &state.db,
        user.id(),
        member.id,
        id,
        body.has_conflict,
        body.details.as_deref().filter(|d| !d.is_empty()),
    )
    .await
    .map_err(rule_error)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Член действующей комиссии, соответствующий текущему пользователю.
async fn current_member(
    state: &AppState,
    user: &CurrentUser,
) -> Result<commission::MemberRow, ApiError> {
    let record = commission::active(&state.db).await?.ok_or_else(|| {
        ApiError::rule(
            RuleViolation::CommissionComposition,
            "действующей комиссии нет",
        )
    })?;
    commission::member_of(&state.db, record.id, user.id())
        .await?
        .ok_or(ApiError::Forbidden)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RecusalRequest {
    pub member_id: Uuid,
    /// Отвод по одному лоту; без него - по всему тендеру
    pub lot_id: Option<Uuid>,
    /// Основание отвода - идет в протокол (п. 15)
    pub reason: String,
    /// Резервный член, заменяющий отведенного (п. 15)
    pub replacement_member_id: Option<Uuid>,
}

/// Отвод члена комиссии (FR-1104): решение большинства фиксирует секретарь;
/// отведенный теряет доступ к материалам лота (RLS) и право голоса.
#[utoipa::path(
    post,
    path = "/api/v1/tenders/{id}/recusals",
    tag = "commission",
    params(("id" = Uuid, Path, description = "Тендер")),
    request_body = RecusalRequest,
    responses(
        (status = 204, description = "Отвод зафиксирован"),
        (status = 409, description = "Отвод невозможен", body = crate::error::Problem),
        (status = 422, description = "Основание отвода не указано", body = crate::error::Problem),
    )
)]
pub async fn recuse_member(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<RecusalRequest>,
) -> Result<StatusCode, ApiError> {
    user.require(Action::MeetingManage)?;

    let reason = body.reason.trim();
    if reason.is_empty() {
        return Err(ApiError::Validation(
            "отвод фиксируется с основанием (п. 15)".to_owned(),
        ));
    }

    commission::recuse(
        &state.db,
        user.id(),
        NewRecusal {
            tender_id: id,
            member_id: body.member_id,
            lot_id: body.lot_id,
            reason,
            replacement_member_id: body.replacement_member_id,
        },
    )
    .await
    .map_err(rule_error)?;

    Ok(StatusCode::NO_CONTENT)
}

// -------------------------------------------------------- голосование ---

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VoteValueDto {
    For,
    Against,
}

impl From<VoteValueDto> for Vote {
    fn from(value: VoteValueDto) -> Self {
        match value {
            VoteValueDto::For => Vote::For,
            VoteValueDto::Against => Vote::Against,
        }
    }
}

impl From<Vote> for VoteValueDto {
    fn from(value: Vote) -> Self {
        match value {
            Vote::For => VoteValueDto::For,
            Vote::Against => VoteValueDto::Against,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CastVoteRequest {
    pub value: VoteValueDto,
    /// Особое мнение прикладывается к протоколу (п. 13–14, 55.8)
    pub dissent: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TallyDto {
    /// Присутствующие с правом голоса по этому лоту (база большинства, п. 13)
    pub eligible: usize,
    pub votes_for: usize,
    pub votes_against: usize,
    /// Голос председательствующего - решает при равенстве (п. 14)
    pub chair_vote: Option<VoteValueDto>,
    /// Итог, когда проголосовали все присутствующие; иначе причина ожидания
    pub outcome: Option<DecisionDto>,
    pub pending: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DecisionDto {
    Admitted,
    Rejected,
}

impl From<Decision> for DecisionDto {
    fn from(value: Decision) -> Self {
        match value {
            Decision::Admitted => DecisionDto::Admitted,
            Decision::Rejected => DecisionDto::Rejected,
        }
    }
}

pub(crate) fn tally_dto(tally: tou_domain::commission::Tally) -> TallyDto {
    let outcome = tally.outcome();
    TallyDto {
        eligible: tally.eligible,
        votes_for: tally.votes_for,
        votes_against: tally.votes_against,
        chair_vote: tally.chair_vote.map(VoteValueDto::from),
        outcome: outcome.as_ref().ok().copied().map(DecisionDto::from),
        pending: outcome.err().map(|e| e.to_string()),
    }
}

/// Личный голос члена комиссии по заявке (FR-1103). Голосуют только
/// присутствующие на открытом заседании и не отведенные - это стерегут
/// триггеры БД; «воздержался» не существует типом (INV-055).
#[utoipa::path(
    post,
    path = "/api/v1/applications/{id}/vote",
    tag = "commission",
    params(("id" = Uuid, Path, description = "Заявка")),
    request_body = CastVoteRequest,
    responses(
        (status = 200, description = "Голос учтен", body = TallyDto),
        (status = 403, description = "Пользователь не член действующей комиссии", body = crate::error::Problem),
        (status = 409, description = "Голосование сейчас невозможно", body = crate::error::Problem),
    )
)]
pub async fn cast_vote(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<CastVoteRequest>,
) -> Result<Json<TallyDto>, ApiError> {
    user.require(Action::VoteCast)?;
    let member = current_member(&state, &user).await?;

    let application = tou_db::applications::get(&state.db, user.id(), id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let meeting = tou_db::admission::qualification_meeting(&state.db, application.tender_id)
        .await?
        .ok_or_else(|| {
            ApiError::rule(
                RuleViolation::CommissionMeeting,
                "заседание комиссии по тендеру не назначено",
            )
        })?;

    commission::cast_vote(
        &state.db,
        user.id(),
        meeting.id,
        id,
        member.id,
        body.value.into(),
        body.dissent.as_deref().filter(|d| !d.is_empty()),
    )
    .await
    .map_err(rule_error)?;

    let tally = commission::tally(&state.db, meeting.id, id).await?;
    Ok(Json(tally_dto(tally)))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct VoteDto {
    pub member_id: Uuid,
    pub member_name: String,
    pub value: VoteValueDto,
    pub dissent: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApplicationVotesDto {
    pub tally: TallyDto,
    pub votes: Vec<VoteDto>,
}

/// Голоса по заявке и текущий подсчет (FR-1103): видят секретарь и комиссия.
#[utoipa::path(
    get,
    path = "/api/v1/applications/{id}/votes",
    tag = "commission",
    params(("id" = Uuid, Path, description = "Заявка")),
    responses(
        (status = 200, description = "Голоса и подсчет", body = ApplicationVotesDto),
        (status = 404, description = "Заявка или заседание не найдены", body = crate::error::Problem),
    )
)]
pub async fn application_votes(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApplicationVotesDto>, ApiError> {
    user.require(Action::ApplicationReadAll)?;

    let application = tou_db::applications::get(&state.db, user.id(), id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let meeting = tou_db::admission::qualification_meeting(&state.db, application.tender_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let tally = commission::tally(&state.db, meeting.id, id).await?;
    let votes = tou_db::admission::votes_of_meeting(&state.db, meeting.id).await?;

    Ok(Json(ApplicationVotesDto {
        tally: tally_dto(tally),
        votes: votes
            .into_iter()
            .filter(|vote| vote.application_id == id)
            .map(|vote| VoteDto {
                member_id: vote.member_id,
                member_name: vote.member_name,
                value: match vote.value.as_str() {
                    "against" => VoteValueDto::Against,
                    _ => VoteValueDto::For,
                },
                dissent: vote.dissent,
            })
            .collect(),
    }))
}

// ---------------------------------------------------- явка и кворум ---

#[derive(Debug, Deserialize, ToSchema)]
pub struct AttendanceInputDto {
    pub member_id: Uuid,
    pub present: bool,
    /// Председательствующий - один на заседание, председатель или заместитель
    #[serde(default)]
    pub chairing: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AttendanceRequest {
    pub rows: Vec<AttendanceInputDto>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AttendanceRowDto {
    pub member_id: Uuid,
    pub full_name: String,
    pub member_role: MemberRoleDto,
    pub present: bool,
    pub chairing: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeclarationDto {
    pub member_id: Uuid,
    pub full_name: String,
    pub has_conflict: bool,
    pub details: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub declared_at: OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RecusalDto {
    pub member_id: Uuid,
    pub full_name: String,
    pub lot_id: Option<Uuid>,
    pub reason: String,
    pub replacement_member_id: Option<Uuid>,
    pub replacement_name: Option<String>,
}

/// Отметка явки на заседание (FR-1102): ведет секретарь до открытия.
/// Заседание создается при первой отметке - вести его будет действующая
/// комиссия с утвержденным составом.
#[utoipa::path(
    post,
    path = "/api/v1/tenders/{id}/meeting/attendance",
    tag = "commission",
    params(("id" = Uuid, Path, description = "Тендер")),
    request_body = AttendanceRequest,
    responses(
        (status = 200, description = "Явка отмечена", body = [AttendanceRowDto]),
        (status = 409, description = "Заседание уже открыто либо нет утвержденной комиссии", body = crate::error::Problem),
    )
)]
pub async fn record_attendance(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AttendanceRequest>,
) -> Result<Json<Vec<AttendanceRowDto>>, ApiError> {
    user.require(Action::MeetingManage)?;

    let meeting = tou_db::admission::ensure_qualification_meeting(&state.db, user.id(), id)
        .await
        .map_err(crate::admission::open_error)?;

    // Председательствующий обязан присутствовать (п. 12) - то же стережет
    // CHECK БД, но пользователю нужна причина, а не текст constraint
    if body.rows.iter().any(|row| row.chairing && !row.present) {
        return Err(ApiError::Validation(
            "председательствующий обязан присутствовать на заседании (п. 12)".to_owned(),
        ));
    }

    let rows: Vec<NewAttendance> = body
        .rows
        .iter()
        .map(|row| NewAttendance {
            member_id: row.member_id,
            present: row.present,
            chairing: row.chairing,
        })
        .collect();

    let recorded = commission::record_attendance(&state.db, user.id(), meeting.id, &rows)
        .await
        .map_err(rule_error)?;

    Ok(Json(recorded.into_iter().map(attendance_dto).collect()))
}

pub(crate) fn attendance_dto(row: commission::AttendanceRow) -> AttendanceRowDto {
    AttendanceRowDto {
        member_id: row.member_id,
        full_name: row.full_name,
        member_role: row.member_role.into(),
        present: row.present,
        chairing: row.chairing,
    }
}

/// Открытие заседания (FR-1102): кворум ⅔ с председателем или заместителем
/// проверяет триггер БД - без кворума заседание не открывается (п. 12).
#[utoipa::path(
    post,
    path = "/api/v1/tenders/{id}/meeting/open",
    tag = "commission",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 204, description = "Заседание открыто"),
        (status = 409, description = "Кворума нет либо заседание уже открыто", body = crate::error::Problem),
    )
)]
pub async fn open_meeting(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    user.require(Action::MeetingManage)?;

    let meeting = tou_db::admission::qualification_meeting(&state.db, id)
        .await?
        .ok_or_else(|| {
            ApiError::rule(
                RuleViolation::CommissionMeeting,
                "явка не отмечена - заседание не назначено (п. 12)",
            )
        })?;

    commission::open_meeting(&state.db, user.id(), meeting.id)
        .await
        .map_err(rule_error)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Декларации и отводы по тендеру (FR-1104): идут в протокол допуска.
pub(crate) async fn declarations_and_recusals(
    state: &AppState,
    tender_id: Uuid,
) -> Result<(Vec<DeclarationDto>, Vec<RecusalDto>), ApiError> {
    let declarations = commission::declarations(&state.db, tender_id).await?;
    let recusals = commission::recusals(&state.db, tender_id).await?;

    Ok((
        declarations
            .into_iter()
            .map(|d| DeclarationDto {
                member_id: d.member_id,
                full_name: d.full_name,
                has_conflict: d.has_conflict,
                details: d.details,
                declared_at: d.declared_at,
            })
            .collect(),
        recusals
            .into_iter()
            .map(|r| RecusalDto {
                member_id: r.member_id,
                full_name: r.full_name,
                lot_id: r.lot_id,
                reason: r.reason,
                replacement_member_id: r.replacement_member_id,
                replacement_name: r.replacement_name,
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Паритет имен на проводе с enum БД `core.vote_value` и доменом
    /// (INV-055: вариантов ровно два, «воздержался» нет).
    #[test]
    fn vote_wire_names_match_db_enum() {
        for (dto, wire) in [
            (VoteValueDto::For, "for"),
            (VoteValueDto::Against, "against"),
        ] {
            assert_eq!(
                serde_json::to_value(dto).expect("сериализация"),
                serde_json::Value::String(wire.to_owned())
            );
            assert_eq!(Vote::from(dto).as_str(), wire);
        }
    }

    /// Пока проголосовали не все присутствующие, вердикта нет - подсчет
    /// объясняет, чего ждет (FR-1103).
    #[test]
    fn tally_dto_reports_pending_until_everyone_voted() {
        let pending = tally_dto(tou_domain::commission::Tally {
            eligible: 5,
            votes_for: 2,
            votes_against: 1,
            chair_vote: None,
        });
        assert!(pending.outcome.is_none());
        assert!(pending.pending.is_some_and(|text| text.contains("из 5")));

        let done = tally_dto(tou_domain::commission::Tally {
            eligible: 3,
            votes_for: 2,
            votes_against: 1,
            chair_vote: Some(Vote::Against),
        });
        assert!(matches!(done.outcome, Some(DecisionDto::Admitted)));
        assert!(done.pending.is_none());
    }
}
