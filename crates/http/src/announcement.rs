//! PDF объявления о тендере по форме Прил. 1 (FR-303).
//! Видимость - как у карточки тендера: черновик доступен только organizer
//! (предпросмотр с водяным знаком), опубликованные - публично (п. 5–6).

use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use rust_decimal::Decimal;
use serde_json::json;
use time::macros::format_description;
use time::{OffsetDateTime, UtcOffset};
use tou_db::objects::ObjectRecord;
use tou_db::tenders::{self, LotRecord, TenderRecord};
use tou_domain::policy::Action;
use tou_domain::rates::RateUnit;
use tou_domain::tender::TenderStatus;
use uuid::Uuid;

use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::pdf;
use crate::request::Path;
use crate::state::AppState;

const TEMPLATE: &str = include_str!("templates/announcement.typ");

/// PDF объявления (Прил. 1). Для черновика (только organizer) - с пометкой
/// «черновик»; после публикации - публичный документ карточки (FR-1401).
#[utoipa::path(
    get,
    path = "/api/v1/tenders/{id}/announcement.pdf",
    tag = "tenders",
    params(("id" = Uuid, Path, description = "Тендер")),
    responses(
        (status = 200, description = "PDF объявления по форме Прил. 1 (FR-303)",
         content_type = "application/pdf", body = Vec<u8>),
        (status = 404, description = "Не найден", body = crate::error::Problem),
    )
)]
pub async fn announcement_pdf(
    user: Option<CurrentUser>,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let record = tenders::get(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;

    if record.status == TenderStatus::Draft.as_str() {
        let manages = user
            .as_ref()
            .map(|u| u.require(Action::TenderManage).is_ok())
            .unwrap_or(false);
        if !manages {
            return Err(ApiError::NotFound); // черновик не публикуется (п. 5–6)
        }
    }

    let lots = tenders::lots_of(&state.db, record.id).await?;
    let mut pairs = Vec::with_capacity(lots.len());
    for lot in lots {
        let object = tou_db::objects::get(&state.db, lot.object_id)
            .await?
            .ok_or_else(|| {
                // FK лота гарантирует объект; отсутствие - рассинхрон данных
                ApiError::internal(std::io::Error::other(format!(
                    "объект {} лота {} не найден",
                    lot.object_id, lot.id
                )))
            })?;
        pairs.push((lot, object));
    }

    let data = announcement_data(&record, &pairs);
    let pdf_bytes = tokio::task::spawn_blocking(move || pdf::render(TEMPLATE, &data))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)?;

    let filename = format!("tender-{}-announcement.pdf", record.id);
    Ok((
        [
            (header::CONTENT_TYPE, "application/pdf".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("inline; filename=\"{filename}\""),
            ),
        ],
        pdf_bytes,
    )
        .into_response())
}

/// Печатная форма объявления как снимок: используется публикацией новой
/// редакции документации (FR-304) - редакция сохраняет форму на момент
/// изменения, а не пересобирает ее задним числом.
pub(crate) async fn render_announcement(
    state: &AppState,
    tender_id: Uuid,
) -> Result<Vec<u8>, ApiError> {
    let record = tenders::get(&state.db, tender_id)
        .await?
        .ok_or(ApiError::NotFound)?;

    let lots = tenders::lots_of(&state.db, record.id).await?;
    let mut pairs = Vec::with_capacity(lots.len());
    for lot in lots {
        let object = tou_db::objects::get(&state.db, lot.object_id)
            .await?
            .ok_or_else(|| {
                ApiError::internal(std::io::Error::other(format!(
                    "объект {} лота {} не найден",
                    lot.object_id, lot.id
                )))
            })?;
        pairs.push((lot, object));
    }

    let data = announcement_data(&record, &pairs);
    tokio::task::spawn_blocking(move || pdf::render(TEMPLATE, &data))
        .await
        .map_err(ApiError::internal)?
        .map_err(ApiError::internal)
}

/// Данные шаблона: все значения предформатированы - Typst-шаблон не содержит
/// логики локализации (печатные формы контура 1 - ru, NFR-01).
fn announcement_data(
    tender: &TenderRecord,
    lots: &[(LotRecord, ObjectRecord)],
) -> serde_json::Value {
    let viewings: Vec<serde_json::Value> = lots
        .iter()
        .filter_map(|(lot, _)| {
            lot.viewing_terms
                .as_deref()
                .map(|text| json!({ "seq": lot.seq, "text": text }))
        })
        .collect();

    json!({
        "is_draft": tender.status == TenderStatus::Draft.as_str(),
        "number": short_number(tender.id),
        "title": tender.title,
        "published_at": format_almaty_opt(tender.announced_at),
        "submission_deadline": format_almaty_opt(tender.submission_deadline),
        "opening_at": format_almaty_opt(tender.opening_at),
        "trading_at": format_almaty_opt(tender.trading_at),
        "lots": lots.iter().map(|(lot, object)| json!({
            "seq": lot.seq,
            "object": format!("{} ({})", object.name, object.address),
            "area": format_decimal_ru(object.area_m2),
            "purpose": lot.purpose,
            "lease_months": lot.lease_months,
            "monthly": format_decimal_ru(lot.base_rate_monthly),
            // FR-205: у почасового лота ставка указывается за час (п. 97)
            "rate_unit": lot.rate_unit.parse::<RateUnit>()
                .map(RateUnit::title_ru)
                .unwrap_or(RateUnit::Monthly.title_ru()),
            "hours_total": lot.hours_total,
            "fee": format_decimal_ru(lot.guarantee_fee),
        })).collect::<Vec<_>>(),
        "viewings": viewings,
    })
}

/// Короткий номер для печатной формы - первые 8 знаков UUIDv7 (монотонен).
fn short_number(id: Uuid) -> String {
    id.simple().to_string()[..8].to_uppercase()
}

/// NFR-03: хранение в UTC, отображение - Asia/Almaty (UTC+5).
fn format_almaty_opt(value: Option<OffsetDateTime>) -> String {
    let Some(ts) = value else {
        return "не назначено".to_owned();
    };
    let almaty = UtcOffset::from_hms(5, 0, 0).unwrap_or(UtcOffset::UTC);
    ts.to_offset(almaty)
        .format(format_description!("[day].[month].[year] [hour]:[minute]"))
        .unwrap_or_else(|_| "не назначено".to_owned())
}

/// «52 495,00» - группировка разрядов узким неразрывным пробелом,
/// десятичная запятая, ровно два знака (FR-204: тиыны значимы).
pub(crate) fn format_decimal_ru(value: Decimal) -> String {
    let rounded = value.round_dp(2);
    let negative = rounded.is_sign_negative();
    let text = rounded.abs().to_string();
    let (int_part, frac_part) = text.split_once('.').unwrap_or((text.as_str(), ""));

    let mut grouped = String::with_capacity(int_part.len() + int_part.len() / 3 + 3);
    for (i, ch) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i).is_multiple_of(3) {
            grouped.push('\u{202f}');
        }
        grouped.push(ch);
    }

    let frac: String = frac_part.chars().chain("00".chars()).take(2).collect();
    format!("{}{grouped},{frac}", if negative { "−" } else { "" })
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;
    use time::macros::datetime;

    use super::*;

    fn sample() -> (TenderRecord, Vec<(LotRecord, ObjectRecord)>) {
        let tender_id = Uuid::nil();
        let object_id = Uuid::from_u128(7);
        let tender = TenderRecord {
            id: tender_id,
            status: "announced".to_owned(),
            title: "Аренда помещений учебного корпуса - спецсимволы *#_$@«»".to_owned(),
            organizer_id: Uuid::from_u128(1),
            announced_at: Some(datetime!(2026-08-06 08:52 UTC)),
            submission_deadline: Some(datetime!(2026-08-17 08:52 UTC)),
            opening_at: Some(datetime!(2026-08-18 08:52 UTC)),
            opened_at: None,
            trading_at: None,
            zoom_url: None,
            zoom_recording_url: None,
            repeat_of: None,
            created_at: datetime!(2026-08-06 08:00 UTC),
            updated_at: datetime!(2026-08-06 08:00 UTC),
        };
        let lot = LotRecord {
            id: Uuid::from_u128(2),
            tender_id,
            seq: 1,
            object_id,
            purpose: "офис".to_owned(),
            lease_months: 12,
            base_rate_monthly: Decimal::new(5_249_500, 2),
            guarantee_fee: Decimal::new(5_249_500, 2),
            rate_calculation: serde_json::json!({}),
            viewing_terms: Some("по будням 09:00–17:00".to_owned()),
            rate_unit: "monthly".to_owned(),
            hours_total: None,
            cancelled_at: None,
            cancel_reason: None,
        };
        let object = ObjectRecord {
            id: object_id,
            kind: "premises".to_owned(),
            name: "Аудитория 101".to_owned(),
            address: "г. Павлодар, ул. Демонстрационная, 1".to_owned(),
            area_m2: Decimal::new(4200, 2),
            floor_part: None,
            premises_type_code: None,
            premises_kind_code: None,
            comfort_code: None,
            location_code: None,
            photo_keys: vec![],
            status: "in_tender".to_owned(),
            created_at: datetime!(2026-08-06 08:00 UTC),
            updated_at: datetime!(2026-08-06 08:00 UTC),
        };
        (tender, vec![(lot, object)])
    }

    #[test]
    fn announcement_renders_pdf() {
        let (tender, lots) = sample();
        let data = announcement_data(&tender, &lots);
        let bytes = pdf::render(TEMPLATE, &data).unwrap();
        assert!(bytes.starts_with(b"%PDF"), "не PDF: {:?}", &bytes[..8]);
        assert!(bytes.len() > 1_000);
    }

    #[test]
    fn draft_gets_watermark_flag_and_dates_render_in_almaty() {
        let (mut tender, lots) = sample();
        tender.status = "draft".to_owned();
        let data = announcement_data(&tender, &lots);
        assert_eq!(data["is_draft"], true);
        // 08:52 UTC → 13:52 Asia/Almaty (NFR-03)
        assert_eq!(data["submission_deadline"], "17.08.2026 13:52");
        assert_eq!(data["trading_at"], "не назначено");
    }

    #[test]
    fn ru_decimal_format_groups_thousands() {
        assert_eq!(
            format_decimal_ru(Decimal::new(5_249_500, 2)),
            "52\u{202f}495,00"
        );
        assert_eq!(
            format_decimal_ru(Decimal::new(123_456_789, 2)),
            "1\u{202f}234\u{202f}567,89"
        );
        assert_eq!(format_decimal_ru(Decimal::new(500, 0)), "500,00");
        assert_eq!(format_decimal_ru(Decimal::new(5, 1)), "0,50");
    }
}
