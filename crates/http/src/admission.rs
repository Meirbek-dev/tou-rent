//! Вскрытие и допуск (М5, FR-501–503): переходы и решения - через db-слой
//! (триггеры INV-021 и CHECK стерегут время вскрытия, FK - основания INV-052),
//! протокол допуска - jsonb-снимок + Typst-PDF в RustFS (п. 54–55).

use std::collections::HashMap;

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use object_store::ObjectStoreExt as _;
use object_store::PutPayload;
use object_store::path::Path as ObjectPath;
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;
use time::macros::format_description;
use tou_application::admission::NotifyAdmitted;
use tou_db::admission::{self, DecideError, OpenError};
use tou_db::notifications::{NewNotification, PgNotifyAdmittedStore};
use tou_db::{applications, tenders};
use tou_domain::notification::NotificationKind;
use tou_domain::policy::Action;
use tou_ports::notifications::NotifyAdmittedError;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::applications::ApplicationDto;
use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::pdf;
use crate::request::{Json, Path};
use crate::state::AppState;
use crate::tenders::TenderDto;

const TEMPLATE: &str = include_str!("templates/admission.typ");

/// Вскрытие конвертов (FR-501): `accepting → qualification`, `opened_at=core.now()`,
/// заседание комиссии. Раньше времени заседания БД вскрыть не даст (CHECK).
#[utoipa::path(
    post,
    path = "/api/v1/tenders/{id}/open",
    tag = "admission",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 200, description = "Конверты вскрыты, цены открыты (INV-040)", body = TenderDto),
        (status = 409, description = "Вскрытие невозможно (статус/время/комиссия)", body = crate::error::Problem),
    )
)]
pub async fn open_tender(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<TenderDto>, ApiError> {
    user.require(Action::OpeningPerform)?;

    let (record, _meeting) = admission::open_tender(&state.db, user.id(), id)
        .await
        .map_err(open_error)?;

    let lots = tenders::lots_of(&state.db, record.id).await?;
    Ok(Json(TenderDto::from_record(record, lots)?))
}

/// Перевод отказов вскрытия в problem+json - общий с модулем комиссии.
pub(crate) fn open_error(err: OpenError) -> ApiError {
    match err {
        OpenError::NotFound => ApiError::NotFound,
        OpenError::Rejected(reason) => ApiError::RuleViolation(reason),
        OpenError::NoCommission => ApiError::RuleViolation(
            "нет действующей комиссии с утвержденным составом (FR-1101)".into(),
        ),
        OpenError::Db(db) => db.into(),
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CommissionMemberDto {
    /// id члена комиссии (для голосов)
    pub member_id: Uuid,
    pub full_name: String,
    /// chairman | deputy | member | reserve
    pub member_role: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MeetingDto {
    pub commission_name: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub scheduled_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub held_at: Option<OffsetDateTime>,
    /// Заседание открыто при кворуме (FR-1102); до этого решений нет
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub opened_at: Option<OffsetDateTime>,
    /// Присутствовало при открытии и сколько требовалось (⅔, п. 12)
    pub quorum_present: Option<i32>,
    pub quorum_required: Option<i32>,
    pub members: Vec<CommissionMemberDto>,
    /// Явка (FR-1102) - ее ведет секретарь до открытия заседания
    pub attendance: Vec<crate::commission::AttendanceRowDto>,
    /// Декларации конфликта интересов (FR-1104, п. 15)
    pub declarations: Vec<crate::commission::DeclarationDto>,
    /// Отводы: отведенный не видит материалы лота и не голосует
    pub recusals: Vec<crate::commission::RecusalDto>,
}

/// Заседание допуска: состав комиссии, явка, кворум, декларации и отводы.
#[utoipa::path(
    get,
    path = "/api/v1/tenders/{id}/meeting",
    tag = "admission",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 200, description = "Заседание допуска", body = MeetingDto),
        (status = 404, description = "Заседание не назначено", body = crate::error::Problem),
    )
)]
pub async fn qualification_meeting(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<MeetingDto>, ApiError> {
    user.require(Action::ApplicationReadAll)?;

    let meeting = admission::qualification_meeting(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let members = admission::members_of(&state.db, meeting.commission_id).await?;
    let attendance = tou_db::commission::attendance(&state.db, meeting.id).await?;
    let (declarations, recusals) = crate::commission::declarations_and_recusals(&state, id).await?;

    Ok(Json(MeetingDto {
        commission_name: meeting.commission_name,
        scheduled_at: meeting.scheduled_at,
        held_at: meeting.held_at,
        opened_at: meeting.opened_at,
        quorum_present: meeting.quorum_present,
        quorum_required: meeting.quorum_required,
        members: members
            .into_iter()
            .map(|m| CommissionMemberDto {
                member_id: m.id,
                full_name: m.full_name,
                member_role: m.member_role,
            })
            .collect(),
        attendance: attendance
            .into_iter()
            .map(crate::commission::attendance_dto)
            .collect(),
        declarations,
        recusals,
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DecideRequest {
    /// Обязателен, если комиссия проголосовала за отклонение:
    /// код из `refdata.rejection_reasons` (п. 52)
    pub rejection_reason: Option<String>,
}

/// Фиксация решения по заявке (FR-502). Вердикт не выбирается секретарем:
/// он вычисляется из голосов членов комиссии (FR-1103, большинство
/// присутствующих, при равенстве - голос председательствующего). Пока
/// проголосовали не все присутствующие, решения нет - 409.
#[utoipa::path(
    post,
    path = "/api/v1/applications/{id}/decide",
    tag = "admission",
    params(("id" = Uuid, Path, description = "Заявка")),
    request_body = DecideRequest,
    responses(
        (status = 200, description = "Решение зафиксировано", body = ApplicationDto),
        (status = 409, description = "Голосование не завершено либо решение невозможно", body = crate::error::Problem),
        (status = 422, description = "Отклонение без основания (п. 52)", body = crate::error::Problem),
    )
)]
pub async fn decide_application(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<DecideRequest>,
) -> Result<Json<ApplicationDto>, ApiError> {
    user.require(Action::AdmissionDecide)?;

    let application = applications::get(&state.db, user.id(), id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let meeting = admission::qualification_meeting(&state.db, application.tender_id)
        .await?
        .ok_or_else(|| {
            ApiError::RuleViolation("заседание комиссии по тендеру не назначено".into())
        })?;

    // Итог голосования - единственный источник вердикта (FR-1103)
    let tally = tou_db::commission::tally(&state.db, meeting.id, id).await?;
    let decision = tally
        .outcome()
        .map_err(|err| ApiError::RuleViolation(err.to_string()))?;

    let (verdict, reason) = match decision {
        tou_domain::commission::Decision::Admitted => ("admitted", None),
        tou_domain::commission::Decision::Rejected => {
            let reason = body
                .rejection_reason
                .as_deref()
                .filter(|r| !r.is_empty())
                .ok_or_else(|| {
                    ApiError::Validation(
                        "отклонение требует основания из перечня п. 52 (INV-052)".into(),
                    )
                })?;
            ("rejected", Some(reason))
        }
    };

    let record = admission::decide(&state.db, user.id(), id, verdict, reason)
        .await
        .map_err(|err| match err {
            DecideError::NotDecidable => ApiError::RuleViolation(
                "решение принимается на открытом заседании после вскрытия, по заявке                  в статусе «подана» (FR-502, FR-1102)"
                    .into(),
            ),
            DecideError::Rejected(reason) => ApiError::RuleViolation(reason),
            DecideError::Db(db) => db.into(),
        })?;

    // Отклоненного участника извещает система, а не протокол (п. 56):
    // допущенный узнает о решении приглашением на торги (FR-504), а
    // отклоненный до сих пор не узнавал ничего. Запись в
    // `core.notifications` - та же доказательная база (FR-1302).
    if verdict == "rejected" {
        let label = reason_label(&state, reason).await;
        let notice = NewNotification {
            user_id: record.participant_id,
            payload: json!({
                "tender_id": record.tender_id,
                "application_id": record.id,
                "reason_code": reason,
                "reason": label,
            }),
        };
        tou_db::notifications::insert(
            &state.db,
            user.id(),
            NotificationKind::ApplicationRejected.as_str(),
            std::slice::from_ref(&notice),
        )
        .await?;
    }

    let files = applications::list_files(&state.db, record.id).await?;
    Ok(Json(ApplicationDto::from_record(record, files)?))
}

/// Формулировка основания п. 52 из справочника: участнику нужен текст
/// правила, а не его код. Справочник недоступен - остается код.
async fn reason_label(state: &AppState, code: Option<&str>) -> Option<String> {
    let code = code?;
    let reasons = tou_db::refdata::rejection_reasons(&state.db).await.ok()?;
    reasons
        .into_iter()
        .find(|reason| reason.code == code)
        .map(|reason| reason.label_ru)
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RejectionReasonDto {
    pub code: String,
    pub label_ru: String,
    pub label_kk: Option<String>,
    pub label_en: Option<String>,
    /// Пункт Правил (п. 52.x)
    pub rule_ref: String,
}

/// Закрытый перечень оснований отклонения (FR-502, п. 52).
#[utoipa::path(
    get,
    path = "/api/v1/refdata/rejection-reasons",
    tag = "admission",
    responses((status = 200, description = "Основания п. 52", body = [RejectionReasonDto]))
)]
pub async fn rejection_reasons(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<RejectionReasonDto>>, ApiError> {
    let reasons = tou_db::refdata::rejection_reasons(&state.db).await?;
    Ok(Json(
        reasons
            .into_iter()
            .map(|r| RejectionReasonDto {
                code: r.code,
                label_ru: r.label_ru,
                label_kk: r.label_kk,
                label_en: r.label_en,
                rule_ref: r.rule_ref,
            })
            .collect(),
    ))
}

/// Только что сформированный протокол - общий ответ для протоколов допуска,
/// итогов, несостоявшегося тендера и победителя № 2. Имя типа уникально в
/// контракте: `ToSchema` берет имя компонента OpenAPI из имени структуры,
/// а одноименный тип в другом модуле молча затирает схему (в `publications`
/// есть свой `ProtocolDto` - с состоянием публикации, а не фактом создания).
#[derive(Debug, Serialize, ToSchema)]
pub struct GeneratedProtocolDto {
    pub id: Uuid,
    pub number: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub generated_at: OffsetDateTime,
}

/// Формирование протокола допуска (FR-503): jsonb-снимок + PDF в RustFS.
/// Повторное формирование отклоняется (протокол - юридический документ).
#[utoipa::path(
    post,
    path = "/api/v1/tenders/{id}/admission-protocol",
    tag = "admission",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 201, description = "Протокол сформирован", body = GeneratedProtocolDto),
        (status = 404, description = "Вскрытие не проводилось", body = crate::error::Problem),
        (status = 409, description = "Протокол уже сформирован", body = crate::error::Problem),
    )
)]
pub async fn generate_admission_protocol(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<GeneratedProtocolDto>), ApiError> {
    user.require(Action::ProtocolGenerate)?;

    let tender = tenders::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let meeting = admission::qualification_meeting(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let members = admission::members_of(&state.db, meeting.commission_id).await?;
    let lots = tenders::lots_of(&state.db, id).await?;
    let records = applications::list_for_tender(&state.db, user.id(), id).await?;
    let votes = admission::votes_of_meeting(&state.db, meeting.id).await?;
    let attendance = tou_db::commission::attendance(&state.db, meeting.id).await?;
    let recusals = tou_db::commission::recusals(&state.db, id).await?;

    let ids: Vec<Uuid> = records.iter().map(|r| r.id).collect();
    let mut files_count: HashMap<Uuid, usize> = HashMap::new();
    for file in applications::files_for(&state.db, &ids).await? {
        *files_count.entry(file.application_id).or_default() += 1;
    }

    let number = format!("Д-{}", short_number(tender.id)); // TODO-ENGINEER: формат номера
    let content = protocol_content(ProtocolInput {
        number: &number,
        tender: &tender,
        meeting: &meeting,
        members: &members,
        attendance: &attendance,
        recusals: &recusals,
        lots: &lots,
        records: &records,
        files_count: &files_count,
        votes: &votes,
    });

    let template_data = content.clone();
    let pdf_bytes = tokio::task::spawn_blocking(move || pdf::render(TEMPLATE, &template_data))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;

    let pdf_key = format!("protocols/{id}/admission.pdf");
    state
        .storage
        .put(
            &ObjectPath::from(pdf_key.as_str()),
            PutPayload::from(pdf_bytes),
        )
        .await
        .map_err(ApiError::internal)?;

    let inserted = admission::insert_protocol(
        &state.db,
        user.id(),
        admission::NewProtocol {
            tender_id: id,
            kind: "admission",
            meeting_id: meeting.id,
            number: &number,
            content: &content,
            pdf_key: &pdf_key,
        },
    )
    .await?;

    match inserted {
        Some(protocol) => Ok((
            StatusCode::CREATED,
            Json(GeneratedProtocolDto {
                id: protocol.id,
                number: protocol.number,
                generated_at: protocol.generated_at,
            }),
        )),
        None => Err(ApiError::RuleViolation(
            "протокол допуска уже сформирован (UNIQUE по тендеру)".into(),
        )),
    }
}

/// Реквизиты протокола допуска (без содержимого) - состояние для UI.
#[utoipa::path(
    get,
    path = "/api/v1/tenders/{id}/admission-protocol",
    tag = "admission",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 200, description = "Протокол сформирован", body = GeneratedProtocolDto),
        (status = 404, description = "Протокол не сформирован", body = crate::error::Problem),
    )
)]
pub async fn admission_protocol_meta(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<GeneratedProtocolDto>, ApiError> {
    user.require(Action::ApplicationReadAll)?;
    let protocol = admission::get_protocol(&state.db, id, "admission")
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(GeneratedProtocolDto {
        id: protocol.id,
        number: protocol.number,
        generated_at: protocol.generated_at,
    }))
}

/// PDF протокола допуска из RustFS (secretary/commission).
#[utoipa::path(
    get,
    path = "/api/v1/tenders/{id}/admission-protocol.pdf",
    tag = "admission",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 200, description = "PDF протокола (п. 55)",
         content_type = "application/pdf", body = Vec<u8>),
        (status = 404, description = "Протокол не сформирован", body = crate::error::Problem),
    )
)]
pub async fn admission_protocol_pdf(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    user.require(Action::ApplicationReadAll)?;

    let protocol = admission::get_protocol(&state.db, id, "admission")
        .await?
        .ok_or(ApiError::NotFound)?;
    let pdf_key = protocol.pdf_key.ok_or(ApiError::NotFound)?;

    let object = state
        .storage
        .get(&ObjectPath::from(pdf_key.as_str()))
        .await
        .map_err(ApiError::internal)?;
    let bytes = object.bytes().await.map_err(ApiError::internal)?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/pdf".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("inline; filename=\"tender-{id}-admission-protocol.pdf\""),
            ),
        ],
        bytes.to_vec(),
    )
        .into_response())
}

#[derive(Debug, Serialize, ToSchema)]
pub struct NotifyAdmittedResponse {
    /// Число уведомленных заявок (по одному уведомлению на допущенную заявку)
    pub notified: usize,
    /// Назначенные дата/время торгов (п. 59: 3-й рабочий день после
    /// уведомления, если организатор не назначил заранее)
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub trading_at: OffsetDateTime,
}

/// Уведомление допущенных (FR-504): дата/время/место второго этапа и
/// стартовая ставка = максимум первоначальных предложений допущенных по лоту
/// (INV-062, п. 57–59, 62). Возможно после протокола допуска, однократно;
/// факт и время каждого уведомления фиксирует audit-триггер (FR-1302).
#[utoipa::path(
    post,
    path = "/api/v1/tenders/{id}/notify-admitted",
    tag = "admission",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 200, description = "Уведомления разосланы", body = NotifyAdmittedResponse),
        (status = 404, description = "Тендер не найден", body = crate::error::Problem),
        (status = 409, description = "Нет протокола, нет допущенных или уже разосланы",
         body = crate::error::Problem),
    )
)]
pub async fn notify_admitted(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<NotifyAdmittedResponse>, ApiError> {
    user.require(Action::ProtocolGenerate)?;

    let use_case = NotifyAdmitted::new(PgNotifyAdmittedStore::new(&state.db), &state.notifier);
    let result = use_case
        .execute(user.id(), id)
        .await
        .map_err(map_notify_admitted_error)?;

    Ok(Json(NotifyAdmittedResponse {
        notified: result.notified,
        trading_at: result.trading_at,
    }))
}

fn map_notify_admitted_error(error: NotifyAdmittedError) -> ApiError {
    match error {
        NotifyAdmittedError::TenderNotFound => ApiError::NotFound,
        NotifyAdmittedError::AdmissionProtocolMissing => ApiError::RuleViolation(
            "уведомление допущенных возможно после протокола допуска (FR-504)".into(),
        ),
        NotifyAdmittedError::AlreadyNotified => ApiError::RuleViolation(
            "допущенные по тендеру уже уведомлены (FR-504 - однократно)".into(),
        ),
        NotifyAdmittedError::NoAdmittedApplications => {
            ApiError::RuleViolation("по тендеру нет допущенных заявок - уведомлять некого".into())
        }
        NotifyAdmittedError::Infrastructure(source) => ApiError::Internal(source),
    }
}

pub(crate) fn short_number(id: Uuid) -> String {
    id.simple().to_string()[..8].to_uppercase()
}

/// NFR-03: отображение времени - Asia/Almaty (UTC+5).
pub(crate) fn format_almaty(value: Option<OffsetDateTime>) -> String {
    let Some(ts) = value else {
        return "-".to_owned();
    };
    let almaty = time::UtcOffset::from_hms(5, 0, 0).unwrap_or(time::UtcOffset::UTC);
    ts.to_offset(almaty)
        .format(format_description!("[day].[month].[year] [hour]:[minute]"))
        .unwrap_or_else(|_| "-".to_owned())
}

pub(crate) fn member_role_ru(role: &str) -> &'static str {
    match role {
        "chairman" => "председатель",
        "deputy" => "заместитель председателя",
        "reserve" => "резервный член",
        _ => "член комиссии",
    }
}

/// Снимок протокола (jsonb `core.protocols.content`): он же - данные шаблона.
#[allow(clippy::too_many_arguments)] // ALLOWED-BY-ENGINEER:T9 снимок собирается из 8 источников БД
/// Данные печатной формы протокола допуска (п. 55) одним аргументом:
/// список полей длинный, а порядок позиционных параметров легко перепутать.
struct ProtocolInput<'a> {
    number: &'a str,
    tender: &'a tou_db::tenders::TenderRecord,
    meeting: &'a tou_db::admission::MeetingRecord,
    members: &'a [tou_db::admission::MemberRecord],
    attendance: &'a [tou_db::commission::AttendanceRow],
    recusals: &'a [tou_db::commission::RecusalRow],
    lots: &'a [tou_db::tenders::LotRecord],
    records: &'a [tou_db::applications::ApplicationRecord],
    files_count: &'a HashMap<Uuid, usize>,
    votes: &'a [tou_db::admission::VoteRecord],
}

fn protocol_content(input: ProtocolInput<'_>) -> serde_json::Value {
    let ProtocolInput {
        number,
        tender,
        meeting,
        members,
        attendance,
        recusals,
        lots,
        records,
        files_count,
        votes,
    } = input;

    // Явка (п. 12): в протоколе фиксируется, кто присутствовал и кто вел
    let present_of: HashMap<Uuid, &tou_db::commission::AttendanceRow> =
        attendance.iter().map(|row| (row.member_id, row)).collect();

    let lot_label: HashMap<Uuid, String> = lots
        .iter()
        .map(|lot| (lot.id, format!("№{} - {}", lot.seq, lot.purpose)))
        .collect();
    let app_label: HashMap<Uuid, String> = records
        .iter()
        .map(|a| {
            let applicant = a.applicant_details.expose()["name"].as_str().unwrap_or("-");
            (a.id, format!("{} ({})", applicant, short_number(a.id)))
        })
        .collect();

    json!({
        "number": number,
        "tender_number": short_number(tender.id),
        "tender_title": tender.title,
        "commission": meeting.commission_name,
        "scheduled_at": format_almaty(Some(meeting.scheduled_at)),
        "held_at": format_almaty(meeting.held_at),
        // TODO-ENGINEER: место заседания - источник в контуре 1 отсутствует
        "place": "уточняется юридической службой",
        // Кворум ⅔ с председателем или заместителем (FR-1102, п. 12)
        "quorum": format!(
            "{} из {} (требуется {})",
            meeting.quorum_present.unwrap_or(0),
            members.iter().filter(|m| m.member_role != "reserve").count(),
            meeting.quorum_required.unwrap_or(0),
        ),
        "members": members.iter().map(|m| json!({
            "name": m.full_name,
            "role": member_role_ru(&m.member_role),
            "attendance": match present_of.get(&m.id) {
                Some(row) if row.chairing => "председательствует",
                Some(row) if row.present => "присутствует",
                Some(_) => "отсутствует",
                None => "-",
            },
        })).collect::<Vec<_>>(),
        // Отводы по конфликту интересов (FR-1104, п. 15)
        "recusals": recusals.iter().map(|r| json!({
            "member": r.full_name,
            "scope": r.lot_id
                .and_then(|lot| lot_label.get(&lot).cloned())
                .unwrap_or_else(|| "весь тендер".to_owned()),
            "reason": r.reason,
            "replacement": r.replacement_name.clone().unwrap_or_else(|| "-".to_owned()),
        })).collect::<Vec<_>>(),
        "applications": records.iter().map(|a| {
            let decision = match a.status.as_str() {
                "admitted" => "допущена".to_owned(),
                "rejected" => format!(
                    "отклонена ({})",
                    a.rejection_reason.as_deref().unwrap_or("-")
                ),
                "withdrawn" => "отозвана участником".to_owned(),
                other => format!("без решения ({other})"),
            };
            json!({
                "applicant": a.applicant_details.expose()["name"].as_str().unwrap_or("-"),
                "lot": lot_label.get(&a.lot_id).cloned().unwrap_or_else(|| "-".into()),
                "price": a.price_amount.map(crate::announcement::format_decimal_ru)
                    .unwrap_or_else(|| "-".into()),
                "files": files_count.get(&a.id).copied().unwrap_or(0),
                "decision": decision,
            })
        }).collect::<Vec<_>>(),
        "votes": votes.iter().map(|v| json!({
            "application": app_label.get(&v.application_id).cloned().unwrap_or_else(|| "-".into()),
            "member": v.member_name,
            "value": if v.value == "for" { "за" } else { "против" },
            // Особое мнение прикладывается к протоколу (п. 13–14, 55.8)
            "dissent": v.dissent.clone().unwrap_or_default(),
        })).collect::<Vec<_>>(),
        // NFR-03: дата в PDF - время заседания по часам БД; момент генерации
        // хранит generated_at записи протокола
        "generated_at": format_almaty(meeting.held_at),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Шаблон протокола компилируется в PDF на снимке с решениями и голосами
    /// (критерий Т9: PDF содержит поля п. 55).
    #[test]
    fn admission_protocol_renders_pdf() {
        let data = json!({
            "number": "Д-0TEST",
            "tender_number": "0TEST",
            "tender_title": "Тестовый тендер - спецсимволы *#_$@«»",
            "commission": "Тендерная комиссия (тест)",
            "quorum": "5 из 7 (требуется 5)",
            "scheduled_at": "20.08.2026 10:00",
            "held_at": "20.08.2026 10:05",
            "place": "уточняется юридической службой",
            "members": [
                {"name": "Председатель П.П.", "role": "председатель",
                 "attendance": "председательствует"},
                {"name": "Член М.М.", "role": "член комиссии", "attendance": "присутствует"},
                {"name": "Отведенный О.О.", "role": "член комиссии", "attendance": "отсутствует"},
            ],
            "recusals": [
                {"member": "Отведенный О.О.", "scope": "№1 - офис",
                 "reason": "аффилированность с участником", "replacement": "Резервный Р.Р."},
            ],
            "applications": [
                {"applicant": "ТОО «Тест»", "lot": "№1 - офис", "price": "40\u{202f}000,00",
                 "files": 2, "decision": "допущена"},
                {"applicant": "ИП Иванов", "lot": "№1 - офис", "price": "39\u{202f}000,00",
                 "files": 0, "decision": "отклонена (fee_not_paid)"},
            ],
            "votes": [
                {"application": "ТОО «Тест» (0TEST)", "member": "Председатель П.П.",
                 "value": "за", "dissent": ""},
                {"application": "ТОО «Тест» (0TEST)", "member": "Член М.М.",
                 "value": "против", "dissent": "сведения о квалификации неполны"},
            ],
            "generated_at": "20.08.2026 10:05",
        });

        let bytes = pdf::render(TEMPLATE, &data).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 1_000);
    }
}
