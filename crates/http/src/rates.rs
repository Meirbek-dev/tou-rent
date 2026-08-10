//! Калькулятор ставок (М2): опции коэффициентов → снимок RateCalculation.
//! Значения берутся из refdata на текущую дату сервера БД (FR-202, NFR-03),
//! сам расчет - чистый `domain::rates` (FR-201).

use axum::extract::State;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tou_db::Db;
use tou_domain::money::Money;
use tou_domain::policy::Action;
use tou_domain::rates::{
    CoefficientCode, Factor, HourlyRate, RateCalculation, RateFactors, RateInputs, calculate,
    calculate_hourly,
};
use utoipa::ToSchema;

use crate::error::ApiError;
use crate::extract::CurrentUser;
use crate::request::Json;
use crate::state::AppState;

fn default_option() -> String {
    "default".to_owned()
}

/// Поля DTO, `Default`, сборка [`RateFactors`] и снимок факторов контракта
/// порождаются из одного перечня «поле → коэффициент»: рассинхрон
/// с Прил. 4 невозможен по построению.
macro_rules! rate_options {
    ($($field:ident => $code:ident),+ $(,)?) => {
        /// Выбор опции по каждому множителю Прил. 4; по умолчанию опция `default`.
        #[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
        pub struct RateOptionsDto {
            $(
                #[serde(default = "default_option")]
                pub $field: String,
            )+
        }

        impl Default for RateOptionsDto {
            fn default() -> Self {
                Self { $($field: default_option()),+ }
            }
        }

        impl RateOptionsDto {
            /// Полный набор факторов через резолвер «(коэффициент, опция) → значение».
            fn resolve_factors(
                &self,
                mut factor: impl FnMut(CoefficientCode, &str) -> Result<Factor, ApiError>,
            ) -> Result<RateFactors, ApiError> {
                Ok(RateFactors {
                    $($field: factor(CoefficientCode::$code, &self.$field)?),+
                })
            }
        }

        /// Снимок выбранных факторов в контракте (расшифровка FR-201).
        #[derive(Debug, Serialize, ToSchema)]
        pub struct RateFactorsDto {
            $(pub $field: FactorDto,)+
        }

        impl From<RateFactors> for RateFactorsDto {
            fn from(factors: RateFactors) -> Self {
                Self { $($field: FactorDto::from(factors.$field)),+ }
            }
        }
    };
}

rate_options! {
    kt => Kt, kk => Kk, ksk => Ksk, kr => Kr, kvd => Kvd, kopf => Kopf,
    kfu => Kfu, ksots => Ksots, k => K, kn => Kn, kv => Kv,
}

/// Множители Прил. 4 на текущую дату и МРП года (FR-202): общая часть
/// годового и почасового расчетов.
async fn resolve_inputs(
    db: &Db,
    options: &RateOptionsDto,
) -> Result<(Money, RateFactors), ApiError> {
    let (_, mrp) = tou_db::refdata::current_mrp(db).await?.ok_or_else(|| {
        ApiError::Validation("МРП на текущий год не заведен в refdata (A-010)".into())
    })?;

    let table = tou_db::refdata::coefficients_today(db).await?;
    let factors = options.resolve_factors(|code, option| {
        let value = table
            .get(&(code.as_str().to_owned(), option.to_owned()))
            .copied()
            .ok_or_else(|| {
                ApiError::Validation(format!(
                    "коэффициент {}: опция '{option}' не действует на текущую дату",
                    code.as_str()
                ))
            })?;
        Ok(Factor::new(option, value))
    })?;

    Ok((Money::new(mrp), factors))
}

/// Снимок почасового расчета (FR-205, п. 97): те же коэффициенты, но база -
/// 2 МРП за час, ниже которых ставка не опускается.
pub async fn build_hourly_calculation(
    db: &Db,
    options: &RateOptionsDto,
) -> Result<HourlyRate, ApiError> {
    let (mrp, factors) = resolve_inputs(db, options).await?;
    calculate_hourly(mrp, factors).map_err(|err| ApiError::Validation(err.to_string()))
}

/// Снимок расчета для площади и выбранных опций «на сегодня» (FR-201–202).
pub async fn build_calculation(
    db: &Db,
    area_m2: Decimal,
    options: &RateOptionsDto,
) -> Result<RateCalculation, ApiError> {
    let (mrp, factors) = resolve_inputs(db, options).await?;

    calculate(RateInputs {
        mrp,
        area_m2,
        factors,
    })
    .map_err(|err| ApiError::Validation(err.to_string()))
}

/// Множитель снимка: код выбранной опции и значение на дату расчета.
#[derive(Debug, Serialize, ToSchema)]
pub struct FactorDto {
    pub option_code: String,
    #[schema(value_type = String, example = "0.5")]
    pub value: Decimal,
}

impl From<Factor> for FactorDto {
    fn from(factor: Factor) -> Self {
        Self {
            option_code: factor.option_code,
            value: factor.value,
        }
    }
}

/// Расшифровка расчета (FR-201) - типизированный контракт для калькулятора
/// организатора (Т7); полный снимок в лоте хранится как есть (FR-202).
#[derive(Debug, Serialize, ToSchema)]
pub struct RateCalculationDto {
    #[schema(value_type = String, example = "3932")]
    pub mrp: Decimal,
    #[schema(value_type = String, example = "42.00")]
    pub area_m2: Decimal,
    pub factors: RateFactorsDto,
    /// Рбс = 1,5 × МРП, за м² в год (п. 137)
    #[schema(value_type = String, example = "5898")]
    pub base_rate_rbs: Decimal,
    /// Кт×Кк×Кск×Кр×Квд×Копф×Кфу×Ксоц×К×Кн / Кв
    #[schema(value_type = String, example = "0.5")]
    pub multiplier: Decimal,
    /// Ап за год до округления
    #[schema(value_type = String, example = "123858")]
    pub annual_raw: Decimal,
    /// Ап за год, округление тиынов FR-204
    #[schema(value_type = String, example = "123858.00")]
    pub annual: Decimal,
    /// Месячная базовая ставка - замораживается в лоте
    #[schema(value_type = String, example = "10321.50")]
    pub monthly: Decimal,
    /// Гарантийный взнос = месячная ставка (FR-206)
    #[schema(value_type = String, example = "10321.50")]
    pub guarantee_fee: Decimal,
    /// НДС не входит в базовую ставку (FR-204, п. 143)
    pub vat_included: bool,
}

impl From<RateCalculation> for RateCalculationDto {
    fn from(calc: RateCalculation) -> Self {
        Self {
            mrp: calc.inputs.mrp.amount(),
            area_m2: calc.inputs.area_m2,
            factors: RateFactorsDto::from(calc.inputs.factors),
            base_rate_rbs: calc.base_rate_rbs,
            multiplier: calc.multiplier,
            annual_raw: calc.annual_raw,
            annual: calc.annual.amount(),
            monthly: calc.monthly.amount(),
            guarantee_fee: calc.guarantee_fee.amount(),
            vat_included: calc.vat_included,
        }
    }
}

/// Опция коэффициента Прил. 4, действующая на текущую дату (FR-202).
#[derive(Debug, Serialize, ToSchema)]
pub struct RateOptionDto {
    /// Код коэффициента (kt, kk, ... kv)
    pub coefficient: String,
    pub option_code: String,
    #[schema(value_type = String, example = "0.5")]
    pub value: Decimal,
}

/// Справочник калькулятора: МРП и все действующие опции коэффициентов.
#[derive(Debug, Serialize, ToSchema)]
pub struct RateOptionsCatalog {
    pub mrp_year: i32,
    #[schema(value_type = String, example = "3932")]
    pub mrp: Decimal,
    pub options: Vec<RateOptionDto>,
}

/// Справочник для формы калькулятора (Т7): селекты опций строятся
/// из refdata, а не из констант фронта (FR-202).
#[utoipa::path(
    get,
    path = "/api/v1/rates/options",
    tag = "rates",
    responses(
        (status = 200, description = "Действующие опции Прил. 4 и МРП", body = RateOptionsCatalog),
        (status = 422, description = "refdata не заведена (A-010)", body = crate::error::Problem),
    )
)]
pub async fn options(
    user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<RateOptionsCatalog>, ApiError> {
    user.require(Action::RateCalculate)?;

    let (mrp_year, mrp) = tou_db::refdata::current_mrp(&state.db)
        .await?
        .ok_or_else(|| {
            ApiError::Validation("МРП на текущий год не заведен в refdata (A-010)".into())
        })?;

    let table = tou_db::refdata::coefficients_today(&state.db).await?;
    let mut options: Vec<RateOptionDto> = table
        .into_iter()
        .map(|((coefficient, option_code), value)| RateOptionDto {
            coefficient,
            option_code,
            value,
        })
        .collect();
    options.sort_by(|a, b| {
        (a.coefficient.as_str(), a.option_code.as_str())
            .cmp(&(b.coefficient.as_str(), b.option_code.as_str()))
    });

    Ok(Json(RateOptionsCatalog {
        mrp_year,
        mrp,
        options,
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RatePreviewRequest {
    /// Площадь, м²
    #[schema(value_type = String, example = "42.00")]
    pub area_m2: Decimal,
    #[serde(default)]
    pub options: RateOptionsDto,
}

/// Предпросмотр расчета для калькулятора организатора (Т7);
/// тот же код замораживает снимок в лоте при создании тендера.
#[utoipa::path(
    post,
    path = "/api/v1/rates/preview",
    tag = "rates",
    request_body = RatePreviewRequest,
    responses(
        (status = 200, description = "Расшифровка расчета по Прил. 4 (FR-201)", body = RateCalculationDto),
        (status = 422, description = "Нет данных refdata или неверные входы", body = crate::error::Problem),
    )
)]
pub async fn preview(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<RatePreviewRequest>,
) -> Result<Json<RateCalculationDto>, ApiError> {
    user.require(Action::RateCalculate)?;
    let calc = build_calculation(&state.db, body.area_m2, &body.options).await?;
    Ok(Json(RateCalculationDto::from(calc)))
}

/// Расшифровка почасового расчета (FR-205, п. 97): площадь в него не входит.
#[derive(Debug, Serialize, ToSchema)]
pub struct HourlyRateDto {
    #[schema(value_type = String, example = "3932")]
    pub mrp: Decimal,
    pub factors: RateFactorsDto,
    /// Нижняя граница: 2 МРП за час (п. 97)
    #[schema(value_type = String, example = "7864.00")]
    pub floor: Decimal,
    /// Кт×Кк×…×Кн / Кв - тот же множитель, что и в годовом расчете
    #[schema(value_type = String, example = "1.5")]
    pub multiplier: Decimal,
    /// 2 × МРП × множитель до округления и до нижней границы
    #[schema(value_type = String, example = "11796")]
    pub hourly_raw: Decimal,
    /// Ставка за час: округление FR-204, но не ниже 2 МРП
    #[schema(value_type = String, example = "11796.00")]
    pub hourly: Decimal,
    /// Множитель понизил бы ставку - применена граница п. 97
    pub floor_applied: bool,
    /// НДС не входит в базовую ставку (FR-204, п. 143)
    pub vat_included: bool,
}

impl From<HourlyRate> for HourlyRateDto {
    fn from(rate: HourlyRate) -> Self {
        Self {
            mrp: rate.mrp.amount(),
            factors: RateFactorsDto::from(rate.factors),
            floor: rate.floor.amount(),
            multiplier: rate.multiplier,
            hourly_raw: rate.hourly_raw,
            hourly: rate.hourly.amount(),
            floor_applied: rate.floor_applied,
            vat_included: rate.vat_included,
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct HourlyPreviewRequest {
    #[serde(default)]
    pub options: RateOptionsDto,
}

/// Предпросмотр почасовой ставки (FR-205, п. 97): «от 2 МРП/час» -
/// коэффициенты Прил. 4 могут ее только повысить.
#[utoipa::path(
    post,
    path = "/api/v1/rates/preview-hourly",
    tag = "rates",
    request_body = HourlyPreviewRequest,
    responses(
        (status = 200, description = "Расшифровка почасового расчета (FR-205)", body = HourlyRateDto),
        (status = 422, description = "Нет данных refdata или неверные входы", body = crate::error::Problem),
    )
)]
pub async fn preview_hourly(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<HourlyPreviewRequest>,
) -> Result<Json<HourlyRateDto>, ApiError> {
    user.require(Action::RateCalculate)?;
    let rate = build_hourly_calculation(&state.db, &body.options).await?;
    Ok(Json(HourlyRateDto::from(rate)))
}
