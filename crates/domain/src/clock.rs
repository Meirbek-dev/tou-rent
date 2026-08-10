use jiff::Timestamp;

/// Источник времени для расчетов домена (регламент А.5): прямые вызовы
/// системных часов запрещены гейтом G2 (`clippy.toml`).
///
/// Реализации на реальных часах здесь нет намеренно. Юридически значимое
/// время в этой системе ставит БД (NFR-03), и с ADR-0005 у нее единственный
/// источник: функция `core.now()`, которую двигает сдвиг часов стенда. Часы
/// процесса разошлись бы с ней, и правила считали бы один день, а отметка
/// в документе стояла бы другим. Поэтому в Rust время либо приходит из БД
/// (`tou_db::refdata::now`), либо задается явно: [`FixedClock`] в тестах
/// и воспроизводимых расчетах.
pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

/// Фиксированные часы для тестов и воспроизводимых расчетов.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub Timestamp);

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_returns_configured_instant() {
        let ts: Timestamp = "2026-01-30T00:00:00Z".parse().unwrap();
        assert_eq!(FixedClock(ts).now(), ts);
    }
}
