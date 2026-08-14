//! Несостоявшийся тендер (М8, FR-801–802, п. 81–83).
//!
//! Основание не выбирается пользователем: система выводит его из фактов
//! процесса и показывает заранее, а признание лишь фиксирует наступившее.
//! Протокол - тот же прием, что и у допуска и итогов: jsonb-снимок
//! и Typst-PDF в RustFS, однократно.

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use object_store::ObjectStoreExt as _;
use object_store::PutPayload;
use object_store::path::Path as ObjectPath;
use serde::Serialize;
use serde_json::json;
use tou_db::failure::{self, FailureError};
use tou_db::results::MeetingError;
use tou_db::{admission, applications, results, tenders};
use tou_domain::policy::{Action, Compound};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::admission::{GeneratedProtocolDto, format_almaty, member_role_ru, short_number};
use crate::announcement::format_decimal_ru;
use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::pdf;
use crate::request::{Json, Path};
use crate::state::AppState;
use tou_domain::rule::RuleViolation;

const TEMPLATE: &str = include_str!("templates/failed.typ");

/// Срок оформления протокола о несостоявшемся - 3 рабочих дня (п. 82).
const PROTOCOL_BUSINESS_DAYS: i32 = 3;

fn failure_error(err: FailureError) -> ApiError {
    match err {
        FailureError::NotFound => ApiError::NotFound,
        FailureError::Rejected(reason) => ApiError::RuleViolation(reason),
        FailureError::Db(db) => db.into(),
    }
}

/// Состояние тендера глазами п. 81–83: наступило ли основание и что из него
/// следует. Показывается секретарю до признания - «не состоялся» не должно
/// быть неожиданностью.
#[derive(Debug, Serialize, ToSchema)]
pub struct FailureStateDto {
    /// Поданные и не отозванные заявки
    pub applications: usize,
    pub admitted: usize,
    /// Срок приема заявок истек (до него о числе заявок судить рано)
    pub deadline_passed: bool,
    /// Вскрытие состоялось: до него о допуске судить рано
    pub opened: bool,
    /// Код основания п. 81, если оно наступило
    pub ground: Option<String>,
    pub ground_rule_ref: Option<String>,
    /// `repeat` | `single_source` | `board_referral` (п. 82–83)
    pub consequence: Option<String>,
    /// Сколько несостоявшихся уже было в цепочке повторов
    pub previous_failures: usize,
    pub failed: bool,
}

/// Основание и следствие по фактам тендера (FR-801–802).
#[utoipa::path(
    get,
    path = "/api/v1/tenders/{id}/failure",
    tag = "failure",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 200, description = "Состояние по п. 81–83", body = FailureStateDto),
        (status = 404, description = "Тендер не найден", body = crate::error::Problem),
    )
)]
pub async fn failure_state(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<FailureStateDto>, ApiError> {
    // Основание и следствие видят и те, кто решает (секретарь, комиссия),
    // и организатор: повторный тендер объявляет он (FR-802, п. 82)
    user.require_any(Compound::TENDER_PROCESS_READ)?;

    let found = failure::state(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(FailureStateDto {
        applications: found.facts.applications,
        admitted: found.facts.admitted,
        deadline_passed: found.facts.deadline_passed,
        opened: found.facts.opened,
        ground: found.ground.map(|ground| ground.as_str().to_owned()),
        ground_rule_ref: found.ground.map(|ground| ground.rule_ref().to_owned()),
        consequence: found
            .consequence
            .map(|consequence| consequence.as_str().to_owned()),
        previous_failures: found.previous_failures,
        failed: found.failed,
    }))
}

/// Признание тендера несостоявшимся (FR-801, п. 81). Основание выводится из
/// фактов; если оно не наступило - отказ.
#[utoipa::path(
    post,
    path = "/api/v1/tenders/{id}/declare-failed",
    tag = "failure",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 200, description = "Тендер признан несостоявшимся", body = FailureStateDto),
        (status = 404, description = "Тендер не найден", body = crate::error::Problem),
        (status = 409, description = "Основание п. 81 не наступило", body = crate::error::Problem),
    )
)]
pub async fn declare_failed(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<FailureStateDto>, ApiError> {
    user.require(Action::AdmissionDecide)?;

    failure::declare_failed(&state.db, user.id(), id)
        .await
        .map_err(failure_error)?;

    failure_state(user, State(state), Path(id)).await
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RepeatTenderDto {
    /// Черновик повторного тендера (п. 82)
    pub tender_id: Uuid,
}

/// Повторный тендер (FR-802, п. 82): черновик с теми же лотами и ссылкой
/// на несостоявшийся. После двух несостоявшихся подряд отклоняется -
/// вопрос идет Правлению (п. 83).
#[utoipa::path(
    post,
    path = "/api/v1/tenders/{id}/repeat",
    tag = "failure",
    params(("id" = Uuid, Path, description = "Несостоявшийся тендер")),
    responses(
        (status = 201, description = "Черновик повторного тендера создан", body = RepeatTenderDto),
        (status = 404, description = "Тендер не найден", body = crate::error::Problem),
        (status = 409, description = "Повтор невозможен (п. 82–83)", body = crate::error::Problem),
    )
)]
pub async fn repeat_tender(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<RepeatTenderDto>), ApiError> {
    user.require(Action::TenderManage)?;

    let tender_id = failure::repeat_tender(&state.db, user.id(), id)
        .await
        .map_err(failure_error)?;

    Ok((StatusCode::CREATED, Json(RepeatTenderDto { tender_id })))
}

/// Протокол о несостоявшемся тендере (FR-802, п. 82): основание, состав
/// комиссии, поданные заявки и следствие. Однократно, как и прочие протоколы.
#[utoipa::path(
    post,
    path = "/api/v1/tenders/{id}/failed-protocol",
    tag = "failure",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 201, description = "Протокол сформирован", body = GeneratedProtocolDto),
        (status = 404, description = "Тендер не найден", body = crate::error::Problem),
        (status = 409, description = "Тендер не признан несостоявшимся либо протокол уже есть",
         body = crate::error::Problem),
    )
)]
pub async fn generate_failed_protocol(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<GeneratedProtocolDto>), ApiError> {
    user.require(Action::ProtocolGenerate)?;

    let tender = tenders::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let found = failure::state(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if !found.failed {
        return Err(ApiError::rule(
            RuleViolation::TenderFailureGround,
            "протокол оформляется после признания тендера несостоявшимся (FR-802)",
        ));
    }
    let ground = found.ground.ok_or_else(|| {
        ApiError::rule(
            RuleViolation::TenderFailureGround,
            "основание п. 81 не зафиксировано",
        )
    })?;

    let meeting = results::results_meeting(&state.db, user.id(), id)
        .await
        .map_err(|err| match err {
            MeetingError::NoCommission => ApiError::rule(
                RuleViolation::CommissionComposition,
                "нет действующей комиссии с утвержденным составом (FR-1101)",
            ),
            MeetingError::Db(db) => db.into(),
        })?;
    let members = admission::members_of(&state.db, meeting.commission_id).await?;
    let records = applications::list_for_tender(&state.db, user.id(), id).await?;

    let grounds = failure::grounds(&state.db).await?;
    let ground_label = grounds
        .iter()
        .find(|row| row.code == ground.as_str())
        .map(|row| row.label_ru.clone())
        .unwrap_or_else(|| ground.as_str().to_owned());

    let deadline = results::protocol_deadline(
        &state.db,
        meeting.held_at.unwrap_or(meeting.scheduled_at),
        PROTOCOL_BUSINESS_DAYS,
    )
    .await?;

    let number = format!("Н-{}", short_number(tender.id)); // TODO-ENGINEER: формат номера
    let content = json!({
        "number": number,
        "tender_number": short_number(tender.id),
        "tender_title": tender.title,
        "commission": meeting.commission_name,
        "held_at": format_almaty(meeting.held_at.or(Some(meeting.scheduled_at))),
        "place": "уточняется юридической службой", // TODO-ENGINEER: место заседания
        "deadline": format_almaty(Some(deadline)),
        "ground_rule": ground.rule_ref(),
        "ground_label": ground_label,
        "applications": found.facts.applications,
        "admitted": found.facts.admitted,
        "members": members
            .iter()
            .map(|member| json!({
                "name": member.full_name,
                "role": member_role_ru(&member.member_role),
            }))
            .collect::<Vec<_>>(),
        "applications_list": records
            .iter()
            .map(|record| json!({
                "applicant": record
                    .applicant_details
                    .expose()
                    .get("name")
                    .and_then(|value| value.as_str())
                    .unwrap_or("-"),
                "price": record
                    .price_amount
                    .map(format_decimal_ru)
                    .unwrap_or_else(|| "-".to_owned()),
                "decision": decision_ru(&record.status, record.rejection_reason.as_deref()),
            }))
            .collect::<Vec<_>>(),
        "consequence_text": consequence_ru(found.consequence),
        "generated_at": format_almaty(meeting.held_at.or(Some(meeting.scheduled_at))),
    });

    let template_data = content.clone();
    let pdf_bytes = tokio::task::spawn_blocking(move || pdf::render(TEMPLATE, &template_data))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;

    let pdf_key = format!("protocols/{id}/failed.pdf");
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
            kind: "failed",
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
        None => Err(ApiError::rule(
            RuleViolation::DuplicateRecord,
            "протокол о несостоявшемся уже сформирован (UNIQUE по тендеру)",
        )),
    }
}

/// PDF протокола о несостоявшемся из RustFS.
#[utoipa::path(
    get,
    path = "/api/v1/tenders/{id}/failed-protocol.pdf",
    tag = "failure",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 200, description = "PDF протокола", content_type = "application/pdf"),
        (status = 404, description = "Протокол не сформирован", body = crate::error::Problem),
    )
)]
pub async fn failed_protocol_pdf(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    user.require(Action::TenderRead)?;

    let protocol = admission::get_protocol(&state.db, id, "failed")
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
                format!("inline; filename=\"tender-{id}-failed-protocol.pdf\""),
            ),
        ],
        bytes,
    )
        .into_response())
}

fn decision_ru(status: &str, reason: Option<&str>) -> String {
    match status {
        "admitted" => "допущена".to_owned(),
        "rejected" => match reason {
            Some(code) => format!("отклонена ({code})"),
            None => "отклонена".to_owned(),
        },
        "withdrawn" => "отозвана".to_owned(),
        _ => "решение не принято".to_owned(),
    }
}

/// Формулировка следствия для протокола (п. 82–83).
/// TODO-ENGINEER: тексты сверяются с утвержденной формой протокола.
fn consequence_ru(consequence: Option<tou_domain::failure::Consequence>) -> &'static str {
    use tou_domain::failure::Consequence;
    match consequence {
        Some(Consequence::SingleSource) => {
            "Подана единственная соответствующая требованиям заявка: комиссия вправе принять \
             решение о заключении договора из одного источника (п. 82)."
        }
        Some(Consequence::BoardReferral) => {
            "Тендер признан несостоявшимся повторно: вопрос передается на рассмотрение \
             Правления (п. 83)."
        }
        _ => "Объявляется повторный тендер (п. 82).",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Шаблон протокола компилируется в PDF на снимке без заявок
    /// (несостоявшийся тендер - типичный случай «заявок нет»).
    #[test]
    fn failed_protocol_renders_pdf() {
        let data = json!({
            "number": "Н-0TEST",
            "tender_number": "0TEST",
            "tender_title": "Тестовый тендер - спецсимволы *#_$@«»",
            "commission": "Тендерная комиссия (тест)",
            "held_at": "20.08.2026 10:00",
            "place": "уточняется юридической службой",
            "deadline": "25.08.2026 10:00",
            "ground_rule": "п. 81.1",
            "ground_label": "Не подано ни одной заявки",
            "applications": 0,
            "admitted": 0,
            "members": [{"name": "Председатель П.П.", "role": "председатель"}],
            "applications_list": [],
            "consequence_text": "Объявляется повторный тендер (п. 82).",
            "generated_at": "20.08.2026 10:05",
        });

        let bytes = pdf::render(TEMPLATE, &data).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 1_000);
    }

    #[test]
    fn consequence_text_matches_the_rules() {
        use tou_domain::failure::Consequence;
        assert!(consequence_ru(Some(Consequence::BoardReferral)).contains("Правления"));
        assert!(consequence_ru(Some(Consequence::SingleSource)).contains("одного источника"));
        assert!(consequence_ru(Some(Consequence::Repeat)).contains("повторный"));
        assert!(consequence_ru(None).contains("повторный"));
    }
}
