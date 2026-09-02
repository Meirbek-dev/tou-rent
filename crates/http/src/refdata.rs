//! Ведение справочников ставок админом (М15, FR-1901, FR-202).
//!
//! Календарь рабочих дней лежит рядом с двигателем сроков
//! ([`crate::obligations`]) - он читается расчетом сроков, а не ставки.
//! Здесь - МРП и коэффициенты Прил. 4.
//!
//! FR-202: правка справочника не меняет прошлые расчеты. Держится это не
//! запретом на правку, а тем, что в лоте лежит снимок `RateCalculation`,
//! а версии коэффициентов различаются по `effective_from` и не
//! перезаписываются - админ добавляет версию, а не исправляет историю.

use axum::extract::State;
use garde::Validate as _;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tou_db::refdata::{self, NewCoefficientVersion};
use tou_domain::policy::{Action, Compound};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::request::{Json, Path};
use crate::state::AppState;

#[derive(Debug, Serialize, ToSchema)]
pub struct MrpDto {
    pub year: i32,
    #[schema(value_type = String, example = "3932")]
    pub amount: Decimal,
}

/// МРП по годам (FR-201, Прил. 4): читают все, кто вправе считать ставку -
/// значение показателя не является закрытым сведением.
#[utoipa::path(
    get,
    path = "/api/v1/refdata/mrp",
    tag = "refdata",
    responses(
        (status = 200, description = "Заведенные годы МРП", body = Vec<MrpDto>),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
    )
)]
pub async fn list_mrp(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<MrpDto>>, ApiError> {
    user.require_any(Compound::RATE_REFDATA_READ)?;

    let items = refdata::mrp_all(&state.db)
        .await?
        .into_iter()
        .map(|r| MrpDto {
            year: r.year,
            amount: r.amount,
        })
        .collect();
    Ok(Json(items))
}

#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct SetMrpRequest {
    /// Величина МРП в тенге (> 0)
    #[garde(custom(positive_amount))]
    #[schema(value_type = String, example = "3932")]
    pub amount: Decimal,
}

/// Год МРП: те же границы, что у CHECK `mrp_year_check` (Прил. 4).
///
/// Рубеж БД оставался единственным, и год за его пределами приходил клиенту
/// ошибкой целостности - то есть 500 вместо 422, без имени поля. Проверка
/// у типа запроса называет и поле, и допустимые границы; CHECK остается
/// последним рубежом.
const MRP_YEARS: std::ops::RangeInclusive<i32> = 2000..=2100;

fn check_year(year: i32) -> Result<(), ApiError> {
    if MRP_YEARS.contains(&year) {
        return Ok(());
    }
    Err(ApiError::Validation(format!(
        "год МРП ожидается в пределах {}..{}, получен {year}",
        MRP_YEARS.start(),
        MRP_YEARS.end()
    )))
}

/// Величина МРП строго положительна (CHECK `mrp_amount_check`): ноль и
/// отрицательное значение обнулили бы или вывернули расчет ставки FR-201.
fn positive_amount(value: &Decimal, _ctx: &()) -> garde::Result {
    if *value > Decimal::ZERO {
        Ok(())
    } else {
        Err(garde::Error::new("величина МРП должна быть > 0"))
    }
}

/// Величина МРП на год (FR-1901). У показателя одна величина на год, поэтому
/// это правка значения, а не новая версия.
#[utoipa::path(
    put,
    path = "/api/v1/refdata/mrp/{year}",
    tag = "refdata",
    params(("year" = i32, Path, description = "Год", minimum = 2000, maximum = 2100)),
    request_body = SetMrpRequest,
    responses(
        (status = 200, description = "Величина сохранена", body = MrpDto),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
        (status = 422, description = "Год или величина недопустимы", body = crate::error::Problem),
    )
)]
pub async fn set_mrp(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(year): Path<i32>,
    Json(body): Json<SetMrpRequest>,
) -> Result<Json<MrpDto>, ApiError> {
    user.require(Action::RefdataManage)?;
    body.validate()
        .map_err(|report| ApiError::Validation(report.to_string()))?;
    check_year(year)?;

    let record = refdata::upsert_mrp(&state.db, user.id(), year, body.amount).await?;
    Ok(Json(MrpDto {
        year: record.year,
        amount: record.amount,
    }))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CoefficientVersionDto {
    pub id: Uuid,
    /// Код множителя Прил. 4: `kt`, `kk`, `ksk`, …
    pub coefficient: String,
    pub option_code: String,
    pub label_ru: String,
    pub label_kk: Option<String>,
    pub label_en: Option<String>,
    #[schema(value_type = String, example = "1.2000")]
    pub value: Decimal,
    #[serde(with = "crate::dto::iso_date")]
    #[schema(value_type = String, format = Date)]
    pub effective_from: time::Date,
    /// Версия применяется к расчетам сегодня
    pub current: bool,
}

/// Все версии коэффициентов Прил. 4 (FR-202): админ видит и действующее
/// значение, и то, что применялось к прошлым расчетам.
#[utoipa::path(
    get,
    path = "/api/v1/refdata/coefficients",
    tag = "refdata",
    responses(
        (status = 200, description = "Версии коэффициентов", body = Vec<CoefficientVersionDto>),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
    )
)]
pub async fn list_coefficients(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<CoefficientVersionDto>>, ApiError> {
    user.require(Action::RefdataManage)?;

    let items = refdata::coefficients_all(&state.db)
        .await?
        .into_iter()
        .map(|r| CoefficientVersionDto {
            id: r.id,
            coefficient: r.coefficient,
            option_code: r.option_code,
            label_ru: r.label_ru,
            label_kk: r.label_kk,
            label_en: r.label_en,
            value: r.value,
            effective_from: r.effective_from,
            current: r.current,
        })
        .collect();
    Ok(Json(items))
}

#[derive(Debug, Deserialize, garde::Validate, ToSchema)]
pub struct NewCoefficientRequest {
    #[garde(length(chars, min = 1, max = 20))]
    pub coefficient: String,
    #[garde(length(chars, min = 1, max = 50))]
    pub option_code: String,
    #[garde(length(chars, min = 1, max = 200))]
    pub label_ru: String,
    #[garde(inner(length(chars, min = 1, max = 200)))]
    pub label_kk: Option<String>,
    #[garde(inner(length(chars, min = 1, max = 200)))]
    pub label_en: Option<String>,
    #[garde(skip)]
    #[schema(value_type = String, example = "1.2000")]
    pub value: Decimal,
    /// Дата вступления версии в силу (FR-202)
    #[garde(skip)]
    #[serde(with = "crate::dto::iso_date")]
    #[schema(value_type = String, format = Date)]
    pub effective_from: time::Date,
}

/// Новая версия коэффициента (FR-1901, FR-202) - для существующей опции
/// Прил. 4. Прошлые расчеты не меняются: добавляется версия с датой
/// вступления в силу, прежняя остается в справочнике.
#[utoipa::path(
    post,
    path = "/api/v1/refdata/coefficients",
    tag = "refdata",
    request_body = NewCoefficientRequest,
    responses(
        (status = 201, description = "Версия добавлена", body = CoefficientVersionDto),
        (status = 403, description = "Недостаточно прав", body = crate::error::Problem),
        (status = 422, description = "Данные не прошли проверку", body = crate::error::Problem),
    )
)]
pub async fn add_coefficient(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<NewCoefficientRequest>,
) -> Result<(axum::http::StatusCode, Json<CoefficientVersionDto>), ApiError> {
    user.require(Action::RefdataManage)?;
    body.validate()
        .map_err(|report| ApiError::Validation(report.to_string()))?;

    // Перечень множителей и их опций задан Прил. 4 - это не данные админа.
    // Тот же рубеж стоит внешним ключом в БД; здесь он дает внятный отказ
    // вместо ошибки целостности
    if !refdata::rate_option_exists(&state.db, &body.coefficient, &body.option_code).await? {
        return Err(ApiError::Validation(format!(
            "опции '{}' у множителя '{}' нет в перечне Прил. 4: версионируется \
             значение существующей опции, новые опции не заводятся",
            body.option_code, body.coefficient
        )));
    }

    let id = refdata::upsert_coefficient_version(
        &state.db,
        user.id(),
        NewCoefficientVersion {
            coefficient: &body.coefficient,
            option_code: &body.option_code,
            label_ru: &body.label_ru,
            label_kk: body.label_kk.as_deref(),
            label_en: body.label_en.as_deref(),
            value: body.value,
            effective_from: body.effective_from,
        },
    )
    .await?;

    // Признак «действует сегодня» считает БД - пересчитывать его в Rust
    // значило бы завести вторую версию правила
    let created = refdata::coefficients_all(&state.db)
        .await?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| ApiError::internal(MissingCoefficient))?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(CoefficientVersionDto {
            id: created.id,
            coefficient: created.coefficient,
            option_code: created.option_code,
            label_ru: created.label_ru,
            label_kk: created.label_kk,
            label_en: created.label_en,
            value: created.value,
            effective_from: created.effective_from,
            current: created.current,
        }),
    ))
}

#[derive(Debug, thiserror::Error)]
#[error("только что записанная версия коэффициента не найдена")]
struct MissingCoefficient;

#[cfg(test)]
mod tests {
    use super::*;

    /// G-023: величина МРП вне рубежа БД должна отказываться проверкой типа
    /// (422 `validation_failed`), а не ошибкой целостности (500).
    #[test]
    fn non_positive_mrp_amount_is_rejected_by_validation() {
        for raw in ["0", "-1", "-0.01"] {
            let amount: Decimal = raw.parse().expect("десятичное число");
            let request = SetMrpRequest { amount };
            assert!(
                request.validate().is_err(),
                "величина {raw} должна отказываться проверкой"
            );
        }

        let ok = SetMrpRequest {
            amount: Decimal::from(3932),
        };
        assert!(ok.validate().is_ok());
    }

    /// Границы года совпадают с CHECK `mrp_year_check`.
    #[test]
    fn mrp_year_bounds_match_db_check() {
        for year in [1999, 2101, 0, -1, i32::MAX] {
            assert!(
                matches!(check_year(year), Err(ApiError::Validation(_))),
                "год {year} должен отказываться проверкой"
            );
        }
        for year in [2000, 2026, 2100] {
            assert!(check_year(year).is_ok(), "год {year} допустим");
        }
    }
}
