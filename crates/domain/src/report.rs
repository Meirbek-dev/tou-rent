//! Отчетность: реестры решений, договоров и поступлений (арх. § 9, контур 3).
//!
//! Требования в ТЗ на отчетность нет (Q-012), поэтому реестр здесь - не
//! придуманная форма с реквизитами и подписями, а выборка того, что система
//! уже хранит: строки, колонки и период. Что именно подшивать в отчет и кому
//! он адресован, решает заказчик - до ответа система отдает данные, а не
//! «утвержденную форму» (A-079).
//!
//! Выгрузка - CSV: он открывается и в Excel, и в 1С, и не требует изобретать
//! верстку. Разделитель - точка с запятой, кодировка UTF-8 с BOM: в русской
//! локали Excel иначе разбирает файл по-своему.

use serde::{Deserialize, Serialize};

/// Разделитель CSV: русская локаль Excel ждет точку с запятой (A-079).
pub const CSV_DELIMITER: char = ';';
/// BOM UTF-8: без него Excel читает кириллицу как cp1251.
pub const CSV_BOM: &str = "\u{feff}";

/// Реестр отчетности (арх. § 9). Перечень закрыт: реестр - это выборка
/// существующих фактов, а не новая предметная сущность.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Registry {
    /// Решения Правления: особый порядок (п. 90) и земельные участки (п. 106)
    Decisions,
    /// Договоры найма всех оснований (п. 126)
    Contracts,
    /// Поступления депозитной книги: подтвержденные взносы и восполнения (FR-1001)
    Receipts,
}

impl Registry {
    pub const ALL: [Registry; 3] = [Registry::Decisions, Registry::Contracts, Registry::Receipts];

    pub fn as_str(self) -> &'static str {
        match self {
            Registry::Decisions => "decisions",
            Registry::Contracts => "contracts",
            Registry::Receipts => "receipts",
        }
    }

    /// Название реестра (ru - делопроизводство, NFR-01).
    pub fn title_ru(self) -> &'static str {
        match self {
            Registry::Decisions => "Реестр решений",
            Registry::Contracts => "Реестр договоров",
            Registry::Receipts => "Реестр поступлений",
        }
    }

    /// Колонки реестра: заголовок строки CSV и шапка таблицы кабинета.
    /// Порядок колонок - часть контракта: строки собираются по нему.
    pub fn columns(self) -> &'static [&'static str] {
        match self {
            Registry::Decisions => &[
                "Дата решения",
                "Порядок",
                "Предмет",
                "Заявитель",
                "Решение",
                "Обоснование",
            ],
            Registry::Contracts => &[
                "Рег. номер",
                "Дата регистрации",
                "Объект",
                "Наниматель",
                "Ставка в месяц, ₸",
                "Период найма",
                "Состояние",
                "Основание",
            ],
            Registry::Receipts => &[
                "Дата",
                "Счет",
                "Плательщик",
                "Сумма, ₸",
                "Основание",
                "Подтвердил",
            ],
        }
    }

    /// Имя файла выгрузки (латиница: файл уезжает во внешние системы).
    pub fn file_name(self) -> &'static str {
        match self {
            Registry::Decisions => "registry-decisions.csv",
            Registry::Contracts => "registry-contracts.csv",
            Registry::Receipts => "registry-receipts.csv",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("неизвестный реестр: {0}")]
pub struct UnknownRegistry(pub String);

impl std::str::FromStr for Registry {
    type Err = UnknownRegistry;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Registry::ALL
            .into_iter()
            .find(|registry| registry.as_str() == s)
            .ok_or_else(|| UnknownRegistry(s.to_owned()))
    }
}

/// Экранирование значения CSV: кавычки удваиваются, а значение берется
/// в кавычки, если содержит разделитель, кавычку или перевод строки.
fn escape(value: &str) -> String {
    let needs_quotes = value.contains(CSV_DELIMITER)
        || value.contains('"')
        || value.contains('\n')
        || value.contains('\r');
    if needs_quotes {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

/// Выгрузка реестра в CSV (арх. § 9): шапка из [`Registry::columns`]
/// и строки в том же порядке колонок.
pub fn to_csv(registry: Registry, rows: &[Vec<String>]) -> String {
    let mut out = String::from(CSV_BOM);
    out.push_str(
        &registry
            .columns()
            .iter()
            .map(|column| escape(column))
            .collect::<Vec<_>>()
            .join(&CSV_DELIMITER.to_string()),
    );
    out.push_str("\r\n");

    for row in rows {
        out.push_str(
            &row.iter()
                .map(|value| escape(value))
                .collect::<Vec<_>>()
                .join(&CSV_DELIMITER.to_string()),
        );
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registries_have_stable_names_columns_and_files() {
        let mut names = std::collections::BTreeSet::new();
        let mut files = std::collections::BTreeSet::new();
        for registry in Registry::ALL {
            assert_eq!(registry.as_str().parse::<Registry>(), Ok(registry));
            assert!(names.insert(registry.as_str()));
            assert!(files.insert(registry.file_name()));
            assert!(registry.file_name().is_ascii(), "имя файла - латиница");
            assert!(!registry.title_ru().is_empty());
            assert!(
                registry.columns().len() >= 4,
                "{registry:?}: реестр без колонок бесполезен"
            );
        }
        assert_eq!(
            "profit".parse::<Registry>(),
            Err(UnknownRegistry("profit".to_owned()))
        );
    }

    #[test]
    fn csv_starts_with_the_header_and_keeps_column_order() {
        let rows = vec![vec![
            "Д-1".to_owned(),
            "08.08.2026".to_owned(),
            "Помещение".to_owned(),
            "ТОО «Наниматель»".to_owned(),
            "79 750,00".to_owned(),
            "01.09.2026 - 31.08.2027".to_owned(),
            "действует".to_owned(),
            "тендер".to_owned(),
        ]];
        let csv = to_csv(Registry::Contracts, &rows);

        assert!(csv.starts_with(CSV_BOM), "BOM нужен Excel в русской локали");
        let lines: Vec<&str> = csv.trim_end().split("\r\n").collect();
        assert_eq!(lines.len(), 2, "шапка и одна строка");
        assert!(lines[0].contains("Рег. номер"));
        assert!(lines[1].starts_with("Д-1;"));
        assert_eq!(
            lines[0].matches(CSV_DELIMITER).count(),
            Registry::Contracts.columns().len() - 1
        );
    }

    #[test]
    fn csv_escapes_separators_quotes_and_newlines() {
        let rows = vec![vec![
            "значение; с разделителем".to_owned(),
            "кавычка \" внутри".to_owned(),
            "две\nстроки".to_owned(),
            "обычное".to_owned(),
            "0,00".to_owned(),
            "-".to_owned(),
        ]];
        let csv = to_csv(Registry::Receipts, &rows);

        assert!(csv.contains("\"значение; с разделителем\""));
        assert!(csv.contains("\"кавычка \"\" внутри\""));
        assert!(csv.contains("\"две\nстроки\""));
        assert!(
            csv.contains(";обычное;"),
            "значение без спецсимволов не берется в кавычки"
        );
    }

    #[test]
    fn empty_registry_is_still_a_valid_file() {
        // Пустой период - это ответ «поступлений нет», а не ошибка
        let csv = to_csv(Registry::Receipts, &[]);
        let lines: Vec<&str> = csv.trim_end().split("\r\n").collect();
        assert_eq!(lines.len(), 1, "остается только шапка");
    }
}
