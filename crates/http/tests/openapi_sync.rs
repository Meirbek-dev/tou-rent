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
