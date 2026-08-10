//! Гейт G16: трассировка инвариантов (арх. § 8).
//!
//! Каждый INV-### из `specs/INVENTORY.md` обязан быть упомянут там, где он
//! закреплен: в миграции (constraint/триггер), в типе домена или в тесте.
//! Инвариант, о котором знает только таблица в спеках, - это намерение,
//! а не правило системы; такой пайплайн красный.
//!
//! Тест читает файлы репозитория, поэтому база ему не нужна.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Идентификаторы вида `INV-021`, `INV-DB-05`, `INV-A01`, `INV-POL-01`.
fn invariant_ids(text: &str) -> BTreeSet<String> {
    let bytes = text.as_bytes();
    let mut ids = BTreeSet::new();
    let mut index = 0;

    while let Some(found) = text[index..].find("INV-") {
        let start = index + found;
        let mut end = start + "INV-".len();
        while end < bytes.len() {
            let ch = bytes[end] as char;
            if ch.is_ascii_alphanumeric() || ch == '-' {
                end += 1;
            } else {
                break;
            }
        }
        // Хвостовой дефис принадлежит тексту, а не идентификатору
        let id = text[start..end].trim_end_matches('-');
        // Идентификатор кончается номером: `INV-021`, `INV-DB-05`,
        // `INV-POL-01`. Собирательные пометки вроде `INV-AUDIT` и обрывки
        // шаблонов (`INV-DB-*`) инвариантами не являются.
        let numbered = id
            .rsplit('-')
            .next()
            .is_some_and(|tail| !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()));
        if numbered {
            ids.insert(id.to_owned());
        }
        index = end.max(start + 1);
    }
    ids
}

/// Инварианты из таблицы реестра (первая колонка строк вида `| INV-… |`).
fn inventory_invariants() -> BTreeSet<String> {
    let inventory = include_str!("../../../specs/INVENTORY.md");
    inventory
        .lines()
        .filter(|line| line.starts_with("| INV-"))
        .flat_map(invariant_ids)
        .collect()
}

/// Корень репозитория: тест лежит в `crates/testkit/tests`.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .to_path_buf()
}

/// Файлы, в которых инвариант считается закрепленным: миграции, домен,
/// слой данных и тесты.
fn sources() -> Vec<PathBuf> {
    let root = repo_root();
    let mut files = Vec::new();
    for dir in [
        root.join("crates/db/migrations"),
        root.join("crates/domain/src"),
        root.join("crates/db/src"),
        root.join("crates/http/src"),
        root.join("crates/testkit/tests"),
        root.join("crates/domain/tests"),
        root.join("crates/db/tests"),
    ] {
        collect(&dir, &mut files);
    }
    files
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, files);
        } else if matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("rs") | Some("sql")
        ) {
            files.push(path);
        }
    }
}

/// G16: каждый инвариант реестра упомянут в коде, закрепляющем его.
#[test]
fn g16_every_invariant_is_traceable() {
    let expected = inventory_invariants();
    assert!(
        expected.len() >= 10,
        "реестр инвариантов прочитан: {expected:?}"
    );

    let mut mentions: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in sources() {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let file = path
            .strip_prefix(repo_root())
            .unwrap_or(&path)
            .display()
            .to_string();
        for id in invariant_ids(&text) {
            mentions.entry(id).or_default().push(file.clone());
        }
    }

    let untraceable: Vec<&String> = expected
        .iter()
        .filter(|id| !mentions.contains_key(*id))
        .collect();
    assert!(
        untraceable.is_empty(),
        "инварианты без закрепления в коде (G16): {untraceable:?}"
    );
}

/// Обратная проверка: инвариант, упомянутый в коде, заведен в реестре -
/// иначе спеки отстают от системы.
#[test]
fn g16_code_invariants_are_registered() {
    let known = inventory_invariants();

    let mut unknown: BTreeSet<String> = BTreeSet::new();
    for path in sources() {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for id in invariant_ids(&text) {
            if !known.contains(&id) {
                unknown.insert(id);
            }
        }
    }

    assert!(
        unknown.is_empty(),
        "инварианты из кода отсутствуют в specs/INVENTORY.md: {unknown:?}"
    );
}

#[cfg(test)]
mod tests {
    use super::invariant_ids;

    #[test]
    fn ids_are_parsed_from_prose() {
        let ids = invariant_ids(
            "текст INV-021, (INV-DB-05) и INV-POL-01. Перечень INV-AUDIT, шаблон INV-DB-*, хвост INV-",
        );
        let mut ids: Vec<String> = ids.into_iter().collect();
        ids.sort();
        assert_eq!(
            ids,
            ["INV-021", "INV-DB-05", "INV-POL-01"],
            "собирательные пометки инвариантами не считаются"
        );
    }
}
