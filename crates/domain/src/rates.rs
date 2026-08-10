//! Расчет арендной ставки по Прил. 4 Правил (М2).
//!
//! Чистая функция без IO: значения МРП и коэффициентов на нужную дату
//! выбирает слой данных (`refdata`, FR-202), сюда они приходят готовыми.
//! Результат [`RateCalculation`] сериализуется в `core.lots.rate_calculation`
//! как замороженный снимок - изменение справочников не меняет прошлые расчеты.
//!
//! FR-203 (п. 138): коэффициент Ки при тендерных процедурах исключается -
//! его в этой модели нет вовсе; закрытость набора проверяет тест.

use rust_decimal::{Decimal, dec};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::money::Money;

/// Закрытый набор множителей формулы Прил. 4 (п. 137):
/// `Ап = Рбс × S × (Кт×Кк×Кск×Кр×Квд×Копф×Кфу×Ксоц×К×Кн / Кв)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoefficientCode {
    /// Кт - тип помещения
    Kt,
    /// Кк - комфортность
    Kk,
    /// Кск
    Ksk,
    /// Кр - расположение
    Kr,
    /// Квд - вид деятельности нанимателя
    Kvd,
    /// Копф - организационно-правовая форма
    Kopf,
    /// Кфу
    Kfu,
    /// Ксоц - социальный (0.5 для социальных арендаторов, FR-1205)
    Ksots,
    /// К
    K,
    /// Кн
    Kn,
    /// Кв - делитель формулы
    Kv,
}

impl CoefficientCode {
    pub const ALL: [CoefficientCode; 11] = [
        CoefficientCode::Kt,
        CoefficientCode::Kk,
        CoefficientCode::Ksk,
        CoefficientCode::Kr,
        CoefficientCode::Kvd,
        CoefficientCode::Kopf,
        CoefficientCode::Kfu,
        CoefficientCode::Ksots,
        CoefficientCode::K,
        CoefficientCode::Kn,
        CoefficientCode::Kv,
    ];

    /// Код в `refdata.rate_coefficients.coefficient`.
    pub fn as_str(self) -> &'static str {
        match self {
            CoefficientCode::Kt => "kt",
            CoefficientCode::Kk => "kk",
            CoefficientCode::Ksk => "ksk",
            CoefficientCode::Kr => "kr",
            CoefficientCode::Kvd => "kvd",
            CoefficientCode::Kopf => "kopf",
            CoefficientCode::Kfu => "kfu",
            CoefficientCode::Ksots => "ksots",
            CoefficientCode::K => "k",
            CoefficientCode::Kn => "kn",
            CoefficientCode::Kv => "kv",
        }
    }
}

/// Выбранная опция множителя: снимок кода опции и значения из
/// `refdata.rate_coefficients` на дату расчета (объяснимость, FR-201).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Factor {
    pub option_code: String,
    pub value: Decimal,
}

impl Factor {
    pub fn new(option_code: impl Into<String>, value: Decimal) -> Self {
        Self {
            option_code: option_code.into(),
            value,
        }
    }
}

/// Все 11 множителей формулы: обязательность каждого поля гарантирует
/// компилятор - «забыть коэффициент» нельзя.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateFactors {
    pub kt: Factor,
    pub kk: Factor,
    pub ksk: Factor,
    pub kr: Factor,
    pub kvd: Factor,
    pub kopf: Factor,
    pub kfu: Factor,
    pub ksots: Factor,
    pub k: Factor,
    pub kn: Factor,
    pub kv: Factor,
}

impl RateFactors {
    fn all(&self) -> [(CoefficientCode, &Factor); 11] {
        [
            (CoefficientCode::Kt, &self.kt),
            (CoefficientCode::Kk, &self.kk),
            (CoefficientCode::Ksk, &self.ksk),
            (CoefficientCode::Kr, &self.kr),
            (CoefficientCode::Kvd, &self.kvd),
            (CoefficientCode::Kopf, &self.kopf),
            (CoefficientCode::Kfu, &self.kfu),
            (CoefficientCode::Ksots, &self.ksots),
            (CoefficientCode::K, &self.k),
            (CoefficientCode::Kn, &self.kn),
            (CoefficientCode::Kv, &self.kv),
        ]
    }
}

/// Входы расчета: МРП на год расчета (FR-202) и площадь объекта.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateInputs {
    pub mrp: Money,
    pub area_m2: Decimal,
    pub factors: RateFactors,
}

/// Результат расчета со всеми промежуточными значениями - «расшифровка»
/// для калькулятора организатора (T7) и снимок в лоте (FR-201, FR-202).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateCalculation {
    pub inputs: RateInputs,
    /// Рбс = 1,5 × МРП, за м² в год (п. 137)
    pub base_rate_rbs: Decimal,
    /// Кт×Кк×Кск×Кр×Квд×Копф×Кфу×Ксоц×К×Кн / Кв
    pub multiplier: Decimal,
    /// Ап до округления, за год
    pub annual_raw: Decimal,
    /// Ап за год, округление тиынов по FR-204 (п. 140–143)
    pub annual: Money,
    /// Месячная базовая ставка = Ап/12 с округлением FR-204 -
    /// именно она замораживается в лоте (`core.lots.base_rate_monthly`)
    pub monthly: Money,
    /// Гарантийный взнос лота = месячная базовая ставка (FR-206, п. 25)
    pub guarantee_fee: Money,
    /// НДС не входит в базовую ставку (FR-204, п. 143) - флаг для UI и печатных форм
    pub vat_included: bool,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RateError {
    #[error("площадь должна быть положительной, получено {0}")]
    NonPositiveArea(Decimal),
    #[error("МРП должен быть положительным, получено {0}")]
    NonPositiveMrp(Decimal),
    #[error("коэффициент {code:?} должен быть положительным, получено {value}")]
    NonPositiveFactor {
        code: CoefficientCode,
        value: Decimal,
    },
}

/// Чистый расчет ставки по п. 137 Прил. 4 (FR-201).
pub fn calculate(inputs: RateInputs) -> Result<RateCalculation, RateError> {
    if inputs.mrp.amount() <= Decimal::ZERO {
        return Err(RateError::NonPositiveMrp(inputs.mrp.amount()));
    }
    if inputs.area_m2 <= Decimal::ZERO {
        return Err(RateError::NonPositiveArea(inputs.area_m2));
    }
    for (code, factor) in inputs.factors.all() {
        if factor.value <= Decimal::ZERO {
            return Err(RateError::NonPositiveFactor {
                code,
                value: factor.value,
            });
        }
    }

    let f = &inputs.factors;
    let base_rate_rbs = dec!(1.5) * inputs.mrp.amount();
    let numerator = f.kt.value
        * f.kk.value
        * f.ksk.value
        * f.kr.value
        * f.kvd.value
        * f.kopf.value
        * f.kfu.value
        * f.ksots.value
        * f.k.value
        * f.kn.value;
    let multiplier = numerator / f.kv.value;

    let annual_raw = base_rate_rbs * inputs.area_m2 * multiplier;
    let annual = Money::new(annual_raw).round_to_tenge();
    let monthly = Money::new(annual_raw / dec!(12)).round_to_tenge();

    Ok(RateCalculation {
        base_rate_rbs,
        multiplier,
        annual_raw,
        annual,
        monthly,
        guarantee_fee: monthly, // FR-206
        vat_included: false,    // FR-204, п. 143
        inputs,
    })
}

/// Минимальная почасовая ставка - 2 МРП за час (FR-205, п. 97, Прил. 4 п. 6).
pub const HOURLY_MIN_MRP: Decimal = dec!(2);

/// Единица базовой ставки лота (FR-205): помесячная аренда площади либо
/// почасовая аренда помещения (п. 97). Паритет с enum БД `core.rate_unit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateUnit {
    /// Ставка за месяц (Прил. 4 п. 137): предмет торгов - месячная плата
    Monthly,
    /// Ставка за час (п. 97): предмет торгов - плата за час
    Hourly,
}

impl RateUnit {
    pub const ALL: [RateUnit; 2] = [RateUnit::Monthly, RateUnit::Hourly];

    pub fn as_str(self) -> &'static str {
        match self {
            RateUnit::Monthly => "monthly",
            RateUnit::Hourly => "hourly",
        }
    }

    /// Единица в печатных формах и интерфейсе (ru - формы контура 1, NFR-01).
    pub fn title_ru(self) -> &'static str {
        match self {
            RateUnit::Monthly => "в месяц",
            RateUnit::Hourly => "за час",
        }
    }

    pub fn rule_ref(self) -> &'static str {
        match self {
            RateUnit::Monthly => "п. 137",
            RateUnit::Hourly => "п. 97",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("неизвестная единица ставки: {0}")]
pub struct UnknownRateUnit(pub String);

impl std::str::FromStr for RateUnit {
    type Err = UnknownRateUnit;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        RateUnit::ALL
            .into_iter()
            .find(|unit| unit.as_str() == s)
            .ok_or_else(|| UnknownRateUnit(s.to_owned()))
    }
}

/// Расчет почасовой ставки (FR-205). Отдельный тип, а не поле в
/// [`RateCalculation`]: почасовая аренда считается не от площади за год,
/// а от минимума в 2 МРП за час, который коэффициенты Прил. 4 могут только
/// повысить - «ставка от 2 МРП/час» (A-061).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HourlyRate {
    pub mrp: Money,
    pub factors: RateFactors,
    /// 2 × МРП - нижняя граница ставки за час (п. 97)
    pub floor: Money,
    /// Тот же множитель Прил. 4, что и в годовом расчете
    pub multiplier: Decimal,
    /// 2 × МРП × множитель до округления и до применения нижней границы
    pub hourly_raw: Decimal,
    /// Ставка за час: округление тиынов FR-204, но не ниже 2 МРП (п. 97)
    pub hourly: Money,
    /// Сработала ли нижняя граница: множитель понизил бы ставку
    pub floor_applied: bool,
    /// НДС не входит в базовую ставку (FR-204, п. 143)
    pub vat_included: bool,
}

impl HourlyRate {
    /// Стоимость объема часов, разыгрываемого лотом (FR-206: от нее считается
    /// гарантийный взнос почасового лота).
    pub fn total_for(&self, hours: i32) -> Option<Money> {
        (hours > 0).then(|| Money::new(self.hourly.amount() * Decimal::from(hours)))
    }
}

/// Почасовая ставка по тем же коэффициентам Прил. 4 (FR-205, п. 97).
/// Площадь в расчет не входит: почасово сдается помещение целиком.
pub fn calculate_hourly(mrp: Money, factors: RateFactors) -> Result<HourlyRate, RateError> {
    if mrp.amount() <= Decimal::ZERO {
        return Err(RateError::NonPositiveMrp(mrp.amount()));
    }
    for (code, factor) in factors.all() {
        if factor.value <= Decimal::ZERO {
            return Err(RateError::NonPositiveFactor {
                code,
                value: factor.value,
            });
        }
    }

    let f = &factors;
    let multiplier = (f.kt.value
        * f.kk.value
        * f.ksk.value
        * f.kr.value
        * f.kvd.value
        * f.kopf.value
        * f.kfu.value
        * f.ksots.value
        * f.k.value
        * f.kn.value)
        / f.kv.value;

    let floor = Money::new(HOURLY_MIN_MRP * mrp.amount()).round_to_tenge();
    let hourly_raw = HOURLY_MIN_MRP * mrp.amount() * multiplier;
    let rounded = Money::new(hourly_raw).round_to_tenge();

    // «Ставка от 2 МРП/час» (п. 97): коэффициенты поднимают ставку, но
    // опустить ее ниже минимума не могут
    let floor_applied = rounded.amount() < floor.amount();
    let hourly = if floor_applied { floor } else { rounded };

    Ok(HourlyRate {
        mrp,
        factors,
        floor,
        multiplier,
        hourly_raw,
        hourly,
        floor_applied,
        vat_included: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    fn unit_factors() -> RateFactors {
        let one = || Factor::new("default", Decimal::ONE);
        RateFactors {
            kt: one(),
            kk: one(),
            ksk: one(),
            kr: one(),
            kvd: one(),
            kopf: one(),
            kfu: one(),
            ksots: one(),
            k: one(),
            kn: one(),
            kv: one(),
        }
    }

    #[test]
    fn baseline_all_unit_factors() {
        // FR-201: Ап = 1.5 × МРП × S при единичных множителях
        let calc = calculate(RateInputs {
            mrp: Money::new(dec("100")),
            area_m2: dec("1"),
            factors: unit_factors(),
        })
        .unwrap();
        assert_eq!(calc.base_rate_rbs, dec("150.0"));
        assert_eq!(calc.annual.amount(), dec("150"));
        // FR-204: 150/12 = 12.50 -> 13 (тиыны >= 50 округляются вверх)
        assert_eq!(calc.monthly.amount(), dec("13"));
        assert_eq!(calc.guarantee_fee, calc.monthly); // FR-206
        assert!(!calc.vat_included); // FR-204
    }

    #[test]
    fn monthly_tiyn_below_fifty_rounds_down() {
        // 149.88 / 12 = 12.49 -> 12 (FR-204)
        let calc = calculate(RateInputs {
            mrp: Money::new(dec("99.92")),
            area_m2: dec("1"),
            factors: unit_factors(),
        })
        .unwrap();
        assert_eq!(calc.annual_raw, dec("149.880"));
        assert_eq!(calc.monthly.amount(), dec("12"));
    }

    #[test]
    fn kv_divides_the_product() {
        let mut factors = unit_factors();
        factors.kv = Factor::new("half", dec("2"));
        let calc = calculate(RateInputs {
            mrp: Money::new(dec("100")),
            area_m2: dec("10"),
            factors,
        })
        .unwrap();
        assert_eq!(calc.multiplier, dec("0.5"));
        assert_eq!(calc.annual.amount(), dec("750"));
    }

    #[test]
    fn non_positive_factor_is_rejected() {
        // G2: паник нет - ошибка типизирована
        let mut factors = unit_factors();
        factors.ksots = Factor::new("broken", Decimal::ZERO);
        let err = calculate(RateInputs {
            mrp: Money::new(dec("100")),
            area_m2: dec("1"),
            factors,
        })
        .unwrap_err();
        assert_eq!(
            err,
            RateError::NonPositiveFactor {
                code: CoefficientCode::Ksots,
                value: Decimal::ZERO
            }
        );
    }

    #[test]
    fn hourly_rate_starts_at_two_mrp() {
        // FR-205 (п. 97): при единичных коэффициентах ставка равна 2 МРП/час
        let rate = calculate_hourly(Money::new(dec("10000")), unit_factors()).unwrap();
        assert_eq!(rate.hourly.amount(), dec("20000"));
        assert_eq!(rate.floor.amount(), dec("20000"));
        assert!(!rate.floor_applied, "минимум не понадобился");
    }

    #[test]
    fn coefficients_raise_the_hourly_rate() {
        let mut factors = unit_factors();
        factors.kt = Factor::new("hall", dec("1.5"));
        let rate = calculate_hourly(Money::new(dec("10000")), factors).unwrap();
        assert_eq!(rate.multiplier, dec("1.5"));
        assert_eq!(rate.hourly.amount(), dec("30000"));
    }

    #[test]
    fn hourly_rate_never_falls_below_the_floor() {
        // Понижающие коэффициенты (например социальный 0,5) минимум не пробивают
        let mut factors = unit_factors();
        factors.ksots = Factor::new("social", dec("0.5"));
        let rate = calculate_hourly(Money::new(dec("10000")), factors).unwrap();
        assert!(rate.floor_applied, "сработала нижняя граница п. 97");
        assert_eq!(rate.hourly.amount(), dec("20000"));
        assert_eq!(rate.hourly_raw, dec("10000"), "расчет до границы сохранен");
    }

    #[test]
    fn hourly_total_needs_a_positive_volume() {
        let rate = calculate_hourly(Money::new(dec("10000")), unit_factors()).unwrap();
        assert_eq!(rate.total_for(4).map(|m| m.amount()), Some(dec("80000")));
        assert_eq!(rate.total_for(0), None);
    }

    #[test]
    fn rate_unit_wire_names_round_trip() {
        for unit in RateUnit::ALL {
            assert_eq!(unit.as_str().parse::<RateUnit>(), Ok(unit));
            assert!(!unit.title_ru().is_empty());
            assert!(unit.rule_ref().starts_with("п. "));
        }
        assert!("недельная".parse::<RateUnit>().is_err());
    }

    #[test]
    fn hourly_rate_rejects_impossible_inputs() {
        assert_eq!(
            calculate_hourly(Money::new(Decimal::ZERO), unit_factors()),
            Err(RateError::NonPositiveMrp(Decimal::ZERO))
        );
        let mut factors = unit_factors();
        factors.kv = Factor::new("broken", Decimal::ZERO);
        assert_eq!(
            calculate_hourly(Money::new(dec("100")), factors),
            Err(RateError::NonPositiveFactor {
                code: CoefficientCode::Kv,
                value: Decimal::ZERO
            })
        );
    }

    #[test]
    fn ki_is_not_part_of_the_model() {
        // FR-203 (п. 138): Ки исключен из тендерных расчетов - его нет в наборе
        assert_eq!(CoefficientCode::ALL.len(), 11);
        assert!(CoefficientCode::ALL.iter().all(|c| c.as_str() != "ki"));
    }
}
