//! HTTP-слой (арх. § 4): роутеры, экстракторы, OpenAPI-контракт, middleware, SSE/WS.
//!
//! Конвенции API (ТЗ § 7): REST под `/api/v1`, kebab-case пути,
//! cursor-пагинация, идемпотентность мутаций через `Idempotency-Key`,
//! ошибки - RFC 9457 problem+json с машинным `code`.

pub mod acts;
pub mod admin;
pub mod admin_data;
pub mod admin_schedule;
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
pub mod file_crypto;
pub mod idempotency;
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
pub mod site_announcements;
pub mod special;
pub mod state;
pub mod storage;
pub mod tenders;
pub mod timeout;
pub mod upload;
pub mod verification;

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

/// Размер страницы реестра по умолчанию (ТЗ § 7).
const PAGE_LIMIT: i64 = 50;

/// Границы страницы, о которой просит клиент.
///
/// Верхняя граница - сотня, как у уже курсорных маршрутов: страница крупнее
/// экрана бессмысленна, а `limit=1000000` в чужих руках - это тот же обход
/// потолка [`tou_db::MAX_ROWS`], только через контракт. Числа названы, а не
/// вписаны литералами: тот же интервал уходит в OpenAPI ([`SecurityAddon`]),
/// и разойтись «сервер режет» с «контракт обещает» они не должны.
const PAGE_LIMIT_MIN: i64 = 1;
const PAGE_LIMIT_MAX: i64 = 100;

/// Размер страницы, о котором просит клиент, - в допустимых пределах.
fn page_limit(requested: Option<i64>) -> i64 {
    requested
        .unwrap_or(PAGE_LIMIT)
        .clamp(PAGE_LIMIT_MIN, PAGE_LIMIT_MAX)
}

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
        .routes(routes!(auth::confirm_registration))
        .routes(routes!(auth::login))
        .routes(routes!(auth::logout))
        .routes(routes!(auth::me))
        .routes(routes!(auth::change_password))
        .routes(routes!(oidc::auth_providers))
        .routes(routes!(oidc::oidc_login))
        .routes(routes!(oidc::oidc_callback))
        .routes(routes!(oidc::oidc_logout))
        .routes(routes!(admin::list_users))
        .routes(routes!(admin::grant_role))
        .routes(routes!(admin::revoke_role))
        .routes(routes!(admin::reset_password))
        .routes(routes!(admin::set_user_active))
        .routes(routes!(admin::audit_chain))
        .routes(routes!(admin_data::data_overview))
        .routes(routes!(admin_data::list_records))
        .routes(routes!(admin_data::purge_data))
        .routes(routes!(admin_data::purge_record))
        .routes(routes!(admin_data::deactivate_demo_accounts))
        .routes(routes!(admin_schedule::list_schedules))
        .routes(routes!(admin_schedule::set_schedule))
        .routes(routes!(site_announcements::published))
        .routes(routes!(
            site_announcements::current,
            site_announcements::save
        ))
        .routes(routes!(objects::list_objects, objects::create_object))
        .routes(routes!(
            objects::get_object,
            objects::update_object,
            objects::delete_object
        ))
        .routes(routes!(tenders::list_tenders, tenders::create_tender))
        .routes(routes!(
            tenders::get_tender,
            tenders::update_tender,
            tenders::delete_tender
        ))
        .routes(routes!(tenders::list_documents, tenders::upload_document))
        .routes(routes!(tenders::download_document))
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

/// Имя схемы безопасности сессии - оно же имя cookie ([`session_layer`]).
const SESSION_SCHEME: &str = "tou_session";

/// Имя схемы безопасности токена double-submit ([`csrf`]).
///
/// Токен описан именно схемой безопасности, а не только параметром: это
/// удостоверение, а не входные данные операции, и клиент контракта
/// (`packages/api-client`) подставляет его слоем, а не в месте вызова.
const CSRF_SCHEME: &str = "tou_csrf";

/// Заголовок ключа идемпотентности в каноническом написании (ТЗ § 7).
/// Слой ищет его регистронезависимо ([`idempotency::IDEMPOTENCY_HEADER`]),
/// а контракт читает человек.
const IDEMPOTENCY_HEADER_NAME: &str = "Idempotency-Key";

/// Пустой перечень областей: cookie-сессия их не знает.
const NO_SCOPES: [&str; 0] = [];

/// Операции, доступные гостю: 401 от них не приходит.
///
/// Признак живет в хендлере, а не в роутере: рубеж накладывает экстрактор
/// [`extract::CurrentUser`], и «публично» здесь означает ровно «в сигнатуре
/// нет обязательного `CurrentUser`». Перечень продублирован сюда потому, что
/// собранный документ хендлеров уже не видит; расхождение с сигнатурами
/// ловит тест `public_operations_match_handler_signatures`.
const PUBLIC_OPERATIONS: &[&str] = &[
    "amendment_pdf",
    "announcement_pdf",
    "auth_providers",
    "confirm_registration",
    "download_document",
    "get_object",
    "get_tender",
    "healthz",
    "land_refdata",
    "list_documents",
    "list_land_plots",
    "list_objects",
    "list_public_records",
    "list_tenders",
    "login",
    "logout",
    "oidc_callback",
    "oidc_login",
    "oidc_logout",
    "protocol_pdf",
    "public_record_pdf",
    "published",
    "register",
    "tender_amendments",
    "tender_protocols",
];

/// Дологинные мутации: сессии, а значит и токена в ней, еще нет, и слой CSRF
/// их пропускает (`csrf::CSRF_EXEMPT`). Выход в перечень не входит намеренно -
/// он идет уже с токеном.
const CSRF_FREE_OPERATIONS: &[&str] = &["confirm_registration", "login", "register"];

/// Общие отказы problem+json (NFR-08): имя компонента, статус, описание.
///
/// Описание пишется один раз в `components.responses`, а операции ссылаются
/// на него - иначе те же полсотни слов размножились бы по 190 операциям.
const SHARED_RESPONSES: &[(&str, &str, &str)] = &[
    (
        "BadRequest",
        "400",
        "Запрос не разобран: идентификатор в пути или параметр запроса неверного вида (`validation_failed`)",
    ),
    (
        "Unauthorized",
        "401",
        "Требуется вход: сессионной cookie `tou_session` нет либо она недействительна (`unauthorized`)",
    ),
    (
        "Forbidden",
        "403",
        "Недостаточно прав для действия (`forbidden`) либо не сошелся токен `x-csrf-token` (`csrf_rejected`)",
    ),
    ("NotFound", "404", "Ресурс не найден (`not_found`)"),
    (
        "MethodNotAllowed",
        "405",
        "Метод не поддерживается этим ресурсом (`method_not_allowed`)",
    ),
    (
        "IdempotencyInFlight",
        "409",
        "Запрос с тем же `Idempotency-Key` еще выполняется (`idempotency_in_flight`)",
    ),
    (
        "UnsupportedMediaType",
        "415",
        "Тип содержимого тела запроса не поддерживается (`validation_failed`)",
    ),
    (
        "Unprocessable",
        "422",
        "Данные не прошли проверку (`validation_failed`) либо тем же `Idempotency-Key` пришел другой запрос (`idempotency_key_reuse`)",
    ),
];

/// Рубежи доступа, общие отказы и границы страницы в контракте.
///
/// Накладывается на уже собранный документ, а не через
/// `#[openapi(modifiers(..))]` у [`ApiDoc`]: модификаторы деривации
/// отрабатывают внутри `ApiDoc::openapi()`, то есть до того, как
/// `OpenApiRouter` внесет в документ хоть один путь, - править им было бы
/// нечего.
///
/// Что накладывается и почему именно так:
///
/// * схема безопасности сессии (`apiKey` в cookie) - на каждую операцию,
///   кроме публичных, плюс ответ 401;
/// * 403 - там же и на каждой мутации: отказать может и политика
///   ([`extract::CurrentUser::require`]), и рубеж CSRF;
/// * 405 - на всех операциях: `method_not_allowed_fallback` роутера отвечает
///   так по любому пути;
/// * 404 - на операциях с параметром пути;
/// * 400 и 422 - там, где есть что разбирать (параметры или тело);
/// * 415 - на мутациях: тип содержимого проверяется при чтении тела;
/// * `Idempotency-Key` и его 409 - на мутациях вошедшего пользователя, ровно
///   там, где работает слой [`idempotency`].
///
/// Заголовки объявлены необязательными (`required: false`), и это не описка.
/// `x-csrf-token` обязателен для сервера, но проставляет его клиент контракта
/// слоем, а не место вызова; объяви контракт заголовок обязательным - и
/// генератор типов потребовал бы токен аргументом в каждой из 89 мутаций
/// фронта, то есть заставил бы обходить тот самый слой. Обязательность
/// выражена там, где ей место и где генератору типов она не мешает, - в
/// `security` (`tou_csrf`).
struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
        use utoipa::openapi::{Components, RefOr};

        let components = openapi.components.get_or_insert_with(Components::new);
        components.add_security_scheme(
            SESSION_SCHEME,
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                SESSION_SCHEME,
                "Сессионная cookie (httpOnly, SameSite=Lax): ставится входом, снимается выходом (FR-1501)",
            ))),
        );
        components.add_security_scheme(
            CSRF_SCHEME,
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::with_description(
                csrf::CSRF_HEADER,
                "Токен double-submit: значение cookie `tou_csrf`, возвращаемое заголовком на каждой мутации (арх. § 5)",
            ))),
        );
        for (name, _, description) in SHARED_RESPONSES {
            components
                .responses
                .insert((*name).to_owned(), RefOr::T(problem_response(description)));
        }

        for (path, item) in openapi.paths.paths.iter_mut() {
            let has_path_parameter = path.contains('{');
            let operations = [
                (false, &mut item.get),
                (true, &mut item.post),
                (true, &mut item.put),
                (true, &mut item.patch),
                (true, &mut item.delete),
            ];
            for (mutating, slot) in operations {
                if let Some(operation) = slot.as_mut() {
                    document_operation(operation, mutating, has_path_parameter);
                }
            }
        }
    }
}

/// Ответ problem+json общего вида.
fn problem_response(description: &str) -> utoipa::openapi::Response {
    use utoipa::openapi::{ContentBuilder, Ref, ResponseBuilder};

    ResponseBuilder::new()
        .description(description)
        .content(
            "application/problem+json",
            ContentBuilder::new()
                .schema(Some(Ref::from_schema_name("Problem")))
                .build(),
        )
        .build()
}

/// Рубежи и общие отказы одной операции (см. [`SecurityAddon`]).
fn document_operation(
    operation: &mut utoipa::openapi::path::Operation,
    mutating: bool,
    has_path_parameter: bool,
) {
    use utoipa::openapi::security::SecurityRequirement;

    let id = operation.operation_id.clone().unwrap_or_default();
    let public = PUBLIC_OPERATIONS.contains(&id.as_str());
    let needs_csrf = mutating && !CSRF_FREE_OPERATIONS.contains(&id.as_str());
    // Слой идемпотентности берет только мутации вошедшего (см. [`idempotency`])
    let keyed = mutating && !public;
    // Считается до того, как ниже добавятся заголовки: 400 и 422 обещаются
    // за разбор входа операции, а не за наши же параметры
    let takes_input = operation.parameters.is_some() || operation.request_body.is_some();

    clamp_page_limit(operation);

    let requirement = match (!public, needs_csrf) {
        (true, true) => {
            Some(SecurityRequirement::new(SESSION_SCHEME, NO_SCOPES).add(CSRF_SCHEME, NO_SCOPES))
        }
        (true, false) => Some(SecurityRequirement::new(SESSION_SCHEME, NO_SCOPES)),
        // Выход: сессия не обязательна, а токен - да
        (false, true) => Some(SecurityRequirement::new(CSRF_SCHEME, NO_SCOPES)),
        (false, false) => None,
    };
    if let Some(requirement) = requirement
        && operation.security.is_none()
    {
        operation.security = Some(vec![requirement]);
    }

    if needs_csrf {
        add_header(
            operation,
            csrf::CSRF_HEADER,
            "Обязателен: значение cookie `tou_csrf`. Без него мутация отклоняется 403 `csrf_rejected` (арх. § 5)",
        );
    }
    if keyed {
        add_header(
            operation,
            IDEMPOTENCY_HEADER_NAME,
            "Необязателен: с ним повтор мутации отдает ответ первой попытки, а не выполняет ее второй раз (ТЗ § 7)",
        );
    }

    ensure_response(operation, "MethodNotAllowed");
    if !public {
        ensure_response(operation, "Unauthorized");
    }
    if !public || needs_csrf {
        ensure_response(operation, "Forbidden");
    }
    if has_path_parameter {
        ensure_response(operation, "NotFound");
    }
    if takes_input || mutating {
        ensure_response(operation, "BadRequest");
        ensure_response(operation, "Unprocessable");
    }
    if mutating {
        ensure_response(operation, "UnsupportedMediaType");
    }
    if keyed {
        ensure_response(operation, "IdempotencyInFlight");
    }
}

/// Ссылка на общий отказ - если операция не объявила свой ответ с тем же
/// статусом. Объявленный вручную точнее: он знает, за что именно откажет
/// эта операция.
fn ensure_response(operation: &mut utoipa::openapi::path::Operation, name: &str) {
    use utoipa::openapi::{Ref, RefOr};

    let Some((_, status, _)) = SHARED_RESPONSES.iter().find(|(known, _, _)| *known == name) else {
        return;
    };
    operation
        .responses
        .responses
        .entry((*status).to_owned())
        .or_insert_with(|| RefOr::Ref(Ref::from_response_name(name)));
}

/// Заголовок запроса в перечне параметров операции.
fn add_header(operation: &mut utoipa::openapi::path::Operation, name: &str, description: &str) {
    use utoipa::openapi::Required;
    use utoipa::openapi::path::{ParameterBuilder, ParameterIn};
    use utoipa::openapi::schema::{ObjectBuilder, Type};

    let parameters = operation.parameters.get_or_insert_with(Vec::new);
    if parameters
        .iter()
        .any(|parameter| parameter.name.eq_ignore_ascii_case(name))
    {
        return;
    }
    parameters.push(
        ParameterBuilder::new()
            .name(name)
            .parameter_in(ParameterIn::Header)
            .required(Required::False)
            .description(Some(description))
            .schema(Some(ObjectBuilder::new().schema_type(Type::String)))
            .build(),
    );
}

/// Границы `limit` в контракте - те же, что режет [`page_limit`] (G-038).
///
/// Правится собранный документ, а не восемь одинаковых `IntoParams` в
/// модулях: интервал один на все реестры, и объявленный в одном месте он не
/// разъедется с сервером по пропущенной структуре.
fn clamp_page_limit(operation: &mut utoipa::openapi::path::Operation) {
    use utoipa::Number;
    use utoipa::openapi::RefOr;
    use utoipa::openapi::path::ParameterIn;
    use utoipa::openapi::schema::Schema;

    let Some(parameters) = operation.parameters.as_mut() else {
        return;
    };
    for parameter in parameters {
        if parameter.name != "limit" || !matches!(parameter.parameter_in, ParameterIn::Query) {
            continue;
        }
        if let Some(RefOr::T(Schema::Object(object))) = parameter.schema.as_mut() {
            object.minimum = Some(Number::Int(PAGE_LIMIT_MIN as isize));
            object.maximum = Some(Number::Int(PAGE_LIMIT_MAX as isize));
        }
        if parameter.description.is_none() {
            parameter.description = Some(format!(
                "Размер страницы: от {PAGE_LIMIT_MIN} до {PAGE_LIMIT_MAX}, по умолчанию {PAGE_LIMIT}"
            ));
        }
    }
}

/// Контракт OpenAPI 3.1 - единственный источник для `@tou/api-client` (G5)
/// и Swagger UI; собирается один раз.
static OPENAPI: LazyLock<utoipa::openapi::OpenApi> = LazyLock::new(|| {
    use utoipa::Modify as _;

    let mut document = api_router().split_for_parts().1;
    SecurityAddon.modify(&mut document);
    document
});

pub fn openapi() -> utoipa::openapi::OpenApi {
    OPENAPI.clone()
}

/// Потолок тела запроса (FR-401, сканы заявок): по умолчанию axum режет его
/// на 2 МБ. Константа, а не литерал по месту: тот же предел применяет слой
/// идемпотентности, когда буферизует тело ради отпечатка запроса, и разойтись
/// эти два числа не должны.
pub const MAX_BODY_BYTES: usize = 20 * 1024 * 1024;

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
        // Идемпотентность мутаций (ТЗ § 7): внутри CSRF - запрос, не прошедший
        // проверку токена, не должен занимать ключ в журнале
        .layer(middleware::from_fn_with_state(
            state.idempotency.clone(),
            idempotency::enforce,
        ))
        .layer(middleware::from_fn(csrf::enforce))
        // Ограничение дорогих маршрутов (NFR-07): снаружи CSRF - обращение,
        // отбитое проверкой токена, считается так же, как обычное, а вот
        // успеть выполнить печать или выгрузку до счетчика нельзя
        .layer(middleware::from_fn_with_state(
            state.rate_limit.clone(),
            ratelimit::enforce,
        ))
        // Потолок времени обработки (NFR-02): снаружи CSRF, чтобы зависнуть
        // не мог и он, но внутри метрик - отказ по таймауту должен быть виден
        .layer(middleware::from_fn(timeout::enforce))
        // Сканы заявок (FR-401): по умолчанию axum режет тело на 2 МБ
        .layer(axum::extract::DefaultBodyLimit::max(MAX_BODY_BYTES))
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

    /// Исходники модулей контракта: по ним проверяются вещи, которых в
    /// собранном документе уже не видно, - имена типов и сигнатуры хендлеров.
    const SOURCES: [(&str, &str); 47] = [
        ("acts", include_str!("acts.rs")),
        ("admin", include_str!("admin.rs")),
        ("admin_data", include_str!("admin_data.rs")),
        ("admin_schedule", include_str!("admin_schedule.rs")),
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
        ("file_crypto", include_str!("file_crypto.rs")),
        ("idempotency", include_str!("idempotency.rs")),
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
        ("site_announcements", include_str!("site_announcements.rs")),
        ("special", include_str!("special.rs")),
        ("state", include_str!("state.rs")),
        ("storage", include_str!("storage.rs")),
        ("tenders", include_str!("tenders.rs")),
        ("timeout", include_str!("timeout.rs")),
        ("upload", include_str!("upload.rs")),
        ("verification", include_str!("verification.rs")),
    ];

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

        let sources = SOURCES;

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

    /// Операции документа: путь, метод, тело.
    fn contract_operations() -> Vec<(String, &'static str, serde_json::Value)> {
        let json = super::openapi().to_json().expect("сериализация контракта");
        let document: serde_json::Value = serde_json::from_str(&json).expect("разбор контракта");
        let mut operations = Vec::new();
        for (path, item) in document["paths"].as_object().expect("пути документа") {
            for method in ["get", "post", "put", "patch", "delete"] {
                if let Some(operation) = item.get(method) {
                    operations.push((path.clone(), method, operation.clone()));
                }
            }
        }
        operations
    }

    fn declares(operation: &serde_json::Value, status: &str) -> bool {
        !operation["responses"][status].is_null()
    }

    fn declares_header(operation: &serde_json::Value, name: &str) -> bool {
        operation["parameters"]
            .as_array()
            .is_some_and(|parameters| {
                parameters.iter().any(|parameter| {
                    parameter["in"] == "header"
                        && parameter["name"]
                            .as_str()
                            .is_some_and(|value| value.eq_ignore_ascii_case(name))
                })
            })
    }

    fn operation_id(operation: &serde_json::Value) -> &str {
        operation["operationId"].as_str().unwrap_or_default()
    }

    /// Рубеж доступа обязан быть виден в контракте, а не только в коде
    /// (G-033): без схемы безопасности и общих отказов клиент контракта
    /// узнает про 401, 403 и 405 из практики.
    #[test]
    fn contract_declares_access_boundaries() {
        let json = super::openapi().to_json().expect("сериализация контракта");
        let document: serde_json::Value = serde_json::from_str(&json).expect("разбор контракта");

        let schemes = &document["components"]["securitySchemes"];
        assert_eq!(schemes[super::SESSION_SCHEME]["type"], "apiKey");
        assert_eq!(schemes[super::SESSION_SCHEME]["in"], "cookie");
        assert_eq!(
            schemes[super::SESSION_SCHEME]["name"],
            super::SESSION_SCHEME
        );
        assert_eq!(schemes[super::CSRF_SCHEME]["type"], "apiKey");
        assert_eq!(schemes[super::CSRF_SCHEME]["in"], "header");
        assert_eq!(
            schemes[super::CSRF_SCHEME]["name"],
            super::csrf::CSRF_HEADER
        );

        for (path, method, operation) in contract_operations() {
            let id = operation_id(&operation);
            let at = format!("{} {path} ({id})", method.to_uppercase());
            let public = super::PUBLIC_OPERATIONS.contains(&id);
            let mutating = method != "get";

            assert!(declares(&operation, "405"), "{at}: нет 405");
            if path.contains('{') {
                assert!(declares(&operation, "404"), "{at}: нет 404 при id в пути");
            }
            if public {
                assert!(
                    operation["security"][0][super::SESSION_SCHEME].is_null(),
                    "{at}: публичная операция требует сессию"
                );
            } else {
                assert!(declares(&operation, "401"), "{at}: нет 401");
                assert!(
                    !operation["security"][0][super::SESSION_SCHEME].is_null(),
                    "{at}: нет требования сессии"
                );
            }
            if mutating {
                assert!(declares(&operation, "400"), "{at}: нет 400");
                assert!(declares(&operation, "415"), "{at}: нет 415");
                assert!(declares(&operation, "422"), "{at}: нет 422");
                if super::CSRF_FREE_OPERATIONS.contains(&id) {
                    continue;
                }
                assert!(declares(&operation, "403"), "{at}: нет 403");
                assert!(
                    declares_header(&operation, super::csrf::CSRF_HEADER),
                    "{at}: не объявлен заголовок CSRF"
                );
                assert!(
                    !operation["security"][0][super::CSRF_SCHEME].is_null(),
                    "{at}: нет требования токена CSRF"
                );
                if !public {
                    assert!(
                        declares_header(&operation, super::IDEMPOTENCY_HEADER_NAME),
                        "{at}: не объявлен ключ идемпотентности"
                    );
                    assert!(declares(&operation, "409"), "{at}: нет 409");
                }
            }
        }
    }

    /// Заголовок ключа идемпотентности в контракте и в слое - один и тот же.
    #[test]
    fn idempotency_header_name_matches_the_layer() {
        assert!(
            super::IDEMPOTENCY_HEADER_NAME
                .eq_ignore_ascii_case(super::idempotency::IDEMPOTENCY_HEADER),
            "контракт обещает заголовок, которого слой не ищет"
        );
    }

    /// Потолок страницы обещан контрактом ровно тот, что режет сервер (G-038).
    #[test]
    fn page_limit_bounds_reach_the_contract() {
        let mut found = 0;
        for (path, method, operation) in contract_operations() {
            let Some(parameters) = operation["parameters"].as_array() else {
                continue;
            };
            for parameter in parameters {
                if parameter["name"] != "limit" || parameter["in"] != "query" {
                    continue;
                }
                found += 1;
                let at = format!("{} {path}", method.to_uppercase());
                assert_eq!(
                    parameter["schema"]["minimum"],
                    super::PAGE_LIMIT_MIN,
                    "{at}"
                );
                assert_eq!(
                    parameter["schema"]["maximum"],
                    super::PAGE_LIMIT_MAX,
                    "{at}"
                );
            }
        }
        assert!(
            found >= 8,
            "параметров limit найдено {found} - реестры потерялись"
        );
        assert_eq!(super::page_limit(Some(10_000)), super::PAGE_LIMIT_MAX);
        assert_eq!(super::page_limit(Some(0)), super::PAGE_LIMIT_MIN);
    }

    /// Перечень публичных операций обязан совпадать с сигнатурами хендлеров:
    /// рубеж накладывает экстрактор, а не роутер, и разъехаться эти два
    /// перечня могут молча - новый маршрут просто не получил бы 401
    /// в контракте.
    ///
    /// Иголка собирается `concat!`: в исходном тексте самого lib.rs она
    /// не встречается, иначе разбор нашел бы этот тест.
    #[test]
    fn public_operations_match_handler_signatures() {
        let needle = concat!("#[utoipa", "::path(");
        let mut public = Vec::new();
        let mut documented = 0;

        for source in SOURCES
            .iter()
            .map(|(_, source)| *source)
            .chain(std::iter::once(include_str!("lib.rs")))
        {
            let mut rest = source;
            while let Some(at) = rest.find(needle) {
                rest = &rest[at + needle.len()..];
                let Some(start) = rest.find("async fn ") else {
                    break;
                };
                let after = &rest[start + "async fn ".len()..];
                let Some(open) = after.find('(') else { break };
                let name = after[..open].trim();

                let mut depth = 0_usize;
                let mut end = after.len();
                for (index, symbol) in after[open..].char_indices() {
                    match symbol {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                end = open + index;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                let signature = &after[open..end];

                documented += 1;
                let secured =
                    signature.contains("CurrentUser") && !signature.contains("Option<CurrentUser>");
                if !secured {
                    public.push(name.to_owned());
                }
                rest = after;
            }
        }

        assert_eq!(
            documented,
            contract_operations().len(),
            "разобрано не столько хендлеров, сколько операций в документе"
        );
        public.sort_unstable();
        let declared: Vec<String> = {
            let mut declared: Vec<String> = super::PUBLIC_OPERATIONS
                .iter()
                .map(|id| (*id).to_owned())
                .collect();
            declared.sort_unstable();
            declared
        };
        assert_eq!(
            public, declared,
            "PUBLIC_OPERATIONS разошелся с сигнатурами: слева - хендлеры без обязательного CurrentUser"
        );
    }

    /// Дологинные мутации контракта - те же, что пропускает слой CSRF.
    #[test]
    fn csrf_free_operations_match_the_exempt_paths() {
        let source = include_str!("csrf.rs");
        assert_eq!(
            source.matches("\"/api/v1/").count(),
            super::CSRF_FREE_OPERATIONS.len(),
            "CSRF_EXEMPT в csrf.rs изменился - выровняйте CSRF_FREE_OPERATIONS"
        );
        for (path, _, operation) in contract_operations() {
            if super::CSRF_FREE_OPERATIONS.contains(&operation_id(&operation)) {
                assert!(
                    source.contains(&format!("\"{path}\"")),
                    "{path} объявлен дологинным, но слой CSRF его не пропускает"
                );
            }
        }
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
