//! Гейт G16 (вторая половина): трассировка требований (T44, T79).
//!
//! Контур принимается, когда каждое FR контура закрыто тестом с привязкой
//! к своему ID (ТЗ § 10а). Тест читает ТЗ и требует, чтобы каждое
//! требование упоминалось хотя бы в одном тесте репозитория - доменном,
//! testkit или e2e.
//!
//! Требования берутся **из всех контуров**, а не только из третьего.
//! Пока фильтр стоял на контуре 3, вне гейта оставались FR-402 (журнал
//! регистрации) и FR-404 (отзыв заявки) - ядро контура 1: тесты на них
//! были, но привязки к ID не имели, и пропажа такого теста прошла бы
//! молча. К ним добавлены требования ТЗ v2 (FR-19xx, NFR-11…NFR-16) -
//! условие приемки контура 4 (ТЗ v2 § 8а).
//!
//! Тест читает файлы репозитория, поэтому база ему не нужна.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Требования, оставленные без автотеста осознанно. Пропуск виден здесь,
/// а не в молчаливом отсутствии теста; причина - часть записи.
///
/// Инфраструктурные NFR доказываются прогоном на живом стенде, а не
/// тестом в репозитории: их «тест» - это протокол приемки (ТЗ v2 § 8б-г).
/// Когда задача закрыта и артефакт приложен, строка отсюда убирается.
const SKIPPED: [(&str, &str); 6] = [
    (
        "FR-1303",
        "T41 (внешние каналы уведомлений) пропущена по решению заказчика",
    ),
    (
        "NFR-06",
        "T51: бэкап и учебное восстановление - протокол прогона, не тест",
    ),
    (
        "NFR-10",
        "T48-T49: деплой на хост - смоук-чек-лист прогона, не тест",
    ),
    (
        "NFR-11",
        "T45: доказательность гейтов - ссылка на прогон пайплайна",
    ),
    (
        "NFR-13",
        "T51: восстановление из бэкапа - протокол учебного восстановления",
    ),
    (
        "NFR-14",
        "T59: нагрузка - отчет прогона на прод-подобном стенде",
    ),
];

/// Корень репозитория: тест лежит в `crates/testkit/tests`.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .to_path_buf()
}

/// Требования из таблиц ТЗ.
///
/// v1: строки вида `| FR-1206 | 3 | …` - берутся все контуры, номер контура
/// определяет только очередность поставки, а не нужность теста.
/// v2 (§ 4.4): строки вида `| FR-1901 | … ` и `| NFR-12 | …` - у них колонки
/// контура нет, поэтому признак другой: идентификатор в первой ячейке.
fn requirements() -> BTreeSet<String> {
    let mut ids: BTreeSet<String> = BTreeSet::new();

    let v1 = include_str!("../../../docs/tou-rent-tz-v1.md");
    for line in v1.lines() {
        let mut cells = line.split('|').map(str::trim);
        cells.next(); // пустая ячейка перед первым разделителем
        let Some(id) = cells.next() else { continue };
        let Some(loop_number) = cells.next() else {
            continue;
        };
        if id.starts_with("FR-") && matches!(loop_number, "1" | "2" | "3") {
            ids.insert(id.to_owned());
        }
    }

    // Нефункциональные требования v1 (§ 5) - таблица из двух колонок,
    // номера контура у них нет: качества системы сквозные
    for line in v1.lines() {
        let mut cells = line.split('|').map(str::trim);
        cells.next();
        let Some(id) = cells.next() else { continue };
        if id.starts_with("NFR-") && id[4..].parse::<u32>().is_ok() {
            ids.insert(id.to_owned());
        }
    }

    let v2 = include_str!("../../../docs/tou-rent-tz-v2.md");
    for line in v2.lines() {
        let mut cells = line.split('|').map(str::trim);
        cells.next();
        let Some(id) = cells.next() else { continue };
        if is_new_requirement(id) {
            ids.insert(id.to_owned());
        }
    }

    ids
}

/// Идентификатор требования, введенного ТЗ v2: `FR-19xx` (администрирование)
/// и `NFR-11`…`NFR-16`. Прочие упоминания в таблицах v2 - ссылки на уже
/// существующие требования, их гейт берет из v1.
fn is_new_requirement(id: &str) -> bool {
    if let Some(number) = id.strip_prefix("FR-") {
        return number.len() == 4
            && number.starts_with("19")
            && number.chars().all(|c| c.is_ascii_digit());
    }
    if let Some(number) = id.strip_prefix("NFR-") {
        return matches!(number.parse::<u32>(), Ok(11..=16));
    }
    false
}

/// Файлы тестов: доменные модульные тесты живут рядом с кодом, поэтому
/// в выборку идут и они - привязка к ID важнее места.
fn test_files() -> Vec<PathBuf> {
    let root = repo_root();
    let mut files = Vec::new();
    for dir in [
        root.join("crates/testkit/tests"),
        root.join("crates/domain/src"),
        root.join("crates/domain/tests"),
        root.join("crates/http/src"),
        root.join("apps/e2e/tests"),
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
        } else if path
            .extension()
            .is_some_and(|ext| ext == "rs" || ext == "ts")
        {
            files.push(path);
        }
    }
}

/// Упоминания FR-ID в тексте с указанием файла.
fn mentions() -> BTreeMap<String, BTreeSet<String>> {
    let root = repo_root();
    let mut found: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for file in test_files() {
        // Сам гейт покрытием не считается: перечень SKIPPED упоминает ID
        if file.ends_with("fr_traceability.rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        // Тест засчитывается, только если в нем есть проверки: комментарий
        // «FR-…» в коде без утверждений покрытием не считается
        let is_test = text.contains("#[test]")
            || text.contains("#[tokio::test]")
            || text.contains("test(")
            || text.contains("assert");
        if !is_test {
            continue;
        }

        let name = file
            .strip_prefix(&root)
            .unwrap_or(&file)
            .display()
            .to_string();
        for id in requirement_ids(&text) {
            found.entry(id).or_default().insert(name.clone());
        }
    }
    found
}

/// Идентификаторы вида `FR-1206` и `NFR-12` в тексте.
///
/// Граница слева проверяется намеренно: `NFR-12` содержит `FR-12` как
/// подстроку, и наивный поиск засчитывал бы упоминание нефункционального
/// требования как покрытие функционального.
fn requirement_ids(text: &str) -> BTreeSet<String> {
    let bytes = text.as_bytes();
    let mut ids = BTreeSet::new();

    for (start, _) in text.match_indices("FR-") {
        let boundary_ok = start == 0
            || !bytes[start - 1].is_ascii_alphanumeric() && bytes[start - 1] != b'_'
            || bytes[start - 1] == b'N';
        if !boundary_ok {
            continue;
        }
        // `NFR-` начинается на один символ левее
        let (id_start, prefix_len) = if start > 0 && bytes[start - 1] == b'N' {
            (start - 1, "NFR-".len())
        } else {
            (start, "FR-".len())
        };
        let mut end = id_start + prefix_len;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end > id_start + prefix_len {
            ids.insert(text[id_start..end].to_owned());
        }
    }
    ids
}

/// G16: каждое требование ТЗ закрыто тестом с привязкой к ID (ТЗ § 10а).
#[test]
fn requirements_are_covered_by_tests() {
    let required = requirements();
    assert!(
        required.len() >= 80,
        "в ТЗ должны быть требования всех контуров, найдено {}",
        required.len()
    );

    let covered = mentions();
    let skipped: BTreeMap<&str, &str> = SKIPPED.into_iter().collect();

    let missing: Vec<String> = required
        .iter()
        .filter(|id| !covered.contains_key(*id) && !skipped.contains_key(id.as_str()))
        .cloned()
        .collect();

    assert!(
        missing.is_empty(),
        "требования без тестов с привязкой к ID: {missing:?}"
    );
}

/// Пропущенное требование остается пропущенным осознанно: если тест на него
/// появился, запись в `SKIPPED` пора убирать.
#[test]
fn skipped_requirements_stay_declared() {
    let required = requirements();
    let covered = mentions();

    for (id, reason) in SKIPPED {
        assert!(
            required.contains(id),
            "{id} не встречается в ТЗ - запись в SKIPPED лишняя"
        );
        assert!(
            !covered.contains_key(id),
            "{id} уже закрыт тестами ({reason}) - уберите его из SKIPPED"
        );
    }
}

/// Разбор идентификаторов: `NFR-12` не должен читаться как `FR-12`.
#[test]
fn nfr_is_not_mistaken_for_fr() {
    let ids = requirement_ids("покрывает NFR-12 и FR-601");
    assert!(ids.contains("NFR-12"), "NFR-12 не распознан: {ids:?}");
    assert!(ids.contains("FR-601"));
    assert!(
        !ids.contains("FR-12"),
        "хвост NFR-12 засчитан как FR-12: {ids:?}"
    );
}
