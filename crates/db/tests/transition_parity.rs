//! Тест паритета typestate ↔ БД (INV-021, FR-302, арх. § 5).
//!
//! Сверяет `tou_domain::tender::TRANSITIONS` (порожден тем же макросом,
//! что и typestate-методы) со всеми миграциями, добавляющими строки в
//! `refdata.tender_status_transitions`. Уже примененные миграции неизменяемы,
//! поэтому новые переходы добавляются отдельным файлом.

use std::collections::BTreeSet;

use tou_domain::tender::TRANSITIONS;

const SEED_SQL: &str = include_str!("../migrations/20260806100015_refdata_seed.sql");
const ADMIN_OUTCOME_SQL: &str =
    include_str!("../migrations/20260921120000_admin_successful_outcome.sql");

/// Пары `('from', 'to')` из INSERT-блока таблицы переходов.
fn seed_transitions() -> BTreeSet<(String, String)> {
    [SEED_SQL, ADMIN_OUTCOME_SQL]
        .into_iter()
        .flat_map(|sql| {
            sql.split("INSERT INTO refdata.tender_status_transitions")
                .skip(1)
                .filter_map(|rest| rest.split("ON CONFLICT").next())
                .flat_map(str::lines)
        })
        .filter_map(|line| {
            let mut quoted = line.split('\'');
            let from = quoted.nth(1)?;
            let to = quoted.nth(1)?;
            Some((from.to_string(), to.to_string()))
        })
        .collect()
}

#[test]
fn typestate_transitions_match_db_seed() {
    let from_rust: BTreeSet<(String, String)> = TRANSITIONS
        .iter()
        .map(|(from, to)| (from.as_str().to_string(), to.as_str().to_string()))
        .collect();
    let from_seed = seed_transitions();

    assert!(!from_seed.is_empty(), "seed-блок переходов не распарсился");
    assert_eq!(
        from_rust,
        from_seed,
        "INV-021: перечни переходов Rust и refdata-seed разошлись\n\
         только в Rust: {:?}\nтолько в seed: {:?}",
        from_rust.difference(&from_seed).collect::<Vec<_>>(),
        from_seed.difference(&from_rust).collect::<Vec<_>>(),
    );
}

#[test]
fn typestate_has_no_duplicate_pairs() {
    let unique: BTreeSet<_> = TRANSITIONS.iter().collect();
    assert_eq!(unique.len(), TRANSITIONS.len());
}
