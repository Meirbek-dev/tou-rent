//! Golden-тесты расчета ставки (FR-201, КП: >= 10 эталонных кейсов).
//!
//! Снапшоты insta - защищенный путь (арх. § 8): изменение только отдельным
//! MR с меткой needs-engineer. Значения коэффициентов в кейсах синтетические
//! (проверяется формула п. 137; предметные значения Прил. 4 живут в refdata, A-010).

use rust_decimal::{Decimal, dec};
use tou_domain::money::Money;
use tou_domain::rates::{Factor, RateError, RateFactors, RateInputs, calculate};

type TestResult = Result<(), RateError>;

fn factors(values: [Decimal; 11]) -> RateFactors {
    let [kt, kk, ksk, kr, kvd, kopf, kfu, ksots, k, kn, kv] = values;
    let f = |value: Decimal| Factor::new("case", value);
    RateFactors {
        kt: f(kt),
        kk: f(kk),
        ksk: f(ksk),
        kr: f(kr),
        kvd: f(kvd),
        kopf: f(kopf),
        kfu: f(kfu),
        ksots: f(ksots),
        k: f(k),
        kn: f(kn),
        kv: f(kv),
    }
}

fn golden(name: &str, mrp: Decimal, area: Decimal, values: [Decimal; 11]) -> TestResult {
    let calc = calculate(RateInputs {
        mrp: Money::new(mrp),
        area_m2: area,
        factors: factors(values),
    })?;
    insta::assert_json_snapshot!(name, calc);
    Ok(())
}

const UNIT: [Decimal; 11] = [dec!(1); 11];

#[test]
fn golden_baseline_unit_42m2() -> TestResult {
    golden("baseline_unit_42m2", dec!(4000), dec!(42), UNIT)
}

#[test]
fn golden_social_ksots_half() -> TestResult {
    // FR-1205: Ксоц = 0.5 для социальных арендаторов
    let mut values = UNIT;
    values[7] = dec!(0.5);
    golden("social_ksots_half", dec!(4000), dec!(42), values)
}

#[test]
fn golden_kv_divisor_two() -> TestResult {
    let mut values = UNIT;
    values[10] = dec!(2);
    golden("kv_divisor_two", dec!(4000), dec!(42), values)
}

#[test]
fn golden_atm_4m2_premium_location() -> TestResult {
    // Помещение под банкомат 4 м² (Прил. Б): повышающие Кт и Кр
    golden(
        "atm_4m2_premium_location",
        dec!(4000),
        dec!(4),
        [
            dec!(2.0),
            dec!(1.1),
            dec!(1),
            dec!(1.5),
            dec!(1.3),
            dec!(1),
            dec!(1),
            dec!(1),
            dec!(1),
            dec!(1),
            dec!(1),
        ],
    )
}

#[test]
fn golden_canteen_fractional_factors() -> TestResult {
    golden(
        "canteen_fractional_factors",
        dec!(3932.5),
        dec!(120.4),
        [
            dec!(1.2),
            dec!(0.9),
            dec!(1.05),
            dec!(1.1),
            dec!(0.8),
            dec!(1),
            dec!(1),
            dec!(1),
            dec!(1.15),
            dec!(1),
            dec!(1),
        ],
    )
}

#[test]
fn golden_rounding_tiyn_up() -> TestResult {
    // FR-204: годовая 150.00, месячная 12.50 -> 13
    golden("rounding_tiyn_up", dec!(100), dec!(1), UNIT)
}

#[test]
fn golden_rounding_tiyn_down() -> TestResult {
    // FR-204: 1.5 × 99.92 = 149.88; месячная 12.49 -> 12
    golden("rounding_tiyn_down", dec!(99.92), dec!(1), UNIT)
}

#[test]
fn golden_large_area_500_5m2() -> TestResult {
    golden("large_area_500_5m2", dec!(4000), dec!(500.5), UNIT)
}

#[test]
fn golden_decimal_area_and_kv() -> TestResult {
    let mut values = UNIT;
    values[10] = dec!(1.25);
    golden("decimal_area_and_kv", dec!(4000), dec!(42.75), values)
}

#[test]
fn golden_combined_all_factors() -> TestResult {
    golden(
        "combined_all_factors",
        dec!(3932.5),
        dec!(250),
        [
            dec!(1.5),
            dec!(1.2),
            dec!(0.95),
            dec!(1.4),
            dec!(1.1),
            dec!(0.9),
            dec!(1.05),
            dec!(0.5),
            dec!(1.2),
            dec!(1.1),
            dec!(1.3),
        ],
    )
}

#[test]
fn golden_seed_placeholder_mrp() -> TestResult {
    // МРП из seed (9999 - TODO-ENGINEER, A-010): расчет объясним и на плейсхолдере
    golden("seed_placeholder_mrp", dec!(9999), dec!(1), UNIT)
}
