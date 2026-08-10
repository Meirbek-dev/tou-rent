//! Льготные схемы особого порядка (М12, FR-1205, п. 95–96, Прил. 4).
//!
//! Льгота - не скидка «по договоренности», а расписание платы по годам найма:
//! первый год наниматель возмещает только коммунальные расходы, со второго
//! платит половину ставки Прил. 4 (п. 95–96). Социальному арендатору льгота
//! приходит иначе - коэффициентом Ксоц внутри самого расчета (FR-201),
//! поэтому расписания у нее нет.
//!
//! Условия льготы - тоже правило, а не пожелание: оборудованию в
//! образовательном процессе нужно согласование Ученого совета (п. 95),
//! спин-оффу - обучение не менее пяти кредитов в семестр (п. 96).
//!
//! TODO-ENGINEER: доля второго года (50 %), квоты стажировок и порядок
//! согласования Ученого совета проверяются по Правилам - Q-010.

use rust_decimal::prelude::ToPrimitive as _;
use rust_decimal::{Decimal, dec};
use serde::{Deserialize, Serialize};

use crate::money::Money;
use crate::special::BenefitScheme;

/// Доля ставки Прил. 4 со второго года найма (п. 95–96).
pub const LATER_YEARS_SHARE: Decimal = dec!(0.5);

/// Обучение спин-оффом, кредитов в семестр (п. 96).
pub const SPIN_OFF_MIN_CREDITS: i32 = 5;

/// Чем определяется плата за конкретный год найма (п. 95–96).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum YearRule {
    /// Первый год: только коммунальные расходы
    CommunalOnly,
    /// Со второго года: доля ставки Прил. 4
    Share,
    /// Льготного расписания нет - ставка Прил. 4 целиком
    Full,
}

/// Условия льготы, которые обязан подтвердить наниматель (п. 95–96).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Conditions {
    /// Решение Ученого совета о согласовании (п. 95)
    pub council_approved: bool,
    /// Кредитов обучения в семестр, принятых спин-оффом (п. 96)
    pub study_credits: i32,
    /// Мест стажировок по квоте (п. 95). Величина квоты - из Правил (Q-010)
    pub internships: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BenefitError {
    /// INV-095: льгота образовательного оборудования - с согласования
    /// Ученого совета (п. 95)
    #[error("INV-095: льгота применяется по согласованию Ученого совета (п. 95)")]
    CouncilApprovalMissing,
    /// INV-096: спин-офф обучает не менее пяти кредитов в семестр (п. 96)
    #[error("INV-096: спин-офф обучает не менее {SPIN_OFF_MIN_CREDITS} кредитов в семестр (п. 96)")]
    NotEnoughCredits,
    /// Квота стажировок не закрыта (п. 95); величина квоты - из Правил
    #[error("FR-1205: не закрыта квота стажировок (п. 95)")]
    InternshipQuotaUnmet,
    #[error("год найма считается с первого, получено {0}")]
    BadYear(i32),
}

/// Правила льготной схемы (FR-1205). Значения долей и порогов - из ТЗ,
/// квота стажировок приходит данными справочника: ее величины в ТЗ нет.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Benefit {
    pub scheme: BenefitScheme,
    /// Требуемая квота стажировок (п. 95); 0 - квота не установлена
    pub internship_quota: i32,
}

impl Benefit {
    pub fn new(scheme: BenefitScheme) -> Self {
        Self {
            scheme,
            internship_quota: 0,
        }
    }

    /// Есть ли у схемы льготное расписание по годам (п. 95–96).
    pub fn has_schedule(self) -> bool {
        matches!(
            self.scheme,
            BenefitScheme::EducationalEquipment | BenefitScheme::SpinOff
        )
    }

    /// Чем определяется плата за год найма (год считается с первого).
    pub fn year_rule(self, year: i32) -> Result<YearRule, BenefitError> {
        if year < 1 {
            return Err(BenefitError::BadYear(year));
        }
        if !self.has_schedule() {
            // Ксоц уже внутри расчета Прил. 4 (FR-201) - расписания нет
            return Ok(YearRule::Full);
        }
        Ok(if year == 1 {
            YearRule::CommunalOnly
        } else {
            YearRule::Share
        })
    }

    /// Условия льготы выполнены (п. 95–96).
    pub fn check(self, conditions: Conditions) -> Result<(), BenefitError> {
        match self.scheme {
            BenefitScheme::EducationalEquipment => {
                if !conditions.council_approved {
                    return Err(BenefitError::CouncilApprovalMissing);
                }
                if conditions.internships < self.internship_quota {
                    return Err(BenefitError::InternshipQuotaUnmet);
                }
                Ok(())
            }
            BenefitScheme::SpinOff => {
                if conditions.study_credits < SPIN_OFF_MIN_CREDITS {
                    return Err(BenefitError::NotEnoughCredits);
                }
                Ok(())
            }
            // Ксоц применяется расчетом ставки, отдельных условий у него нет
            BenefitScheme::Social | BenefitScheme::None => Ok(()),
        }
    }

    /// Месячная плата за год найма (п. 95–96): первый год - коммунальные
    /// расходы, дальше доля ставки Прил. 4. Округление - как у ставки (FR-204).
    pub fn monthly_for(
        self,
        year: i32,
        base_monthly: Money,
        communal_monthly: Money,
    ) -> Result<Money, BenefitError> {
        Ok(match self.year_rule(year)? {
            YearRule::CommunalOnly => communal_monthly,
            YearRule::Share => {
                Money::new(base_monthly.amount() * LATER_YEARS_SHARE).round_to_tenge()
            }
            YearRule::Full => base_monthly,
        })
    }

    /// Расписание платы на срок найма (п. 95–96) - то, что видит организатор
    /// при составлении договора и наниматель в карточке.
    pub fn schedule(
        self,
        months: i32,
        base_monthly: Money,
        communal_monthly: Money,
    ) -> Result<Vec<YearPayment>, BenefitError> {
        if months < 1 {
            return Err(BenefitError::BadYear(months));
        }
        let years = months.div_euclid(12) + i32::from(months.rem_euclid(12) > 0);
        (1..=years)
            .map(|year| {
                Ok(YearPayment {
                    year,
                    rule: self.year_rule(year)?,
                    monthly: self.monthly_for(year, base_monthly, communal_monthly)?,
                })
            })
            .collect()
    }
}

/// Плата за год найма (п. 95–96).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct YearPayment {
    pub year: i32,
    pub rule: YearRule,
    pub monthly: Money,
}

impl YearRule {
    pub fn as_str(self) -> &'static str {
        match self {
            YearRule::CommunalOnly => "communal_only",
            YearRule::Share => "share",
            YearRule::Full => "full",
        }
    }

    /// Доля ставки Прил. 4 в процентах - для подписи в интерфейсе
    pub fn share_pct(self) -> Option<u32> {
        match self {
            YearRule::CommunalOnly => None,
            YearRule::Share => (LATER_YEARS_SHARE * dec!(100)).to_u32(),
            YearRule::Full => Some(100),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn money(value: &str) -> Money {
        Money::new(value.parse().expect("сумма"))
    }

    /// п. 95–96: первый год - коммунальные расходы, дальше половина ставки.
    #[test]
    fn first_year_is_communal_then_half_the_rate() {
        for scheme in [BenefitScheme::EducationalEquipment, BenefitScheme::SpinOff] {
            let benefit = Benefit::new(scheme);
            assert_eq!(benefit.year_rule(1), Ok(YearRule::CommunalOnly));
            assert_eq!(benefit.year_rule(2), Ok(YearRule::Share));
            assert_eq!(benefit.year_rule(7), Ok(YearRule::Share));

            let base = money("100000");
            let communal = money("18000");
            assert_eq!(benefit.monthly_for(1, base, communal), Ok(communal));
            assert_eq!(
                benefit.monthly_for(2, base, communal),
                Ok(money("50000")),
                "со второго года - половина ставки Прил. 4"
            );
        }
    }

    /// Ксоц применяется расчетом ставки: расписания у социальной схемы нет.
    #[test]
    fn social_benefit_lives_inside_the_rate() {
        let benefit = Benefit::new(BenefitScheme::Social);
        assert!(!benefit.has_schedule());
        assert_eq!(benefit.year_rule(1), Ok(YearRule::Full));

        let base = money("100000");
        assert_eq!(
            benefit.monthly_for(1, base, money("18000")),
            Ok(base),
            "льгота уже внутри ставки (FR-201)"
        );
    }

    /// INV-095: без согласования Ученого совета льготы нет (п. 95).
    #[test]
    fn inv095_educational_benefit_needs_the_council() {
        let benefit = Benefit::new(BenefitScheme::EducationalEquipment);
        assert_eq!(
            benefit.check(Conditions::default()),
            Err(BenefitError::CouncilApprovalMissing)
        );
        assert_eq!(
            benefit.check(Conditions {
                council_approved: true,
                ..Conditions::default()
            }),
            Ok(())
        );
    }

    /// Квота стажировок закрывается фактическими местами (п. 95).
    #[test]
    fn internship_quota_must_be_met() {
        let benefit = Benefit {
            scheme: BenefitScheme::EducationalEquipment,
            internship_quota: 3,
        };
        let approved = Conditions {
            council_approved: true,
            internships: 2,
            ..Conditions::default()
        };
        assert_eq!(
            benefit.check(approved),
            Err(BenefitError::InternshipQuotaUnmet)
        );
        assert_eq!(
            benefit.check(Conditions {
                internships: 3,
                ..approved
            }),
            Ok(())
        );
    }

    /// INV-096: спин-офф обучает не менее пяти кредитов в семестр (п. 96).
    #[test]
    fn inv096_spin_off_teaches_five_credits() {
        let benefit = Benefit::new(BenefitScheme::SpinOff);
        assert_eq!(
            benefit.check(Conditions {
                study_credits: 4,
                ..Conditions::default()
            }),
            Err(BenefitError::NotEnoughCredits)
        );
        assert_eq!(
            benefit.check(Conditions {
                study_credits: SPIN_OFF_MIN_CREDITS,
                ..Conditions::default()
            }),
            Ok(()),
            "согласование Ученого совета спин-оффу не требуется (п. 96)"
        );
    }

    /// Расписание покрывает весь срок найма, неполный год считается годом.
    #[test]
    fn schedule_covers_the_whole_term() {
        let benefit = Benefit::new(BenefitScheme::SpinOff);
        let base = money("100000");
        let communal = money("18000");

        let schedule = benefit.schedule(30, base, communal).expect("расписание");
        assert_eq!(schedule.len(), 3, "30 месяцев - три года найма");
        assert_eq!(schedule[0].monthly, communal);
        assert_eq!(schedule[1].monthly, money("50000"));
        assert_eq!(schedule[2].rule, YearRule::Share);

        assert_eq!(benefit.schedule(12, base, communal).map(|s| s.len()), Ok(1));
        assert_eq!(
            benefit.schedule(0, base, communal),
            Err(BenefitError::BadYear(0))
        );
    }

    #[test]
    fn year_rules_have_wire_names_and_shares() {
        assert_eq!(YearRule::CommunalOnly.share_pct(), None);
        assert_eq!(YearRule::Share.share_pct(), Some(50));
        assert_eq!(YearRule::Full.share_pct(), Some(100));
        for rule in [YearRule::CommunalOnly, YearRule::Share, YearRule::Full] {
            assert!(!rule.as_str().is_empty());
        }
    }
}
