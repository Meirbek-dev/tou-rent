//! Протокол об итогах тендера (М7, FR-701, п. 73–74).
//!
//! Формируется после завершения торгов по всем лотам: jsonb-снимок + Typst-PDF
//! в RustFS, однократно (UNIQUE по виду протокола). Победитель и второе место
//! берутся из `core.auctions` - БД уже проверила, что это реальные ставки
//! этих торгов (FR-606), поэтому протокол не может разойтись с лентой.

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use object_store::ObjectStoreExt as _;
use object_store::PutPayload;
use object_store::path::Path as ObjectPath;
use serde_json::json;
use time::OffsetDateTime;
use tou_db::results::{LotResultRecord, MeetingError};
use tou_db::{admission, applications, results, tenders};
use tou_domain::policy::Action;
use uuid::Uuid;

use crate::admission::{GeneratedProtocolDto, format_almaty, member_role_ru, short_number};
use crate::announcement::format_decimal_ru;
use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::pdf;
use crate::request::{Json, Path};
use crate::state::AppState;

const TEMPLATE: &str = include_str!("templates/results.typ");

/// Срок оформления итогов - 3 рабочих дня после торгов (FR-701).
const PROTOCOL_BUSINESS_DAYS: i32 = 3;

/// Формирование протокола итогов (FR-701): состав комиссии, объекты, все
/// заявки с первоначальными ценами и решениями, победитель и второе место
/// по каждому лоту, обязательства сторон.
#[utoipa::path(
    post,
    path = "/api/v1/tenders/{id}/results-protocol",
    tag = "results",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 201, description = "Протокол итогов сформирован", body = GeneratedProtocolDto),
        (status = 404, description = "Тендер не найден", body = crate::error::Problem),
        (status = 409, description = "Торги не завершены или протокол уже сформирован",
         body = crate::error::Problem),
    )
)]
pub async fn generate_results_protocol(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<GeneratedProtocolDto>), ApiError> {
    user.require(Action::ProtocolGenerate)?;

    let tender = tenders::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let lots = results::lot_results(&state.db, id).await?;
    if lots.is_empty() {
        return Err(ApiError::RuleViolation("в тендере нет лотов".into()));
    }

    // Итог подводится по завершенным торгам: пока идет хотя бы одна комната,
    // победитель лота не определен (п. 69)
    if let Some(pending) = lots
        .iter()
        .find(|lot| lot.auction_status.as_deref() != Some("finished"))
    {
        return Err(ApiError::RuleViolation(format!(
            "по лоту №{} торги не завершены (статус: {}) - протокол итогов невозможен (FR-701)",
            pending.seq,
            pending.auction_status.as_deref().unwrap_or("не открыты")
        )));
    }

    let meeting = results::results_meeting(&state.db, user.id(), id)
        .await
        .map_err(|err| match err {
            MeetingError::NoCommission => ApiError::RuleViolation(
                "нет действующей тендерной комиссии - выполните `api seed` (М11)".into(),
            ),
            MeetingError::Db(db) => db.into(),
        })?;
    let members = admission::members_of(&state.db, meeting.commission_id).await?;
    let records = applications::list_for_tender(&state.db, user.id(), id).await?;

    let finished_at = lots.iter().filter_map(|lot| lot.finished_at).max();
    let deadline = match finished_at {
        Some(from) => {
            Some(results::protocol_deadline(&state.db, from, PROTOCOL_BUSINESS_DAYS).await?)
        }
        None => None,
    };

    let number = format!("И-{}", short_number(tender.id)); // TODO-ENGINEER: формат номера
    let content = protocol_content(
        &number, &tender, &meeting, &members, &lots, &records, deadline,
    );

    let template_data = content.clone();
    let pdf_bytes = tokio::task::spawn_blocking(move || pdf::render(TEMPLATE, &template_data))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;

    let pdf_key = format!("protocols/{id}/results.pdf");
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
            kind: "results",
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
            "протокол итогов уже сформирован (UNIQUE по тендеру)".into(),
        )),
    }
}

/// Реквизиты протокола итогов (без содержимого) - состояние для UI.
#[utoipa::path(
    get,
    path = "/api/v1/tenders/{id}/results-protocol",
    tag = "results",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 200, description = "Протокол сформирован", body = GeneratedProtocolDto),
        (status = 404, description = "Протокол не сформирован", body = crate::error::Problem),
    )
)]
pub async fn results_protocol_meta(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<GeneratedProtocolDto>, ApiError> {
    user.require(Action::AuctionWatch)?;
    let protocol = admission::get_protocol(&state.db, id, "results")
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(GeneratedProtocolDto {
        id: protocol.id,
        number: protocol.number,
        generated_at: protocol.generated_at,
    }))
}

/// PDF протокола итогов из RustFS.
#[utoipa::path(
    get,
    path = "/api/v1/tenders/{id}/results-protocol.pdf",
    tag = "results",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 200, description = "PDF протокола итогов (п. 73–74)",
         content_type = "application/pdf", body = Vec<u8>),
        (status = 404, description = "Протокол не сформирован", body = crate::error::Problem),
    )
)]
pub async fn results_protocol_pdf(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    user.require(Action::AuctionWatch)?;

    let protocol = admission::get_protocol(&state.db, id, "results")
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
                format!("inline; filename=\"tender-{id}-results-protocol.pdf\""),
            ),
        ],
        bytes.to_vec(),
    )
        .into_response())
}

/// Снимок протокола итогов (jsonb `core.protocols.content`): он же - данные
/// шаблона. Все значения предформатированы под печатную форму (NFR-01).
fn protocol_content(
    number: &str,
    tender: &tou_db::tenders::TenderRecord,
    meeting: &tou_db::admission::MeetingRecord,
    members: &[tou_db::admission::MemberRecord],
    lots: &[LotResultRecord],
    records: &[tou_db::applications::ApplicationRecord],
    deadline: Option<OffsetDateTime>,
) -> serde_json::Value {
    let lot_label = |lot_id: Uuid| {
        lots.iter()
            .find(|lot| lot.lot_id == lot_id)
            .map(|lot| format!("№{}", lot.seq))
            .unwrap_or_else(|| "-".to_owned())
    };

    json!({
        "number": number,
        "tender_number": short_number(tender.id),
        "tender_title": tender.title,
        "commission": meeting.commission_name,
        "held_at": format_almaty(meeting.held_at),
        // TODO-ENGINEER: место заседания - источник в контуре 1 отсутствует
        "place": "уточняется юридической службой",
        "deadline": deadline.map_or_else(|| "-".to_owned(), |value| format_almaty(Some(value))),
        "members": members.iter().map(|m| json!({
            "name": m.full_name,
            "role": member_role_ru(&m.member_role),
        })).collect::<Vec<_>>(),
        "lots": lots.iter().map(|lot| json!({
            "seq": lot.seq,
            "object": format!("{} ({})", lot.object_name, lot.object_address),
            "purpose": lot.purpose,
            "area": format_decimal_ru(lot.object_area_m2),
            "lease_months": lot.lease_months,
            "base_rate": format_decimal_ru(lot.base_rate_monthly),
        })).collect::<Vec<_>>(),
        "applications": records.iter().map(|a| json!({
            "applicant": a.applicant_details.expose()["name"].as_str().unwrap_or("-"),
            "lot": lot_label(a.lot_id),
            "price": a.price_amount.map(format_decimal_ru).unwrap_or_else(|| "-".into()),
            "decision": decision_ru(a),
        })).collect::<Vec<_>>(),
        "results": lots.iter().map(|lot| json!({
            "seq": lot.seq,
            "starting_bid": lot.starting_bid.map(format_decimal_ru).unwrap_or_else(|| "-".into()),
            "step": lot.bid_step.map(format_decimal_ru).unwrap_or_else(|| "-".into()),
            "winner": lot.winner_name.clone().unwrap_or_else(|| "не определен".into()),
            "winner_amount": lot.winner_amount.map(format_decimal_ru).unwrap_or_else(|| "-".into()),
            "runner_up": lot.runner_up_name.clone().unwrap_or_else(|| "нет".into()),
            "runner_up_amount": lot.runner_up_amount.map(format_decimal_ru)
                .unwrap_or_else(|| "-".into()),
        })).collect::<Vec<_>>(),
        "obligations": obligations(lots),
        "generated_at": format_almaty(meeting.held_at),
    })
}

fn decision_ru(application: &tou_db::applications::ApplicationRecord) -> String {
    match application.status.as_str() {
        "admitted" => "допущена к торгам".to_owned(),
        "rejected" => format!(
            "отклонена ({})",
            application.rejection_reason.as_deref().unwrap_or("-")
        ),
        "withdrawn" => "отозвана участником".to_owned(),
        other => format!("без решения ({other})"),
    }
}

/// Обязательства сторон (п. 73–74). Сроки - TODO-ENGINEER: точные величины
/// берутся из Правил, здесь очевидно рабочая формулировка контура 1.
fn obligations(lots: &[LotResultRecord]) -> Vec<String> {
    let mut lines = vec![
        "Победитель обязан подписать договор имущественного найма в срок, установленный Правилами (TODO-ENGINEER: п. 73–74)."
            .to_owned(),
        "При отказе победителя от подписания договор заключается с участником, занявшим второе место, по его ставке (п. 74)."
            .to_owned(),
        "Гарантийный взнос победителя засчитывается в счет обязательств по договору; остальным участникам возвращается в порядке Правил."
            .to_owned(),
    ];
    if lots.iter().any(|lot| lot.finished_early == Some(true)) {
        lines.push(
            "По отдельным лотам торги завершены досрочно при общем согласии участников (п. 67)."
                .to_owned(),
        );
    }
    lines
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Критерий Т12: шаблон компилируется в PDF на снимке с итогами торгов.
    #[test]
    fn results_protocol_renders_pdf() {
        let data = json!({
            "number": "И-0TEST",
            "tender_number": "0TEST",
            "tender_title": "Тестовый тендер - спецсимволы *#_$@«»",
            "commission": "Тендерная комиссия (тест)",
            "held_at": "11.08.2026 11:30",
            "place": "уточняется юридической службой",
            "deadline": "14.08.2026 18:00",
            "members": [
                {"name": "Председатель П.П.", "role": "председатель"},
                {"name": "Член М.М.", "role": "член комиссии"},
            ],
            "lots": [{
                "seq": 1, "object": "Корпус А (ул. Ломова, 64)", "purpose": "офис",
                "area": "42,00", "lease_months": 12, "base_rate": "21\u{202f}000,00",
            }],
            "applications": [
                {"applicant": "ТОО «Тест»", "lot": "№1", "price": "55\u{202f}000,00",
                 "decision": "допущена к торгам"},
                {"applicant": "ИП Иванов", "lot": "№1", "price": "39\u{202f}000,00",
                 "decision": "отклонена (fee_not_paid)"},
            ],
            "results": [{
                "seq": 1, "starting_bid": "55\u{202f}000,00", "step": "2\u{202f}750,00",
                "winner": "ТОО «Тест»", "winner_amount": "79\u{202f}750,00",
                "runner_up": "ТОО «Второй»", "runner_up_amount": "77\u{202f}000,00",
            }],
            "obligations": ["Победитель обязан подписать договор.", "Второе место - при отказе победителя."],
            "generated_at": "11.08.2026 11:30",
        });

        let bytes = pdf::render(TEMPLATE, &data).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        assert!(bytes.len() > 1_000);
    }
}
