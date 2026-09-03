//! «Еще нет» - это состояние, а не отсутствующий адрес (G-067; NFR-01).
//!
//! Четыре операции кодировали ненаступившее состояние процедуры кодом 404:
//! незаназначенное заседание допуска, несформированный протокол допуска,
//! невнесенный депозит договора и неприменённая льгота. Кабинет спрашивает
//! их на каждой загрузке карточки, и каждая загрузка писала ошибку в
//! консоль - у секретаря по две на вкладку, у нанимателя по шесть за визит.
//! Отличить «такого договора нет» от «депозит еще не вносился» клиенту было
//! нечем, и обе ветки выглядели поломкой.
//!
//! Ответ у них теперь один - 200 с телом `<Dto> | null`, а 404 остается за
//! родителем: несуществующий тендер и несуществующий договор по-прежнему
//! отдельный ответ (парный тест - `crates/http/tests/parent_not_found.rs`).
//!
//! Тест читает сам контракт: ответ объявлен в нем, и обработчик, потерявший
//! проверку родителя либо вернувший 404 на пустое состояние, расходится с
//! собственным контрактом раньше, чем это увидит браузер.

use serde_json::Value;

/// Операции и схема их тела: «еще нет» - `null`, родителя нет - 404.
const NULLABLE_SUBENTITIES: &[(&str, &str, &str)] = &[
    ("/api/v1/tenders/{id}/meeting", "get", "MeetingDto"),
    (
        "/api/v1/tenders/{id}/admission-protocol",
        "get",
        "GeneratedProtocolDto",
    ),
    ("/api/v1/contracts/{id}/deposit", "get", "LedgerAccountDto"),
    ("/api/v1/contracts/{id}/benefit", "get", "BenefitGrantDto"),
];

/// Ключ пути в JSON Pointer: `~` и `/` в нем экранируются (RFC 6901),
/// иначе сегменты адреса развалятся по слэшам самого пути.
fn pointer_key(path: &str) -> String {
    path.replace('~', "~0").replace('/', "~1")
}

#[test]
fn a_missing_sub_entity_is_a_null_body_and_not_a_missing_address() {
    let json = tou_http::openapi()
        .to_json()
        .expect("сериализация контракта");
    let doc: Value = serde_json::from_str(&json).expect("парсинг контракта");

    for (path, method, dto) in NULLABLE_SUBENTITIES {
        let key = pointer_key(path);
        let responses = doc
            .pointer(&format!("/paths/{key}/{method}/responses"))
            .unwrap_or_else(|| panic!("{method} {path}: операции нет в контракте"));

        let schema = responses
            .pointer("/200/content/application~1json/schema")
            .unwrap_or_else(|| panic!("{method} {path}: у ответа 200 нет тела"));
        let schema = schema.to_string();

        assert!(
            schema.contains("\"null\""),
            "{method} {path}: тело ответа не допускает `null` - \
             ненаступившее состояние процедуры снова кодируется кодом ответа: {schema}"
        );
        assert!(
            schema.contains(&format!("#/components/schemas/{dto}")),
            "{method} {path}: в теле ответа нет схемы {dto}: {schema}"
        );

        assert!(
            responses.get("404").is_some(),
            "{method} {path}: 404 не объявлен - несуществующий родитель обязан \
             оставаться отдельным ответом"
        );
    }
}

/// Льгота договора видна не всякому, кто видит договор (FR-1205, п. 95–96):
/// перечень прав - `Compound::BENEFIT_READ`, и 403 у секретаря остается
/// объявленным ответом, а не неожиданностью для интерфейса.
#[test]
fn the_benefit_declares_the_refusal_by_role() {
    let json = tou_http::openapi()
        .to_json()
        .expect("сериализация контракта");
    let doc: Value = serde_json::from_str(&json).expect("парсинг контракта");

    let responses = doc
        .pointer(&format!(
            "/paths/{}/get/responses",
            pointer_key("/api/v1/contracts/{id}/benefit")
        ))
        .expect("операции нет в контракте");
    assert!(
        responses.get("403").is_some(),
        "403 не объявлен - роль без права на льготу получает необъявленный ответ"
    );
}
