//! Уклонение победителя и участника № 2 (М9, FR-903, FR-505, п. 116–120).
//!
//! Признание уклонения - фиксация факта, а не решение: домен проверяет,
//! возможно ли оно, и выводит следствие, БД удерживает взнос и прекращает
//! договор. Дальше идут сроки п. 117–118: протокол о победителе № 2 за пять
//! рабочих дней и уведомление № 2 не позднее следующего рабочего дня -
//! оно уходит вместе с протоколом, чтобы уведомление не разошлось с фактом.

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use object_store::ObjectStoreExt as _;
use object_store::PutPayload;
use object_store::path::Path as ObjectPath;
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;
use tou_db::evasion::{self, EvasionError};
use tou_db::results::MeetingError;
use tou_db::{admission, results, tenders};
use tou_domain::evasion::EvasionGround;
use tou_domain::notification::NotificationKind;
use tou_domain::obligation::ObligationAction;
use tou_domain::policy::Action;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::admission::{GeneratedProtocolDto, format_almaty, member_role_ru, short_number};
use crate::announcement::format_decimal_ru;
use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::pdf;
use crate::request::{Json, Path};
use crate::state::AppState;

const TEMPLATE: &str = include_str!("templates/winner2.typ");

/// Срок оформления протокола о победителе № 2 - 5 рабочих дней (п. 117).
const PROTOCOL_BUSINESS_DAYS: i32 = 5;

fn evasion_error(err: EvasionError) -> ApiError {
    match err {
        EvasionError::NotFound => ApiError::NotFound,
        EvasionError::Rejected(reason) => ApiError::RuleViolation(reason),
        EvasionError::Db(db) => db.into(),
    }
}

/// Зафиксированное уклонение (FR-903).
#[derive(Debug, Serialize, ToSchema)]
pub struct EvasionDto {
    pub id: Uuid,
    pub contract_id: Uuid,
    pub tender_id: Option<Uuid>,
    pub lot_seq: Option<i32>,
    pub user_name: String,
    /// `winner` | `runner_up` (п. 74)
    pub place: String,
    pub place_title_ru: String,
    /// Код основания п. 116 из закрытого перечня
    pub ground: String,
    pub ground_label: String,
    pub ground_rule_ref: String,
    pub note: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub declared_at: OffsetDateTime,
}

fn evasion_dto(record: evasion::EvasionRecord) -> EvasionDto {
    EvasionDto {
        id: record.id,
        contract_id: record.contract_id,
        tender_id: record.tender_id,
        lot_seq: record.lot_seq,
        user_name: record.user_name,
        place: record.place.as_str().to_owned(),
        place_title_ru: record.place.title_ru().to_owned(),
        ground: record.ground.as_str().to_owned(),
        ground_label: record.ground_label,
        ground_rule_ref: record.ground.rule_ref().to_owned(),
        note: record.note,
        declared_at: record.declared_at,
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct DeclareEvasionRequest {
    /// Основание п. 116: `signing_deadline_missed` | `documents_deadline_missed` | `refused`
    pub ground: String,
    pub note: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeclaredEvasionDto {
    #[serde(flatten)]
    pub evasion: EvasionDto,
    /// `offer_to_runner_up` - договор идет участнику № 2 (п. 117);
    /// `tender_failed` - второго места нет либо уклонился и он (п. 81.4)
    pub consequence: String,
}

/// Признание уклонения от подписания договора (FR-903, п. 116).
/// Взнос удерживается, договор прекращается, сроки п. 117–118 ставятся
/// тем же событием.
#[utoipa::path(
    post,
    path = "/api/v1/contracts/{id}/evasion",
    tag = "evasion",
    params(("id" = Uuid, Path, description = "Договор")),
    request_body = DeclareEvasionRequest,
    responses(
        (status = 201, description = "Уклонение зафиксировано", body = DeclaredEvasionDto),
        (status = 404, description = "Договор не найден", body = crate::error::Problem),
        (status = 409, description = "Договор подписан либо уклонение уже зафиксировано",
         body = crate::error::Problem),
        (status = 422, description = "Неизвестное основание", body = crate::error::Problem),
    )
)]
pub async fn declare_evasion(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<DeclareEvasionRequest>,
) -> Result<(StatusCode, Json<DeclaredEvasionDto>), ApiError> {
    user.require(Action::TenderManage)?;

    let ground: EvasionGround = body.ground.parse().map_err(|_| {
        ApiError::Validation(format!("неизвестное основание уклонения: {}", body.ground))
    })?;

    let (record, consequence) = evasion::declare(
        &state.db,
        user.id(),
        id,
        ground,
        body.note.as_deref().filter(|note| !note.trim().is_empty()),
    )
    .await
    .map_err(evasion_error)?;

    Ok((
        StatusCode::CREATED,
        Json(DeclaredEvasionDto {
            evasion: evasion_dto(record),
            consequence: consequence.as_str().to_owned(),
        }),
    ))
}

/// Уклонения по тендеру (FR-903): что произошло и с кем.
#[utoipa::path(
    get,
    path = "/api/v1/tenders/{id}/evasions",
    tag = "evasion",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses((status = 200, description = "Уклонения тендера", body = [EvasionDto]))
)]
pub async fn tender_evasions(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<EvasionDto>>, ApiError> {
    // Факт уклонения - репутационные сведения об участнике, а не
    // публичная часть тендера (п. 116, 120)
    user.require(Action::ContractRead)?;

    let records = evasion::list_for_tender(&state.db, id).await?;
    Ok(Json(records.into_iter().map(evasion_dto).collect()))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EvaderDto {
    pub user_id: Uuid,
    pub full_name: String,
    /// Сколько раз уклонялся
    pub evasions: i32,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub last_declared_at: Option<OffsetDateTime>,
    pub last_ground: Option<String>,
    pub last_tender_id: Option<Uuid>,
}

/// Реестр уклонистов (FR-505, п. 52.4, 120): их заявки в будущих тендерах
/// отклоняются автоматически - реестр показывает, кого это касается.
#[utoipa::path(
    get,
    path = "/api/v1/evaders",
    tag = "evasion",
    responses((status = 200, description = "Реестр уклонистов", body = [EvaderDto]))
)]
pub async fn evader_registry(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<EvaderDto>>, ApiError> {
    user.require(Action::ApplicationReadAll)?;

    let rows = evasion::registry(&state.db).await?;
    Ok(Json(
        rows.into_iter()
            .map(|row| EvaderDto {
                user_id: row.user_id,
                full_name: row.full_name,
                evasions: row.evasions,
                last_declared_at: row.last_declared_at,
                last_ground: row.last_ground,
                last_tender_id: row.last_tender_id,
            })
            .collect(),
    ))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EvasionGroundDto {
    pub code: String,
    pub label_ru: String,
    pub label_kk: Option<String>,
    pub label_en: Option<String>,
    pub rule_ref: String,
}

/// Закрытый перечень оснований уклонения (п. 116).
#[utoipa::path(
    get,
    path = "/api/v1/evasion-grounds",
    tag = "evasion",
    responses((status = 200, description = "Основания п. 116", body = [EvasionGroundDto]))
)]
pub async fn evasion_grounds(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<EvasionGroundDto>>, ApiError> {
    user.require(Action::TenderRead)?;

    let rows = evasion::grounds(&state.db).await?;
    Ok(Json(
        rows.into_iter()
            .map(|row| EvasionGroundDto {
                code: row.code,
                label_ru: row.label_ru,
                label_kk: row.label_kk,
                label_en: row.label_en,
                rule_ref: row.rule_ref,
            })
            .collect(),
    ))
}

/// Протокол о победителе № 2 (FR-903, п. 117) и уведомление участника № 2
/// (п. 118). Уведомление уходит вместе с протоколом: оно и есть извещение
/// о том, что право на договор перешло, - расходиться им нельзя.
#[utoipa::path(
    post,
    path = "/api/v1/tenders/{id}/winner2-protocol",
    tag = "evasion",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 201, description = "Протокол сформирован", body = GeneratedProtocolDto),
        (status = 404, description = "Тендер не найден", body = crate::error::Problem),
        (status = 409, description = "Уклонения нет либо протокол уже сформирован",
         body = crate::error::Problem),
    )
)]
pub async fn generate_winner2_protocol(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<GeneratedProtocolDto>), ApiError> {
    user.require(Action::ProtocolGenerate)?;

    let tender = tenders::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let evasions = evasion::list_for_tender(&state.db, id).await?;
    if evasions.is_empty() {
        return Err(ApiError::RuleViolation(
            "протокол о победителе № 2 оформляется после признания уклонения (п. 116–117)".into(),
        ));
    }

    let meeting = results::results_meeting(&state.db, user.id(), id)
        .await
        .map_err(|err| match err {
            MeetingError::NoCommission => ApiError::RuleViolation(
                "нет действующей комиссии с утвержденным составом (FR-1101)".into(),
            ),
            MeetingError::Db(db) => db.into(),
        })?;
    let members = admission::members_of(&state.db, meeting.commission_id).await?;

    // Лоты, где право на договор перешло к участнику № 2 (п. 117)
    let lot_results = results::lot_results(&state.db, id).await?;
    let mut lots = Vec::new();
    let mut recipients = Vec::new();
    for lot in &lot_results {
        let evaded_winner = evasions
            .iter()
            .any(|e| e.lot_id == Some(lot.lot_id) && e.place.as_str() == "winner");
        if !evaded_winner {
            continue;
        }
        let Some(runner_up) = evasion::runner_up_of_lot(&state.db, lot.lot_id).await? else {
            continue;
        };
        // Уклонившийся № 2 договор уже не получает (п. 81.4)
        if evasions
            .iter()
            .any(|e| e.lot_id == Some(lot.lot_id) && e.place.as_str() == "runner_up")
        {
            continue;
        }

        lots.push(json!({
            "seq": lot.seq,
            "object": lot.object_name.clone(),
            "runner_up": runner_up.participant_name.clone(),
            "amount": format_decimal_ru(runner_up.amount),
        }));
        recipients.push((lot.lot_id, lot.seq, runner_up));
    }

    let deadline = results::protocol_deadline(
        &state.db,
        meeting.held_at.unwrap_or(meeting.scheduled_at),
        PROTOCOL_BUSINESS_DAYS,
    )
    .await?;

    let number = format!("У-{}", short_number(tender.id)); // TODO-ENGINEER: формат номера
    // Отметка «сформировано» - по часам сервера (`core.now()`, ADR-0005)
    let generated_at = tou_db::refdata::now(&state.db).await?;

    let content = json!({
        "number": number,
        "tender_number": short_number(tender.id),
        "tender_title": tender.title,
        "commission": meeting.commission_name,
        "held_at": format_almaty(meeting.held_at.or(Some(meeting.scheduled_at))),
        "place": "уточняется юридической службой", // TODO-ENGINEER: место заседания
        "deadline": format_almaty(Some(deadline)),
        "evasions": evasions
            .iter()
            .map(|record| json!({
                "name": record.user_name,
                "place": record.place.title_ru(),
                "ground": record.ground_label,
                "declared_at": format_almaty(Some(record.declared_at)),
            }))
            .collect::<Vec<_>>(),
        "lots": lots,
        "members": members
            .iter()
            .map(|member| json!({
                "name": member.full_name,
                "role": member_role_ru(&member.member_role),
            }))
            .collect::<Vec<_>>(),
        "generated_at": format_almaty(Some(generated_at)),
    });

    let template_data = content.clone();
    let pdf_bytes = tokio::task::spawn_blocking(move || pdf::render(TEMPLATE, &template_data))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;

    let pdf_key = format!("protocols/{id}/winner2.pdf");
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
            kind: "winner2",
            meeting_id: meeting.id,
            number: &number,
            content: &content,
            pdf_key: &pdf_key,
        },
    )
    .await?;

    let protocol = inserted.ok_or_else(|| {
        ApiError::RuleViolation(
            "протокол о победителе № 2 уже сформирован (UNIQUE по тендеру)".into(),
        )
    })?;

    // Уведомление участника № 2 - не позднее следующего рабочего дня (п. 118)
    let notices: Vec<tou_db::notifications::NewNotification> = recipients
        .iter()
        .map(
            |(lot_id, seq, runner_up)| tou_db::notifications::NewNotification {
                user_id: runner_up.participant_id,
                payload: json!({
                    "tender_id": id,
                    "tender_title": tender.title,
                    "lot_id": lot_id,
                    "lot": format!("№{seq}"),
                    "amount": runner_up.amount.to_string(),
                    "protocol_number": number,
                    "rule_ref": "п. 117–118",
                }),
            },
        )
        .collect();
    if !notices.is_empty() {
        tou_db::notifications::insert(
            &state.db,
            user.id(),
            NotificationKind::RunnerUpOffer.as_str(),
            &notices,
        )
        .await?;
    }

    // Сроки п. 117–118 закрыты фактами протокола и уведомления (FR-1702)
    tou_db::obligations::complete_tender(
        &state.db,
        user.id(),
        &[
            ObligationAction::Winner2Protocol,
            ObligationAction::NotifyRunnerUp,
        ],
        id,
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(GeneratedProtocolDto {
            id: protocol.id,
            number: protocol.number,
            generated_at: protocol.generated_at,
        }),
    ))
}

/// PDF протокола о победителе № 2 из RustFS.
#[utoipa::path(
    get,
    path = "/api/v1/tenders/{id}/winner2-protocol.pdf",
    tag = "evasion",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 200, description = "PDF протокола", content_type = "application/pdf"),
        (status = 404, description = "Протокол не сформирован", body = crate::error::Problem),
    )
)]
pub async fn winner2_protocol_pdf(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    user.require(Action::TenderRead)?;

    let protocol = admission::get_protocol(&state.db, id, "winner2")
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
                format!("inline; filename=\"tender-{id}-winner2-protocol.pdf\""),
            ),
        ],
        bytes,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Печатная форма протокола компилируется в PDF на снимке уклонения.
    #[test]
    fn winner2_protocol_renders_pdf() {
        let data = json!({
            "number": "У-0TEST",
            "tender_number": "0TEST",
            "tender_title": "Тестовый тендер - спецсимволы *#_$@«»",
            "commission": "Тендерная комиссия (тест)",
            "held_at": "20.08.2026 10:00",
            "place": "уточняется юридической службой",
            "deadline": "27.08.2026 18:00",
            "evasions": [{
                "name": "ТОО «Уклонист»",
                "place": "победитель",
                "ground": "Подписанный договор не возвращен в установленный срок",
                "declared_at": "20.08.2026 09:30",
            }],
            "lots": [{
                "seq": 1,
                "object": "Помещение 42 м²",
                "runner_up": "ИП Второй",
                "amount": "75 000,00",
            }],
            "members": [{"name": "Председатель П.П.", "role": "председатель"}],
            "generated_at": "20.08.2026 10:05",
        });

        let bytes = pdf::render(TEMPLATE, &data).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 1_000);
    }

    /// Без участника № 2 форма тоже собирается: тендер идет к п. 81.4.
    #[test]
    fn protocol_renders_without_a_runner_up() {
        let data = json!({
            "number": "У-0TEST",
            "tender_number": "0TEST",
            "tender_title": "Тестовый тендер",
            "commission": "Тендерная комиссия (тест)",
            "held_at": "20.08.2026 10:00",
            "place": "уточняется юридической службой",
            "deadline": "27.08.2026 18:00",
            "evasions": [{
                "name": "ТОО «Уклонист»",
                "place": "победитель",
                "ground": "Письменный отказ от подписания договора",
                "declared_at": "20.08.2026 09:30",
            }],
            "lots": [],
            "members": [{"name": "Председатель П.П.", "role": "председатель"}],
            "generated_at": "20.08.2026 10:05",
        });

        let bytes = pdf::render(TEMPLATE, &data).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }
}
