//! HTTP-слой (арх. § 4): роутеры, экстракторы, OpenAPI-контракт, middleware, SSE/WS.
//!
//! Конвенции API (ТЗ § 7): REST под `/api/v1`, kebab-case пути,
//! cursor-пагинация, идемпотентность мутаций через `Idempotency-Key`,
//! ошибки - RFC 9457 problem+json с машинным `code`.

pub mod acts;
pub mod admin;
pub mod admission;
pub mod amendments;
pub mod announcement;
pub mod applications;
pub mod auctions;
pub mod auth;
pub mod benefit;
pub mod commission;
pub mod contract_amendments;
pub mod contracts;
pub mod csrf;
pub mod dto;
pub mod error;
pub mod evasion;
pub mod extract;
pub mod failure;
pub mod investment;
pub mod land;
pub mod ledger;
pub mod notifications;
pub mod objects;
pub mod obligations;
pub mod oidc;
pub mod pdf;
pub mod public_records;
pub mod publications;
pub mod ratelimit;
pub mod rates;
pub mod realtime;
pub mod refdata;
pub mod reports;
pub mod request;
pub mod results;
pub mod special;
pub mod state;
pub mod storage;
pub mod tenders;
pub mod timeout;

use std::sync::LazyLock;

use axum::Router;
use axum::middleware;
use axum::routing::get;
use time::Duration;
use tower_sessions::cookie::SameSite;
use tower_sessions::{Expiry, SessionManagerLayer};
use tower_sessions_redis_store::RedisStore;
use tower_sessions_redis_store::fred::prelude::Pool;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub use state::AppState;

/// Базовые схемы контракта; пути и схемы операций собирает [`api_router`] -
/// маршрут без записи в OpenAPI (и наоборот) невозможен по построению.
#[derive(OpenApi)]
#[openapi(components(schemas(error::Problem, error::ErrorCode)))]
struct ApiDoc;

#[utoipa::path(
    get,
    path = "/api/v1/healthz",
    tag = "system",
    responses((status = 200, description = "Сервис жив", body = str))
)]
async fn healthz() -> &'static str {
    "ok"
}

/// Единый реестр маршрутов (арх. § 7): каждый `routes!` кладет хендлер
/// и в axum-роутер, и в OpenAPI-документ. CSRF double-submit накладывается
/// целиком (арх. § 5): безопасные методы и дологинные пути исключает
/// `csrf::enforce`, любая новая мутация защищена по умолчанию.
fn api_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(healthz))
        .routes(routes!(auth::register))
        .routes(routes!(auth::login))
        .routes(routes!(auth::logout))
        .routes(routes!(auth::me))
        .routes(routes!(oidc::auth_providers))
        .routes(routes!(oidc::oidc_login))
        .routes(routes!(oidc::oidc_callback))
        .routes(routes!(oidc::oidc_logout))
        .routes(routes!(admin::list_users))
        .routes(routes!(admin::grant_role))
        .routes(routes!(admin::revoke_role))
        .routes(routes!(objects::list_objects, objects::create_object))
        .routes(routes!(
            objects::get_object,
            objects::update_object,
            objects::delete_object
        ))
        .routes(routes!(tenders::list_tenders, tenders::create_tender))
        .routes(routes!(tenders::get_tender, tenders::update_tender))
        .routes(routes!(tenders::set_recording))
        .routes(routes!(tenders::publish_tender))
        .routes(routes!(tenders::open_acceptance))
        .routes(routes!(amendments::cancel_tender))
        .routes(routes!(amendments::cancel_lot))
        .routes(routes!(
            amendments::tender_amendments,
            amendments::amend_tender
        ))
        .routes(routes!(amendments::amendment_pdf))
        .routes(routes!(amendments::decline_amendment))
        .routes(routes!(announcement::announcement_pdf))
        .routes(routes!(rates::preview))
        .routes(routes!(rates::preview_hourly))
        .routes(routes!(rates::options))
        .routes(routes!(
            applications::submit_application,
            applications::tender_applications
        ))
        .routes(routes!(applications::my_applications))
        .routes(routes!(applications::withdraw_application))
        .routes(routes!(applications::upload_file))
        .routes(routes!(applications::download_file))
        .routes(routes!(applications::tender_journal))
        .routes(routes!(admission::open_tender))
        .routes(routes!(admission::qualification_meeting))
        .routes(routes!(commission::active_commission))
        .routes(routes!(commission::approve_commission))
        .routes(routes!(commission::record_attendance))
        .routes(routes!(commission::open_meeting))
        .routes(routes!(commission::declare_coi))
        .routes(routes!(commission::recuse_member))
        .routes(routes!(commission::cast_vote))
        .routes(routes!(commission::application_votes))
        .routes(routes!(admission::decide_application))
        .routes(routes!(admission::rejection_reasons))
        .routes(routes!(
            admission::generate_admission_protocol,
            admission::admission_protocol_meta
        ))
        .routes(routes!(admission::admission_protocol_pdf))
        .routes(routes!(admission::notify_admitted))
        .routes(routes!(ledger::list_accounts))
        .routes(routes!(ledger::account_entries))
        .routes(routes!(ledger::confirm_fee, ledger::application_account))
        .routes(routes!(ledger::refund_fee))
        .routes(routes!(ledger::refund_reasons))
        // GET и POST по одному пути - один `routes!` (FR-1003, п. 132)
        .routes(routes!(ledger::contract_deposit, ledger::pay_deposit))
        .routes(routes!(ledger::refund_deposit))
        .routes(routes!(refdata::list_mrp))
        .routes(routes!(refdata::set_mrp))
        .routes(routes!(
            refdata::list_coefficients,
            refdata::add_coefficient
        ))
        .routes(routes!(obligations::my_obligations))
        .routes(routes!(
            obligations::list_holidays,
            obligations::add_holiday
        ))
        .routes(routes!(obligations::remove_holiday))
        .routes(routes!(notifications::list_notifications))
        .routes(routes!(notifications::unread_count))
        .routes(routes!(notifications::mark_read))
        .routes(routes!(notifications::stream))
        .routes(routes!(auctions::schedule_auction, auctions::lot_auction))
        .routes(routes!(auctions::auction_room))
        .routes(routes!(auctions::start_auction))
        .routes(routes!(auctions::extend_auction))
        .routes(routes!(auctions::finish_auction))
        .routes(routes!(auctions::place_bid, auctions::auction_bids))
        .routes(routes!(auctions::pass_turn))
        .routes(routes!(auctions::mark_absent))
        .routes(routes!(
            results::generate_results_protocol,
            results::results_protocol_meta
        ))
        .routes(routes!(results::results_protocol_pdf))
        .routes(routes!(contracts::tender_contracts))
        .routes(routes!(contracts::my_contracts))
        .routes(routes!(contracts::draft_contract))
        .routes(routes!(
            contracts::contract_checklist,
            contracts::check_checklist_item
        ))
        .routes(routes!(contracts::advance_contract))
        .routes(routes!(contracts::register_contract))
        .routes(routes!(contracts::upload_scan))
        .routes(routes!(contracts::contract_pdf))
        .routes(routes!(
            contract_amendments::contract_amendments,
            contract_amendments::create_amendment
        ))
        .routes(routes!(contract_amendments::contract_amendment_pdf))
        .routes(routes!(contract_amendments::amendable_fields))
        .routes(routes!(reports::list_registries))
        .routes(routes!(reports::registry_csv))
        .routes(routes!(reports::registry))
        .routes(routes!(acts::contract_acts, acts::create_act))
        .routes(routes!(acts::upload_act_scan))
        .routes(routes!(acts::act_pdf))
        .routes(routes!(failure::failure_state))
        .routes(routes!(failure::declare_failed))
        .routes(routes!(failure::repeat_tender))
        .routes(routes!(failure::generate_failed_protocol))
        .routes(routes!(failure::failed_protocol_pdf))
        .routes(routes!(evasion::declare_evasion))
        .routes(routes!(evasion::tender_evasions))
        .routes(routes!(evasion::evasion_grounds))
        .routes(routes!(evasion::evader_registry))
        .routes(routes!(evasion::generate_winner2_protocol))
        .routes(routes!(evasion::winner2_protocol_pdf))
        .routes(routes!(publications::tender_protocols))
        .routes(routes!(publications::my_protocols))
        .routes(routes!(publications::publish_protocol))
        .routes(routes!(publications::protocol_pdf))
        .routes(routes!(publications::tender_dossier))
        .routes(routes!(publications::dossier_archive))
        .routes(routes!(publications::dossier_sections))
        .routes(routes!(publications::special_dossier))
        .routes(routes!(publications::special_dossier_archive))
        .routes(routes!(
            public_records::list_public_records,
            public_records::publish_record
        ))
        .routes(routes!(public_records::pending_publications))
        .routes(routes!(public_records::public_record_pdf))
        .routes(routes!(land::list_land_plots, land::save_land_plot))
        .routes(routes!(land::publish_land_plot))
        .routes(routes!(land::land_refdata))
        .routes(routes!(
            land::list_land_applications,
            land::submit_land_application
        ))
        .routes(routes!(land::my_land_applications))
        .routes(routes!(land::withdraw_land_application))
        .routes(routes!(land::decide_land_application))
        .routes(routes!(land::draft_land_contract))
        .routes(routes!(special::special_categories))
        .routes(routes!(special::submit_special_request))
        .routes(routes!(special::my_special_requests))
        .routes(routes!(special::get_special_request))
        .routes(routes!(special::withdraw_special_request))
        .routes(routes!(special::upload_special_file))
        .routes(routes!(special::download_special_file))
        .routes(routes!(special::special_request_pdf))
        .routes(routes!(special::pending_special_requests))
        .routes(routes!(special::special_progress))
        .routes(routes!(special::review_special_request))
        .routes(routes!(special::decide_special_request))
        .routes(routes!(special::special_decision_pdf))
        .routes(routes!(special::special_competition))
        .routes(routes!(investment::investment_attachments))
        .routes(routes!(
            investment::list_investment_contracts,
            investment::draft_investment_contract
        ))
        .routes(routes!(
            investment::upload_attachment,
            investment::download_attachment
        ))
        .routes(routes!(
            investment::list_acceptances,
            investment::accept_investment
        ))
        .routes(routes!(investment::extend_investment))
        .routes(routes!(investment::investment_contract_pdf))
        .routes(routes!(benefit::benefit_schemes))
        .routes(routes!(benefit::contract_benefit, benefit::grant_benefit))
}

/// Контракт OpenAPI 3.1 - единственный источник для `@tou/api-client` (G5)
/// и Swagger UI; собирается один раз.
static OPENAPI: LazyLock<utoipa::openapi::OpenApi> =
    LazyLock::new(|| api_router().split_for_parts().1);

pub fn openapi() -> utoipa::openapi::OpenApi {
    OPENAPI.clone()
}

/// Роутер API. Сессионный layer накладывается в композиции (`apps/api`),
/// т.к. ему нужен Redis.
pub fn router(state: AppState) -> Router {
    let (router, _) = api_router().split_for_parts();
    router
        .route("/api/v1/openapi.json", get(openapi_json))
        // WS-комната торгов (FR-603): вне OpenAPI - контракт кодогена
        // описывает HTTP, апгрейд протокола в нем не выразим
        .route("/api/v1/auctions/{id}/ws", get(auctions::room_socket))
        .fallback(|| async { error::ApiError::NotFound })
        .method_not_allowed_fallback(|| async { error::ApiError::MethodNotAllowed })
        .layer(middleware::from_fn(csrf::enforce))
        // Потолок времени обработки (NFR-02): снаружи CSRF, чтобы зависнуть
        // не мог и он, но внутри метрик - отказ по таймауту должен быть виден
        .layer(middleware::from_fn(timeout::enforce))
        // Сканы заявок (FR-401): по умолчанию axum режет тело на 2 МБ
        .layer(axum::extract::DefaultBodyLimit::max(20 * 1024 * 1024))
        .with_state(state)
}

/// Контракт в машиночитаемом виде - вход цепочки кодогена G5 и Swagger UI.
async fn openapi_json() -> axum::Json<&'static utoipa::openapi::OpenApi> {
    axum::Json(&OPENAPI)
}

/// Сессии в Redis (FR-1501): httpOnly, SameSite=Lax, продление по активности.
/// `secure` включается в проде (за Caddy TLS).
pub fn session_layer(pool: Pool, secure_cookies: bool) -> SessionManagerLayer<RedisStore<Pool>> {
    let store = RedisStore::new(pool);
    SessionManagerLayer::new(store)
        .with_name("tou_session")
        .with_secure(secure_cookies)
        .with_same_site(SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(Duration::hours(8)))
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode, header};
    use axum::routing::get;
    use tower::ServiceExt as _;

    #[test]
    fn openapi_document_builds() {
        let json = super::openapi().to_json().unwrap();
        for path in [
            "/api/v1/healthz",
            "/api/v1/auth/login",
            "/api/v1/auth/register",
            "/api/v1/admin/users",
        ] {
            assert!(json.contains(path), "нет пути {path}");
        }
    }

    /// Имя компонента OpenAPI = имя структуры (`ToSchema`), поэтому два
    /// одноименных типа в разных модулях молча сливаются в одну схему: клиент
    /// получает чужие поля, а гейт G5 остается зеленым - диффа нет. Проверяется
    /// не документ, а исходники: коллизия видна до кодогена.
    ///
    /// Так были найдены `RegisterRequest` (auth и contracts) и `ProtocolDto`
    /// (admission и publications).
    #[test]
    fn schema_type_names_are_unique_across_modules() {
        use std::collections::BTreeMap;

        let sources: [(&str, &str); 40] = [
            ("acts", include_str!("acts.rs")),
            ("admin", include_str!("admin.rs")),
            ("admission", include_str!("admission.rs")),
            ("amendments", include_str!("amendments.rs")),
            ("announcement", include_str!("announcement.rs")),
            ("applications", include_str!("applications.rs")),
            ("auctions", include_str!("auctions.rs")),
            ("auth", include_str!("auth.rs")),
            ("benefit", include_str!("benefit.rs")),
            ("commission", include_str!("commission.rs")),
            (
                "contract_amendments",
                include_str!("contract_amendments.rs"),
            ),
            ("contracts", include_str!("contracts.rs")),
            ("csrf", include_str!("csrf.rs")),
            ("dto", include_str!("dto.rs")),
            ("error", include_str!("error.rs")),
            ("evasion", include_str!("evasion.rs")),
            ("extract", include_str!("extract.rs")),
            ("failure", include_str!("failure.rs")),
            ("investment", include_str!("investment.rs")),
            ("land", include_str!("land.rs")),
            ("ledger", include_str!("ledger.rs")),
            ("notifications", include_str!("notifications.rs")),
            ("objects", include_str!("objects.rs")),
            ("obligations", include_str!("obligations.rs")),
            ("oidc", include_str!("oidc.rs")),
            ("pdf", include_str!("pdf.rs")),
            ("public_records", include_str!("public_records.rs")),
            ("publications", include_str!("publications.rs")),
            ("ratelimit", include_str!("ratelimit.rs")),
            ("rates", include_str!("rates.rs")),
            ("realtime", include_str!("realtime.rs")),
            ("refdata", include_str!("refdata.rs")),
            ("reports", include_str!("reports.rs")),
            ("request", include_str!("request.rs")),
            ("results", include_str!("results.rs")),
            ("special", include_str!("special.rs")),
            ("state", include_str!("state.rs")),
            ("storage", include_str!("storage.rs")),
            ("tenders", include_str!("tenders.rs")),
            ("timeout", include_str!("timeout.rs")),
        ];

        // Новый модуль обязан попасть в перечень: иначе проверка тихо
        // перестала бы покрывать часть контракта
        let declared = include_str!("lib.rs")
            .lines()
            .filter(|line| line.starts_with("pub mod "))
            .count();
        assert_eq!(
            sources.len(),
            declared,
            "перечень модулей в тесте разошелся с `pub mod` в lib.rs"
        );

        let mut owners = BTreeMap::<String, Vec<&str>>::new();
        for (module, source) in sources {
            let mut schema_derived = false;
            for line in source.lines() {
                let line = line.trim();
                if line.starts_with("#[derive(") {
                    schema_derived = line.contains("ToSchema");
                    continue;
                }
                let declaration = line
                    .strip_prefix("pub struct ")
                    .or_else(|| line.strip_prefix("pub enum "));
                if let Some(rest) = declaration
                    && schema_derived
                {
                    let name: String = rest
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    owners.entry(name).or_default().push(module);
                }
                // Атрибуты между derive и объявлением сохраняют признак
                if !line.starts_with("#[") && !line.starts_with("///") && !line.is_empty() {
                    schema_derived = false;
                }
            }
        }

        let clashes: Vec<_> = owners.iter().filter(|(_, m)| m.len() > 1).collect();
        assert!(
            clashes.is_empty(),
            "одноименные типы контракта в разных модулях затрут схему друг друга: {clashes:?}"
        );
    }

    #[tokio::test]
    async fn routing_failures_follow_problem_contract() {
        let app = Router::new()
            .route("/known", get(|| async {}))
            .fallback(|| async { super::error::ApiError::NotFound })
            .method_not_allowed_fallback(|| async { super::error::ApiError::MethodNotAllowed });

        for (method, uri, status) in [
            ("GET", "/missing", StatusCode::NOT_FOUND),
            ("POST", "/known", StatusCode::METHOD_NOT_ALLOWED),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .body(Body::empty())
                        .expect("request"),
                )
                .await
                .expect("response");
            assert_eq!(response.status(), status);
            assert_eq!(
                response.headers().get(header::CONTENT_TYPE),
                Some(&header::HeaderValue::from_static(
                    "application/problem+json"
                ))
            );
            let body = to_bytes(response.into_body(), 16 * 1024)
                .await
                .expect("problem body");
            assert!(!body.is_empty());
        }
    }
}
