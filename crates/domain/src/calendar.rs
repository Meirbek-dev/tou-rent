//! Производственный календарь РК (М17, FR-1701): рабочие дни и сроки.
//!
//! «Рабочий день» в Правилах встречается везде (п. 23, 26, 54, 57–59, 73,
//! 111–118, 132–136), поэтому функция ровно одна - здесь и ее двойник в SQL
//! (`refdata.add_business_days`). Паритет двух реализаций проверяет гейт G12:
//! расхождение календарей означало бы разные сроки у системы и у БД.

use std::collections::BTreeSet;

use jiff::civil::{Date, Weekday};

/// Выходные и праздники РК; праздники ведет админ (`refdata.holidays`).
#[derive(Debug, Clone, Default)]
pub struct BusinessCalendar {
    holidays: BTreeSet<Date>,
}

impl BusinessCalendar {
    pub fn new(holidays: impl IntoIterator<Item = Date>) -> Self {
        Self {
            holidays: holidays.into_iter().collect(),
        }
    }

    /// Суббота и воскресенье - не рабочие; остальное решает список праздников.
    pub fn is_business_day(&self, date: Date) -> bool {
        !matches!(date.weekday(), Weekday::Saturday | Weekday::Sunday)
            && !self.holidays.contains(&date)
    }

    /// `days` рабочих дней после `start` (сам `start` не считается - как
    /// в SQL-двойнике). `days = 0` возвращает `start` без изменений.
    pub fn add_business_days(&self, start: Date, days: u32) -> Date {
        let mut date = start;
        let mut remaining = days;
        while remaining > 0 {
            date = date.tomorrow().unwrap_or(date);
            if self.is_business_day(date) {
                remaining -= 1;
            }
        }
        date
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    fn calendar() -> BusinessCalendar {
        // Праздники РК 2026 в тестовом объеме: 1–2 января и 8 марта (вс)
        BusinessCalendar::new([date(2026, 1, 1), date(2026, 1, 2), date(2026, 3, 8)])
    }

    #[test]
    fn weekend_is_not_a_business_day() {
        assert!(calendar().is_business_day(date(2026, 8, 7))); // пятница
        assert!(!calendar().is_business_day(date(2026, 8, 8))); // суббота
        assert!(!calendar().is_business_day(date(2026, 8, 9))); // воскресенье
    }

    #[test]
    fn holidays_are_skipped() {
        let calendar = calendar();
        assert!(!calendar.is_business_day(date(2026, 1, 1)));
        // 31 декабря 2025 - среда; +1 рабочий день перепрыгивает 1–2 января
        // (чт, пт - праздники) и выходные, попадая на понедельник 5 января
        assert_eq!(
            calendar.add_business_days(date(2025, 12, 31), 1),
            date(2026, 1, 5)
        );
    }

    #[test]
    fn zero_days_keeps_the_date() {
        assert_eq!(
            calendar().add_business_days(date(2026, 8, 8), 0),
            date(2026, 8, 8),
            "нулевой срок не двигает дату, даже если она выходной"
        );
    }

    #[test]
    fn three_business_days_from_friday_land_on_wednesday() {
        // пт 07.08 → пн 10.08 → вт 11.08 → ср 12.08
        assert_eq!(
            calendar().add_business_days(date(2026, 8, 7), 3),
            date(2026, 8, 12)
        );
    }

    #[test]
    fn counting_starts_after_the_given_day() {
        // Понедельник + 1 рабочий день = вторник: сам день не считается (п. 23)
        assert_eq!(
            calendar().add_business_days(date(2026, 8, 10), 1),
            date(2026, 8, 11)
        );
    }
}
