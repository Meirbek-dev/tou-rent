//! Тест паритета typestate ↔ БД (INV-021, FR-302, арх. § 5).
//!
//! Сверяет `tou_domain::tender::TRANSITIONS` (порожден тем же макросом,
//! что и typestate-методы) с seed-миграцией `refdata.tender_status_transitions`.
//! Миграция - источник наполнения таблицы, поэтому паритет с файлом равен
//! паритету с БД. Живой вариант против PostgreSQL добавит testkit (G8/G10-стенд).

use std::collections::BTreeSet;

use tou_domain::tender::TRANSITIONS;

const SEED_SQL: &str = include_str!("../migrations/20260806100015_refdata_seed.sql");

/// Пары `('from', 'to')` из INSERT-блока таблицы переходов.
fn seed_transitions() -> BTreeSet<(String, String)> {
    let insert_block = SEED_SQL
        .split("INSERT INTO refdata.tender_status_transitions")
        .nth(1)
        .and_then(|rest| rest.split("ON CONFLICT").next())
        .unwrap_or_default();

    insert_block
        .lines()
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
