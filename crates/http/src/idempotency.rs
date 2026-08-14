//! Идемпотентность мутаций по заголовку `Idempotency-Key` (ТЗ § 7, W-30).
//!
//! Конвенция объявлена в заголовке [`crate`] с самого начала, а реализации
//! не было нигде. Для ставок ее роль играл клиентский `client_bid_id`, для
//! подачи заявки от дубля спасал `UNIQUE (lot_id, participant_id)` - то есть
//! побочный эффект схемы, а не намерение: участник, нажавший «подать заявку»
//! дважды при сетевой задержке, получал отказ по правилу на самом
//! ответственном для него действии.
//!
//! Слой хранит ответ первой попытки и отдает его на повтор с тем же ключом.
//! Ключ считается вместе с пользователем и маршрутом, а не сам по себе:
//! одинаковые ключи у разных людей (а генератор клиента про чужие ключи
//! ничего не знает) не должны встречаться, и тем более чужой ключ не должен
//! возвращать чужой ответ - в нем персональные данные заявителя (NFR-07).
//! В Redis уходит отпечаток этой тройки, а не ее части (NFR-16).
//!
//! # Что под слоем, а что нет
//!
//! Только мутации: GET, HEAD и OPTIONS повторяются по своей природе, а
//! запоминать их ответ - это кеш, а не идемпотентность, и он бы устаревал.
//! Тем же условием отсекаются потоковые маршруты (SSE-лента уведомлений,
//! WS-комната торгов) и выдача файлов - они все GET.
//!
//! Только вошедшие: без пользователя ключ не к чему привязать. Заодно из-под
//! слоя выпадают `/auth/login` и `/auth/register` - единственные мутации до
//! входа, - и это правильно: повторно отданный ответ входа означал бы
//! повторно отданную cookie сессии.
//!
//! Запоминается только ответ в JSON и только окончательный. Ответ сервера об
//! ошибке (5xx) и отказ счетчика (429) не окончательны - это приглашение
//! повторить, и запись их на сутки превратила бы разовый сбой в суточный.
//!
//! # Параллельный повтор
//!
//! Второй запрос с тем же ключом, пришедший, пока первый еще выполняется,
//! получает отказ 409, а не ждет и не выполняется. Ожидание означало бы
//! держать соединение и рабочую задачу ради дубля, который пользователь и не
//! ждет (он уже нажал кнопку второй раз), а выполнение - ровно то, от чего
//! слой и защищает: два одновременных нажатия проходят проверку «ответа еще
//! нет» одновременно. Отметка «в работе» ставится атомарно (`SET NX GET`),
//! поэтому ее выигрывает ровно один запрос.
//!
//! Отметка живет минуту - дольше потолка обработки запроса (NFR-02, 30 с).
//! Если задача исчезла, не дописав ответ (клиент отключился, процесс
//! перезапустили), ключ освободится сам через минуту, а не запрет операцию
//! на сутки.
//!
//! # Срок хранения
//!
//! Сутки. Повтор рождается из сетевой задержки, промаха сети и второго
//! нажатия - это секунды и минуты; сутки покрывают и клиента, вернувшегося
//! после долгого обрыва, с запасом на всю рабочую смену. Больше держать
//! нечего: Redis в этой системе - хранилище эфемерного (арх. § 2), а факт
//! операции давно записан в PostgreSQL.

use axum::body::{Body, to_bytes};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse as _, Response};
use fred::prelude::{Expiration, KeysInterface as _, SetOptions};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use crate::error::ApiError;
use crate::realtime::Pool;

/// Ключ идемпотентности запроса (ТЗ § 7). Имя - в нижнем регистре: так его
/// хранит `HeaderMap`, и так его можно сравнивать без разбора.
pub const IDEMPOTENCY_HEADER: &str = "idempotency-key";

/// Признак того, что ответ отдан из журнала, а не выполнен заново. Клиенту он
/// не обязателен (ответ и так тот же), но без него ни диагностика, ни тест не
/// отличают повтор от нового выполнения.
pub const REPLAY_HEADER: &str = "idempotent-replay";

/// Значение отметки «запрос в работе». Отличается от записанного ответа тем,
/// что не разбирается как JSON, - другого признака не требуется.
const IN_FLIGHT: &str = "in-flight";

/// Срок отметки «в работе»: больше потолка обработки запроса (NFR-02).
const IN_FLIGHT_TTL_SECONDS: i64 = 60;

/// Срок хранения записанного ответа - сутки (см. заголовок модуля).
const STORED_TTL_SECONDS: i64 = 24 * 60 * 60;

/// Потолок тела, которое имеет смысл держать в Redis. Ответ мутации в этом
/// API - один DTO; шестьдесят четыре килобайта его перекрывают, а все, что
/// крупнее, - повод не занимать эфемерное хранилище, а выполнить повтор
/// заново.
const MAX_STORED_BODY_BYTES: usize = 64 * 1024;

/// Потолок длины клиентского ключа. Ключ все равно сворачивается в отпечаток,
/// поэтому предел - не про Redis, а про то, что килобайтный «ключ» означает
/// ошибку клиента, а не намерение.
const MAX_KEY_CHARS: usize = 255;

/// Журнал идемпотентности в том же Redis, что и сессии (NFR-12): повтор может
/// прийти в другой экземпляр api, и ответ обязан найтись и там.
///
/// `None` - Redis не подключен (тесты, `api openapi`): слой становится пустой
/// операцией, как и счетчик попыток.
#[derive(Clone, Default)]
pub struct IdempotencyStore {
    pool: Option<Pool>,
}

/// Записанный ответ на проводе Redis.
///
/// Из заголовков сохраняется только тип содержимого: остальное - либо
/// вычисляемое (длина), либо привязанное к соединению. Заголовков, которые
/// нельзя потерять, у мутаций этого API нет: cookie выдает только вход, а он
/// под слой не попадает.
#[derive(Debug, Serialize, Deserialize)]
struct StoredResponse {
    status: u16,
    content_type: Option<String>,
    body: String,
}

/// Состояние ключа на момент прихода запроса.
enum Slot {
    /// Ключ свободен, отметка «в работе» поставлена этим запросом
    Taken,
    /// Тот же ключ уже выполняется
    InFlight,
    /// Ответ записан - его и надо отдать
    Answered(Box<StoredResponse>),
    /// Журнала нет (Redis не подключен либо не ответил): выполняем как раньше
    Disabled,
}

impl IdempotencyStore {
    pub fn new(pool: Pool) -> Self {
        Self { pool: Some(pool) }
    }

    /// Занять ключ.
    ///
    /// `SET NX GET` - одна атомарная операция: она и ставит отметку, если ключ
    /// свободен, и возвращает то, что там уже лежит, если занят. Пары
    /// «прочитать - записать» здесь было бы недостаточно: два одновременных
    /// нажатия кнопки читают пустоту одновременно, и оба выполняются.
    ///
    /// Недоступность Redis не отказ, а работа как без заголовка. Отказывать
    /// незачем: сюда запрос доходит только с прочитанной сессией, то есть
    /// Redis только что отвечал, а поведение без журнала - ровно то, которое
    /// система имела до этой задачи, и от дубля заявку по-прежнему стережет
    /// `UNIQUE (lot_id, participant_id)`.
    async fn begin(&self, key: &str) -> Slot {
        let Some(pool) = self.pool.as_ref() else {
            return Slot::Disabled;
        };

        let previous: Result<Option<String>, _> = pool
            .set(
                key,
                IN_FLIGHT,
                Some(Expiration::EX(IN_FLIGHT_TTL_SECONDS)),
                Some(SetOptions::NX),
                true,
            )
            .await;

        match previous {
            Ok(None) => Slot::Taken,
            Ok(Some(value)) if value == IN_FLIGHT => Slot::InFlight,
            Ok(Some(value)) => match serde_json::from_str::<StoredResponse>(&value) {
                Ok(stored) => Slot::Answered(Box::new(stored)),
                // Записать неразбираемое значение мог только другой формат
                // записи (выкатка новой версии): выполнить запрос заново
                // безопаснее, чем отдать непонятное
                Err(error) => {
                    tracing::warn!(%error, "запись идемпотентности не разобрана");
                    Slot::Disabled
                }
            },
            Err(error) => {
                tracing::warn!(%error, "журнал идемпотентности недоступен - запрос выполняется как обычный");
                Slot::Disabled
            }
        }
    }

    /// Записать окончательный ответ на сутки, заменив отметку «в работе».
    async fn record(&self, key: &str, stored: &StoredResponse) {
        let Some(pool) = self.pool.as_ref() else {
            return;
        };
        let payload = match serde_json::to_string(stored) {
            Ok(payload) => payload,
            Err(error) => {
                tracing::error!(%error, "сериализация записи идемпотентности");
                self.release(key).await;
                return;
            }
        };

        if let Err(error) = pool
            .set::<(), _, _>(
                key,
                payload,
                Some(Expiration::EX(STORED_TTL_SECONDS)),
                None,
                false,
            )
            .await
        {
            // Отметка «в работе» истечет через минуту сама, и повтор просто
            // выполнится заново - хуже, чем идемпотентность, но не сломано
            tracing::warn!(%error, "ответ не записан в журнал идемпотентности");
        }
    }

    /// Снять отметку: ответ повторять нельзя (ошибка сервера, не-JSON,
    /// слишком большое тело), и держать ключ занятым не за что.
    async fn release(&self, key: &str) {
        let Some(pool) = self.pool.as_ref() else {
            return;
        };
        if let Err(error) = pool.del::<(), _>(key).await {
            tracing::warn!(%error, "отметка идемпотентности не снята");
        }
    }
}

/// Middleware идемпотентности. Слой накладывается на весь роутер, но работает
/// только там, где заголовок есть и где он имеет смысл: без него поведение
/// маршрута прежнее до последнего байта.
///
/// Слой стоит внутри CSRF: запрос, не прошедший проверку токена, не должен
/// занимать ключ - иначе чужая страница, знающая ключ, отравляла бы журнал.
pub async fn enforce(
    State(store): State<IdempotencyStore>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if !is_guarded(request.method()) {
        return Ok(next.run(request).await);
    }
    let Some(client_key) = header_key(request.headers())? else {
        return Ok(next.run(request).await);
    };
    // Сессия снимается копией до первого await: тело запроса не Sync, и
    // ссылка на запрос не пережила бы точку ожидания (будущее слоя обязано
    // быть Send)
    let session = request
        .extensions()
        .get::<tower_sessions::Session>()
        .cloned();
    let Some(user_id) = user_of(session).await else {
        return Ok(next.run(request).await);
    };

    let key = key_of(user_id, request.method(), request.uri().path(), &client_key);

    match store.begin(&key).await {
        Slot::Answered(stored) => return Ok(replay(*stored)),
        Slot::InFlight => return Err(ApiError::IdempotencyInFlight),
        Slot::Disabled => return Ok(next.run(request).await),
        Slot::Taken => {}
    }

    Ok(finish(&store, &key, next.run(request).await).await)
}

/// Мутации - и только они (см. заголовок модуля).
fn is_guarded(method: &Method) -> bool {
    *method == Method::POST
        || *method == Method::PUT
        || *method == Method::PATCH
        || *method == Method::DELETE
}

/// Клиентский ключ из заголовка. `None` - заголовка нет, идемпотентность не
/// запрошена; ошибка - заголовок есть, но пользоваться им нельзя, и молчать
/// об этом нельзя тем более: клиент считает, что защищен.
fn header_key(headers: &HeaderMap) -> Result<Option<String>, ApiError> {
    let Some(raw) = headers.get(IDEMPOTENCY_HEADER) else {
        return Ok(None);
    };
    let value = raw
        .to_str()
        .map_err(|_| ApiError::bad_request("Idempotency-Key: ожидается печатный ASCII"))?
        .trim();

    if value.is_empty() {
        return Err(ApiError::bad_request("Idempotency-Key: пустое значение"));
    }
    if value.chars().count() > MAX_KEY_CHARS {
        return Err(ApiError::bad_request(format!(
            "Idempotency-Key: не длиннее {MAX_KEY_CHARS} символов"
        )));
    }
    Ok(Some(value.to_owned()))
}

/// Пользователь запроса; без него идемпотентность не применяется.
async fn user_of(session: Option<tower_sessions::Session>) -> Option<Uuid> {
    match crate::ratelimit::session_user_id(session).await {
        Ok(user_id) => user_id,
        // Сессию не прочитать - запрос все равно упрется в 401 экстрактора;
        // подменять этот отказ отказом слоя незачем
        Err(error) => {
            tracing::warn!(%error, "сессия не прочитана - идемпотентность не применяется");
            None
        }
    }
}

/// Ключ журнала: пользователь + метод + путь + клиентский ключ.
///
/// Отпечаток берется целиком, а не половиной, как у счетчика попыток
/// ([`crate::ratelimit`]): там коллизия стоит одного лишнего счетчика, здесь -
/// чужого ответа. Разделитель - нулевой байт: ни в пути, ни в значении
/// заголовка его быть не может, поэтому склеить две разные тройки в одну
/// строку нельзя.
fn key_of(user_id: Uuid, method: &Method, path: &str, client_key: &str) -> String {
    let user = user_id.to_string();
    let mut digest = Sha256::new();
    for part in [
        user.as_bytes(),
        method.as_str().as_bytes(),
        path.as_bytes(),
        client_key.as_bytes(),
    ] {
        digest.update(part);
        digest.update(b"\0");
    }

    digest
        .finalize()
        .iter()
        .fold(String::from("tou:idem:"), |mut acc, byte| {
            use std::fmt::Write as _;
            let _ = write!(acc, "{byte:02x}");
            acc
        })
}

/// Ответ первой попытки как его увидит повтор.
fn replay(stored: StoredResponse) -> Response {
    let mut response = Response::new(Body::from(stored.body));
    // Записывался только разобранный статус, так что подстановка недостижима
    *response.status_mut() = StatusCode::from_u16(stored.status).unwrap_or(StatusCode::OK);

    if let Some(content_type) = stored.content_type.as_deref()
        && let Ok(value) = HeaderValue::from_str(content_type)
    {
        response.headers_mut().insert(header::CONTENT_TYPE, value);
    }
    response.headers_mut().insert(
        HeaderName::from_static(REPLAY_HEADER),
        HeaderValue::from_static("true"),
    );

    response
}

/// Записать ответ в журнал и вернуть его клиенту без изменений.
async fn finish(store: &IdempotencyStore, key: &str, response: Response) -> Response {
    let (parts, body) = response.into_parts();

    if !is_final(parts.status) || !is_json(&parts.headers) {
        store.release(key).await;
        return Response::from_parts(parts, body);
    }

    // Тело читается целиком: сюда доходят только ответы в JSON, а их
    // обработчик и так собрал в памяти. Потоковый ответ до этой строки не
    // добирается - он GET (см. заголовок модуля)
    let bytes = match to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::error!(%error, "тело ответа не прочитано");
            store.release(key).await;
            return ApiError::internal(error).into_response();
        }
    };

    match std::str::from_utf8(&bytes)
        .ok()
        .filter(|text| text.len() <= MAX_STORED_BODY_BYTES)
    {
        Some(text) => {
            store
                .record(
                    key,
                    &StoredResponse {
                        status: parts.status.as_u16(),
                        content_type: content_type(&parts.headers),
                        body: text.to_owned(),
                    },
                )
                .await;
        }
        None => store.release(key).await,
    }

    Response::from_parts(parts, Body::from(bytes))
}

/// Окончателен ли исход. Ошибка сервера и отказ счетчика - приглашение
/// повторить, и повтор обязан выполниться, а не получить вчерашний отказ.
fn is_final(status: StatusCode) -> bool {
    status.is_success()
        || (status.is_client_error()
            && status != StatusCode::TOO_MANY_REQUESTS
            && status != StatusCode::REQUEST_TIMEOUT)
}

/// Запоминается только JSON: и обычный ответ мутации, и problem+json отказа.
/// Все остальное (файл, архив, поток) - не то, что стоит держать в Redis.
fn is_json(headers: &HeaderMap) -> bool {
    content_type(headers).is_some_and(|value| {
        value.starts_with("application/json") || value.starts_with("application/problem+json")
    })
}

fn content_type(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::CONTENT_TYPE)?
        .to_str()
        .ok()
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::Json;
    use axum::Router;
    use axum::http::Request as HttpRequest;
    use axum::routing::{get, post};
    use tokio::sync::Notify;
    use tower::ServiceExt as _;
    use tower_sessions::{MemoryStore, Session, SessionManagerLayer};

    /// Журнал идемпотентности - в том же Redis, что и сессии. Без переменной
    /// проверка пропускается (машина разработчика без стенда); в пайплайне
    /// пропуск недопустим (ср. `tou_testkit`).
    async fn live_store() -> Option<IdempotencyStore> {
        match std::env::var("REDIS_URL") {
            Ok(url) if !url.trim().is_empty() => {
                let pool = crate::realtime::connect(&url)
                    .await
                    .expect("Redis дев-стенда доступен");
                Some(IdempotencyStore::new(pool))
            }
            _ => {
                assert!(
                    std::env::var_os("CI").is_none(),
                    "REDIS_URL не задан в пайплайне: идемпотентность не проверена"
                );
                None
            }
        }
    }

    /// Счетчик выполнений обработчика - тем и проверяется, что повтор не
    /// выполнил операцию второй раз.
    #[derive(Clone, Default)]
    struct Runs(Arc<AtomicUsize>);

    impl Runs {
        fn count(&self) -> usize {
            self.0.load(Ordering::SeqCst)
        }
    }

    /// Стенд слоя: «подача заявки» отвечает новым идентификатором на каждое
    /// выполнение, «сбой» - ошибкой сервера, вход кладет пользователя в
    /// сессию. Полный роутер тянет Postgres и RustFS, слой от них не зависит.
    fn stand(store: IdempotencyStore, runs: Runs, gate: Option<Arc<Gate>>) -> Router {
        let failing = runs.clone();
        let submitting = runs.clone();

        Router::new()
            .route(
                "/api/v1/tenders/{id}/applications",
                post(move || {
                    let runs = submitting.clone();
                    let gate = gate.clone();
                    async move {
                        if let Some(gate) = gate {
                            gate.entered.notify_one();
                            gate.release.notified().await;
                        }
                        let number = runs.0.fetch_add(1, Ordering::SeqCst) + 1;
                        (
                            StatusCode::CREATED,
                            Json(serde_json::json!({ "application": number })),
                        )
                    }
                }),
            )
            .route(
                "/api/v1/applications/{id}/withdraw",
                post(|| async { (StatusCode::OK, Json(serde_json::json!({ "withdrawn": true }))) }),
            )
            .route(
                "/api/v1/tenders/{id}/broken",
                post(move || {
                    let runs = failing.clone();
                    async move {
                        let number = runs.0.fetch_add(1, Ordering::SeqCst) + 1;
                        if number == 1 {
                            return ApiError::internal(std::io::Error::other("сбой"))
                                .into_response();
                        }
                        (StatusCode::OK, Json(serde_json::json!({ "run": number }))).into_response()
                    }
                }),
            )
            .route(
                "/login/{user}",
                get(
                    |session: Session, axum::extract::Path(user): axum::extract::Path<Uuid>| async move {
                        session
                            .insert(crate::extract::SESSION_USER_KEY, user)
                            .await
                            .expect("сессия записана");
                        StatusCode::NO_CONTENT
                    },
                ),
            )
            .layer(axum::middleware::from_fn_with_state(store, enforce))
            .layer(SessionManagerLayer::new(MemoryStore::default()).with_secure(false))
    }

    /// Задвижка обработчика: даёт тесту поймать момент, когда первый запрос
    /// уже выполняется, а ответа еще нет.
    #[derive(Default)]
    struct Gate {
        entered: Notify,
        release: Notify,
    }

    async fn send(app: &Router, uri: &str, cookies: &str, key: Option<&str>) -> Response {
        let mut builder = HttpRequest::builder().method("POST").uri(uri);
        if !cookies.is_empty() {
            builder = builder.header(header::COOKIE, cookies);
        }
        if let Some(key) = key {
            builder = builder.header(IDEMPOTENCY_HEADER, key);
        }
        app.clone()
            .oneshot(builder.body(Body::empty()).expect("запрос"))
            .await
            .expect("ответ")
    }

    async fn login(app: &Router, user: Uuid) -> String {
        let response = app
            .clone()
            .oneshot(
                HttpRequest::builder()
                    .uri(format!("/login/{user}"))
                    .body(Body::empty())
                    .expect("запрос"),
            )
            .await
            .expect("ответ");
        response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .filter_map(|value| value.split(';').next())
            .collect::<Vec<_>>()
            .join("; ")
    }

    async fn body_of(response: Response) -> String {
        let bytes = to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("тело ответа");
        String::from_utf8(bytes.to_vec()).expect("тело в UTF-8")
    }

    fn fresh_key() -> String {
        Uuid::now_v7().to_string()
    }

    /// Рубеж W-30: двойное нажатие «подать заявку» не создает вторую заявку
    /// и получает тот же ответ, а не отказ по правилу.
    #[tokio::test]
    async fn a_repeat_returns_the_stored_response_and_runs_nothing() {
        let Some(store) = live_store().await else {
            return;
        };
        let runs = Runs::default();
        let app = stand(store, runs.clone(), None);
        let cookies = login(&app, Uuid::now_v7()).await;
        let key = fresh_key();
        let uri = "/api/v1/tenders/0198f0d5-0000-7000-8000-000000000000/applications";

        let first = send(&app, uri, &cookies, Some(&key)).await;
        assert_eq!(first.status(), StatusCode::CREATED);
        assert!(!first.headers().contains_key(REPLAY_HEADER));
        let first_body = body_of(first).await;

        let second = send(&app, uri, &cookies, Some(&key)).await;
        assert_eq!(second.status(), StatusCode::CREATED);
        assert_eq!(
            second
                .headers()
                .get(REPLAY_HEADER)
                .and_then(|v| v.to_str().ok()),
            Some("true"),
            "повтор обязан быть опознаваем"
        );
        assert_eq!(
            second.headers().get(header::CONTENT_TYPE),
            Some(&HeaderValue::from_static("application/json")),
            "тип содержимого сохраняется"
        );
        assert_eq!(body_of(second).await, first_body, "ответ повтора не тот же");

        assert_eq!(runs.count(), 1, "операция выполнена дважды");
    }

    /// Ключ привязан к человеку и к маршруту: одинаковое значение у другого
    /// пользователя (или на другом маршруте) не должно отдавать чужой ответ -
    /// в нем сведения заявителя (NFR-07).
    #[tokio::test]
    async fn a_foreign_key_does_not_return_a_foreign_response() {
        let Some(store) = live_store().await else {
            return;
        };
        let runs = Runs::default();
        let app = stand(store, runs.clone(), None);
        let mine = login(&app, Uuid::now_v7()).await;
        let stranger = login(&app, Uuid::now_v7()).await;
        let key = fresh_key();
        let uri = "/api/v1/tenders/0198f0d5-0000-7000-8000-000000000000/applications";

        let first = body_of(send(&app, uri, &mine, Some(&key)).await).await;

        let foreign = send(&app, uri, &stranger, Some(&key)).await;
        assert!(!foreign.headers().contains_key(REPLAY_HEADER));
        assert_ne!(
            body_of(foreign).await,
            first,
            "чужой ключ вернул чужой ответ"
        );

        let other_route = send(
            &app,
            "/api/v1/applications/0198f0d5-0000-7000-8000-000000000000/withdraw",
            &mine,
            Some(&key),
        )
        .await;
        assert_eq!(
            other_route.status(),
            StatusCode::OK,
            "тот же ключ на другом маршруте - другая операция"
        );
        assert!(!other_route.headers().contains_key(REPLAY_HEADER));

        assert_eq!(runs.count(), 2, "оба выполнения обязаны состояться");
    }

    /// Без заголовка поведение прежнее: каждый запрос выполняется.
    #[tokio::test]
    async fn without_the_header_nothing_changes() {
        let Some(store) = live_store().await else {
            return;
        };
        let runs = Runs::default();
        let app = stand(store, runs.clone(), None);
        let cookies = login(&app, Uuid::now_v7()).await;
        let uri = "/api/v1/tenders/0198f0d5-0000-7000-8000-000000000000/applications";

        for _ in 0..3 {
            let response = send(&app, uri, &cookies, None).await;
            assert_eq!(response.status(), StatusCode::CREATED);
            assert!(!response.headers().contains_key(REPLAY_HEADER));
        }
        assert_eq!(runs.count(), 3);
    }

    /// Пустой ключ - не «ключа нет», а ошибка клиента: он считает, что
    /// защищен, и обязан узнать, что нет.
    #[tokio::test]
    async fn a_malformed_key_is_refused() {
        let Some(store) = live_store().await else {
            return;
        };
        let runs = Runs::default();
        let app = stand(store, runs.clone(), None);
        let cookies = login(&app, Uuid::now_v7()).await;
        let uri = "/api/v1/tenders/0198f0d5-0000-7000-8000-000000000000/applications";

        let response = send(&app, uri, &cookies, Some("   ")).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(runs.count(), 0, "операция не должна была выполниться");
    }

    /// Параллельный повтор: второй запрос приходит, пока первый выполняется.
    /// Операция обязана состояться один раз, а не два и не ноль.
    #[tokio::test]
    async fn a_parallel_repeat_is_refused_while_the_first_runs() {
        let Some(store) = live_store().await else {
            return;
        };
        let runs = Runs::default();
        let gate = Arc::new(Gate::default());
        let app = stand(store, runs.clone(), Some(gate.clone()));
        let cookies = login(&app, Uuid::now_v7()).await;
        let key = fresh_key();
        let uri = "/api/v1/tenders/0198f0d5-0000-7000-8000-000000000000/applications";

        let first = tokio::spawn({
            let (app, cookies, key) = (app.clone(), cookies.clone(), key.clone());
            async move { send(&app, uri, &cookies, Some(&key)).await }
        });

        gate.entered.notified().await;
        let parallel = send(&app, uri, &cookies, Some(&key)).await;
        assert_eq!(
            parallel.status(),
            StatusCode::CONFLICT,
            "параллельный повтор обязан получить отказ, а не выполниться"
        );

        gate.release.notify_one();
        let first = first.await.expect("первый запрос завершен");
        assert_eq!(first.status(), StatusCode::CREATED);
        assert_eq!(runs.count(), 1, "операция выполнена дважды");
    }

    /// Ошибка сервера не запоминается: повтор с тем же ключом обязан
    /// выполниться заново, иначе разовый сбой становится суточным.
    #[tokio::test]
    async fn a_server_error_is_not_pinned_for_a_day() {
        let Some(store) = live_store().await else {
            return;
        };
        let runs = Runs::default();
        let app = stand(store, runs.clone(), None);
        let cookies = login(&app, Uuid::now_v7()).await;
        let key = fresh_key();
        let uri = "/api/v1/tenders/0198f0d5-0000-7000-8000-000000000000/broken";

        let failed = send(&app, uri, &cookies, Some(&key)).await;
        assert_eq!(failed.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let retried = send(&app, uri, &cookies, Some(&key)).await;
        assert_eq!(retried.status(), StatusCode::OK);
        assert_eq!(runs.count(), 2, "повтор после сбоя обязан выполниться");
    }

    /// Ключ считается по тройке целиком: подмена любой ее части дает другой
    /// ключ журнала. Проверяется без Redis - это чистая функция.
    #[test]
    fn the_journal_key_binds_user_method_and_path() {
        let (mine, stranger) = (Uuid::now_v7(), Uuid::now_v7());
        let path = "/api/v1/tenders/0198f0d5/applications";
        let key = key_of(mine, &Method::POST, path, "abc");

        assert_eq!(key, key_of(mine, &Method::POST, path, "abc"));
        assert_ne!(key, key_of(stranger, &Method::POST, path, "abc"));
        assert_ne!(key, key_of(mine, &Method::DELETE, path, "abc"));
        assert_ne!(key, key_of(mine, &Method::POST, "/api/v1/objects", "abc"));
        assert_ne!(key, key_of(mine, &Method::POST, path, "abd"));
        assert!(key.starts_with("tou:idem:"));
        assert!(
            !key.contains(&mine.to_string()),
            "идентификатор пользователя не должен попадать в ключ Redis"
        );
    }

    /// Разделитель обязан быть таким, чтобы соседние части нельзя было
    /// «сдвинуть» друг в друга: без него `a` + `bc` и `ab` + `c` дали бы один
    /// ключ, то есть чужой ответ.
    #[test]
    fn key_parts_cannot_be_shifted_into_each_other() {
        let user = Uuid::now_v7();
        assert_ne!(
            key_of(user, &Method::POST, "/api/v1/a", "bc"),
            key_of(user, &Method::POST, "/api/v1/ab", "c")
        );
    }

    /// Под слоем - только мутации; безопасные методы проходят мимо.
    #[test]
    fn safe_methods_are_outside_the_layer() {
        for method in [Method::GET, Method::HEAD, Method::OPTIONS] {
            assert!(!is_guarded(&method), "{method} не мутация");
        }
        for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            assert!(is_guarded(&method), "{method} - мутация");
        }
    }

    /// Что запоминается, а что нет.
    #[test]
    fn only_final_outcomes_are_stored() {
        for status in [StatusCode::OK, StatusCode::CREATED, StatusCode::CONFLICT] {
            assert!(is_final(status), "{status} - окончательный исход");
        }
        for status in [
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::GATEWAY_TIMEOUT,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::TOO_MANY_REQUESTS,
        ] {
            assert!(!is_final(status), "{status} - приглашение повторить");
        }
    }

    /// Без Redis (тесты контракта, `api openapi`) слой ничего не меняет.
    #[tokio::test]
    async fn a_detached_store_changes_nothing() {
        let runs = Runs::default();
        let app = stand(IdempotencyStore::default(), runs.clone(), None);
        let cookies = login(&app, Uuid::now_v7()).await;
        let key = fresh_key();
        let uri = "/api/v1/tenders/0198f0d5-0000-7000-8000-000000000000/applications";

        for _ in 0..2 {
            let response = send(&app, uri, &cookies, Some(&key)).await;
            assert_eq!(response.status(), StatusCode::CREATED);
            assert!(!response.headers().contains_key(REPLAY_HEADER));
        }
        assert_eq!(runs.count(), 2);
    }
}
