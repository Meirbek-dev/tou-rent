//! Суть гейта G5 «кодоген без диффа», выполнимая обычным `cargo test`:
//! закоммиченный `packages/api-client/openapi.json` обязан совпадать
//! с контрактом из кода. Разошлось - перегенерируй:
//!   контейнер: cargo run -q -p api -- openapi > packages/api-client/openapi.json
//!   затем:     vp run codegen  (openapi-typescript → schema.d.ts)

const COMMITTED: &str = include_str!("../../../packages/api-client/openapi.json");

#[test]
fn committed_openapi_matches_code() {
    let committed: serde_json::Value =
        serde_json::from_str(COMMITTED).expect("packages/api-client/openapi.json не парсится");
    let generated_json = tou_http::openapi()
        .to_json()
        .expect("сериализация контракта");
    let generated: serde_json::Value =
        serde_json::from_str(&generated_json).expect("парсинг сгенерированного контракта");

    assert_eq!(
        committed, generated,
        "G5: openapi.json разошелся с кодом - перегенерируй кодоген (см. заголовок файла)"
    );
}

/// OpenAPI 3.1 requires a description even when a response has no body.
#[test]
fn every_inline_response_has_a_description() {
    let generated: serde_json::Value =
        serde_json::from_str(&tou_http::openapi().to_json().expect("OpenAPI serializes"))
            .expect("OpenAPI parses");
    for (path, item) in generated["paths"].as_object().expect("paths") {
        for (method, operation) in item.as_object().expect("path item") {
            if let Some(responses) = operation
                .get("responses")
                .and_then(serde_json::Value::as_object)
            {
                for (status, response) in responses {
                    assert!(
                        response.get("$ref").is_some() || response["description"].is_string(),
                        "{method} {path} {status} must describe its response"
                    );
                }
            }
        }
    }
}
