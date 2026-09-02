//! Отсутствующая запись объявлена отдельным ответом (G-034, G-035).
//!
//! Пустой перечень на месте несуществующего родителя и отказ по правилу на
//! месте несуществующей записи одинаково вводят в заблуждение: по первому
//! клиент не отличает «такого тендера нет» от «у тендера пусто» и рисует
//! пустую вкладку, по второму получает 409 на опечатку в идентификаторе и
//! ищет нарушение правила там, где нарушать нечего.
//!
//! Перечень ниже - те самые операции, на которых это было найдено обходом
//! контракта. Тест держит их ответ объявленным: пока 404 стоит в контракте,
//! кодоген отдает клиенту разбор этого случая, а обработчик, потерявший
//! проверку родителя, расходится с собственным контрактом.

use serde_json::Value;

/// Операции, у которых отсутствующая запись обязана быть отдельным ответом.
const MUST_DECLARE_NOT_FOUND: &[(&str, &str)] = &[
    // G-034: подресурс родителя
    ("/api/v1/tenders/{id}/amendments", "get"),
    ("/api/v1/tenders/{id}/applications", "get"),
    ("/api/v1/tenders/{id}/protocols", "get"),
    ("/api/v1/tenders/{id}/journal", "get"),
    ("/api/v1/tenders/{id}/contracts", "get"),
    ("/api/v1/tenders/{id}/dossier", "get"),
    ("/api/v1/tenders/{id}/evasions", "get"),
    ("/api/v1/ledger/accounts/{id}/entries", "get"),
    ("/api/v1/special-requests/{id}/dossier", "get"),
    // G-035: мутация конкретной записи
    ("/api/v1/applications/{id}/withdraw", "post"),
    ("/api/v1/applications/{id}/decline-amendment", "post"),
    ("/api/v1/lots/{id}/contract", "post"),
    ("/api/v1/contracts/{id}/deposit/refund", "post"),
    ("/api/v1/tenders/{id}/meeting/open", "post"),
];

/// Ключ пути в JSON Pointer: `~` и `/` в нем экранируются (RFC 6901),
/// иначе сегменты адреса развалятся по слэшам самого пути.
fn pointer_key(path: &str) -> String {
    path.replace('~', "~0").replace('/', "~1")
}

#[test]
fn operations_on_a_missing_record_declare_not_found() {
    let json = tou_http::openapi()
        .to_json()
        .expect("сериализация контракта");
    let doc: Value = serde_json::from_str(&json).expect("парсинг контракта");

    for (path, method) in MUST_DECLARE_NOT_FOUND {
        let pointer = format!("/paths/{}/{method}/responses", pointer_key(path));
        let responses = doc
            .pointer(&pointer)
            .unwrap_or_else(|| panic!("{method} {path}: операции нет в контракте"));

        assert!(
            responses.get("404").is_some(),
            "{method} {path}: 404 не объявлен - отсутствующая запись отвечает \
             пустым перечнем либо отказом по правилу, и клиенту нечем их различить"
        );
    }
}
