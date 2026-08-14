//! Ограничение частоты: дологинные маршруты (NFR-07) и дорогие операции.
//!
//! `/auth/login` и `/auth/register` - единственные мутации, открытые до
//! входа (их исключает и CSRF, см. [`crate::csrf`]). Перебор пароля по ним
//! ничем не ограничен: Argon2id делает попытку дорогой для сервера, но не
//! для атакующего, у которого их миллион.
//!
//! Счетчик живет в том же Redis, что и сессии: два экземпляра api (NFR-12)
//! обязаны считать попытки вместе, иначе лимит удваивается балансировкой.
//!
//! Ключ - не сам email, а его отпечаток: Redis не место для персональных
//! данных (NFR-16), а для счетчика важно только различать обращения.
//!
//! Считаются **неудачные** попытки, а удачная счетчик обнуляет. Иначе
//! ограничение бьет не по перебору, а по работе: у входа нет причин быть
//! однократным - вторая вкладка, второе устройство, истекшая сессия, - и
//! счетчик «любых попыток» запирал бы человека, который каждый раз вводил
//! верный пароль. Так и вышло при первом же сквозном прогоне.
//!
//! Сбой Redis не запирает вход: счетчик - защита, а не правило домена,
//! и превращать его отказ в отказ системы нельзя. Промах пишется в лог.
//!
//! # Дорогие маршруты (W-29)
//!
//! Двух дологинных маршрутов мало. Печатная форма - это компиляция Typst
//! (12 шаблонов, десятки миллисекунд CPU на документ), выгрузка досье -
//! обход всего бакета с упаковкой в архив, выгрузка реестра - полный проход
//! по выборке, загрузка файла - до 20 МБ тела и запись в WORM-бакет, откуда
//! приложение уже ничего не удалит (INV-042). Все это доступно любому
//! вошедшему, и потолок времени обработки (NFR-02) от такой нагрузки не
//! спасает: он режет один зависший запрос, а не поток исправных.
//!
//! Здесь считаются **все** обращения, а не только неудачные: перебор по
//! этим маршрутам состоит как раз из успешных ответов. Субъект - сам
//! пользователь ([`subject_of`]), а не адрес: маршруты авторизованные, и за
//! одним адресом стоит весь кампус (см. [`LOGIN_PER_ADDRESS`]).
//!
//! Отказ счетчика здесь запрещает, а не пропускает - в отличие от входа.
//! Цена ошибки разная. Пропустив вход при недоступном Redis, система теряет
//! ограничение на подбор пароля, но остается работоспособной для всех, кто
//! пароль помнит; запретив - оставляет без входа весь университет, то есть
//! превращает Redis в единственную точку отказа продукта. Пропустив же
//! генерацию печатных форм, система отдает CPU тому, кто первым заметит, что
//! счетчик молчит, - и ложится целиком. Запрет стоит отложенной на секунды
//! печати документа, поэтому на дорогих маршрутах выбран он ([`RateLimiter::hit`]).

use axum::extract::{Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;
use fred::prelude::KeysInterface as _;
use sha2::{Digest as _, Sha256};
use tower_sessions::Session;
use uuid::Uuid;

use crate::error::ApiError;
use crate::realtime::Pool;

/// Сколько попыток за какое окно. Окно фиксированное: скользящее точнее,
/// но требует хранить отметки каждой попытки - для защиты от перебора
/// разница несущественная.
#[derive(Debug, Clone, Copy)]
pub struct Limit {
    pub attempts: u32,
    pub window_seconds: i64,
}

/// Перебор пароля конкретной учетной записи: десять **неудачных** попыток
/// за четверть часа. Вчетверо больше, чем нужно человеку, опечатавшемуся
/// в раскладке, и на порядки меньше, чем нужно словарю.
pub const LOGIN_PER_ACCOUNT: Limit = Limit {
    attempts: 10,
    window_seconds: 900,
};

/// Перебор по многим учетным записям с одного адреса.
///
/// Порог заметно выше, чем у учетной записи, и это не небрежность:
/// университет выходит в сеть через общий адрес, поэтому за одним IP стоит
/// весь кампус. Сотня неудач за четверть часа - это уже не «утро понедельника
/// с забытыми паролями», а перебор, но кампус в такой порог укладывается.
pub const LOGIN_PER_ADDRESS: Limit = Limit {
    attempts: 100,
    window_seconds: 900,
};

/// Массовая регистрация (FR-1504). Считаются заведенные учетные записи,
/// а не попытки: ограничивать нужно создание, а не отказ валидации у живого
/// человека. Десять за час с одного адреса - больше, чем бывает у людей,
/// и мало для бота.
pub const REGISTER_PER_ADDRESS: Limit = Limit {
    attempts: 10,
    window_seconds: 3600,
};

/// Окно дорогих маршрутов - десять минут.
///
/// Час запирал бы человека надолго за один всплеск работы, минута не отличила
/// бы всплеск от перебора: пачка документов уходит именно за минуту-другую.
const EXPENSIVE_WINDOW_SECONDS: i64 = 600;

/// Печатные формы: генерация Typst и выдача уже собранного PDF (`*.pdf`,
/// `/pdf`).
///
/// Порог считается от работы секретаря, а не от круглого числа. Самый плотный
/// эпизод - печать пачки протоколов по тендеру: протокол допуска, протокол
/// итогов, объявление, договоры и акты по каждому лоту; на тендере из десятка
/// лотов это четыре-пять десятков документов подряд, плюс повторные открытия
/// на просмотр. Сто двадцать за десять минут накрывают такой эпизод с
/// двукратным запасом и при этом отсекают поток: непрерывная печать в этот
/// порог - это документ каждые пять секунд, чего человек, читающий то, что
/// печатает, не делает.
pub const PRINT_FORMS: Limit = Limit {
    attempts: 120,
    window_seconds: EXPENSIVE_WINDOW_SECONDS,
};

/// Выгрузка досье архивом (`*.zip`, FR-1602): самая дорогая операция контура -
/// обход всех материалов тендера в бакете и упаковка.
///
/// Досье выгружают поштучно и по делу: проверяющему нужен архив по тендеру,
/// изредка по нескольким сразу. Двадцать за десять минут - это выгрузка досье
/// по всем тендерам квартала подряд, и все равно на порядок меньше, чем нужно,
/// чтобы занять хранилище чтением.
pub const DOSSIER_ARCHIVES: Limit = Limit {
    attempts: 20,
    window_seconds: EXPENSIVE_WINDOW_SECONDS,
};

/// Выгрузка реестра в CSV (`*.csv`): полный проход по выборке за период.
///
/// Реестров три (решения, договоры, поступления), у каждого - период и
/// фильтры. Тридцать за десять минут покрывают подбор периода вручную по
/// каждому реестру; выкачивание реестра в цикле в этот порог не помещается.
pub const REGISTRY_EXPORTS: Limit = Limit {
    attempts: 30,
    window_seconds: EXPENSIVE_WINDOW_SECONDS,
};

/// Загрузка файлов (вложения заявок, сканы договоров и актов, приложения
/// инвестиционного договора).
///
/// Дорога не только полоса (до 20 МБ на тело), но и то, что бакет досье -
/// WORM: удалить загруженное приложение не может (INV-042), поэтому каждая
/// лишняя загрузка занимает место навсегда. Участник прикладывает к заявке
/// единицы файлов, секретарь - сканы по нескольким договорам подряд;
/// шестьдесят за десять минут покрывают и то, и другое, включая повторные
/// попытки после обрыва связи.
pub const FILE_UPLOADS: Limit = Limit {
    attempts: 60,
    window_seconds: EXPENSIVE_WINDOW_SECONDS,
};

/// Пауза, которую сервер называет клиенту, когда счетчик недоступен.
///
/// Это не окно лимита: ограничение не сработало, сломался счетчик, и повтор
/// имеет смысл сразу после того, как Redis вернется. Просить клиента ждать
/// десять минут из-за чужой аварии незачем.
const OUTAGE_RETRY_AFTER_SECONDS: u64 = 5;

/// Счетчик попыток. `None` - Redis не подключен (тесты, `api openapi`):
/// проверка становится пустой операцией.
#[derive(Clone, Default)]
pub struct RateLimiter {
    pool: Option<Pool>,
}

impl RateLimiter {
    pub fn new(pool: Pool) -> Self {
        Self { pool: Some(pool) }
    }

    /// Отказать, если лимит уже исчерпан. Сама проверка ничего не считает:
    /// попытку засчитывает [`RateLimiter::record`], и только ту, которая
    /// того заслуживает.
    ///
    /// `bucket` разделяет счетчики разных проверок, `subject` - обращения
    /// внутри одной (учетная запись, адрес).
    ///
    /// Недоступный Redis здесь пропускает: вход - последнее, что можно
    /// закрывать из-за аварии счетчика. На дорогих маршрутах решение
    /// обратное, и почему - в заголовке модуля и у [`RateLimiter::hit`].
    pub async fn check(&self, bucket: &str, subject: &str, limit: Limit) -> Result<(), ApiError> {
        let Some(pool) = self.pool.as_ref() else {
            return Ok(());
        };
        let key = key_of(bucket, subject);

        let count: Option<i64> = match pool.get(&key).await {
            Ok(count) => count,
            Err(error) => {
                tracing::warn!(%error, bucket, "счетчик попыток недоступен - проверка пропущена");
                return Ok(());
            }
        };

        if count.unwrap_or(0) >= i64::from(limit.attempts) {
            let ttl: i64 = pool.ttl(&key).await.unwrap_or(limit.window_seconds);
            tracing::warn!(bucket, "лимит попыток исчерпан");
            return Err(ApiError::TooManyRequests {
                retry_after_seconds: ttl.clamp(1, limit.window_seconds).unsigned_abs(),
            });
        }

        Ok(())
    }

    /// Засчитать попытку: неудачный вход или заведенную учетную запись.
    pub async fn record(&self, bucket: &str, subject: &str, limit: Limit) {
        let Some(pool) = self.pool.as_ref() else {
            return;
        };
        let key = key_of(bucket, subject);

        let count: i64 = match pool.incr(&key).await {
            Ok(count) => count,
            Err(error) => {
                tracing::warn!(%error, bucket, "счетчик попыток недоступен - попытка не учтена");
                return;
            }
        };

        // Срок ставится на первой попытке окна: продление на каждой
        // превратило бы окно в вечную блокировку
        if count == 1
            && let Err(error) = pool.expire::<(), _>(&key, limit.window_seconds, None).await
        {
            // Ключ без срока запер бы учетную запись навсегда - лучше снять
            tracing::warn!(%error, bucket, "срок счетчика не поставлен - счетчик сбрасывается");
            let _: Result<(), _> = pool.del(&key).await;
        }
    }

    /// Засчитать обращение и сразу отказать, если порог перейден.
    ///
    /// Отличается от пары [`RateLimiter::check`] + [`RateLimiter::record`]
    /// двумя вещами, и обе существенны для дорогих маршрутов.
    ///
    /// Считается каждое обращение, а не только неудачное: перебор печатных
    /// форм состоит из успешных ответов, и «считать промахи» здесь означало
    /// бы не считать ничего.
    ///
    /// Счет и проверка - одна операция. `check`, стоящий до `record`,
    /// пропускает столько параллельных запросов, сколько успели прочитать
    /// счетчик до первой записи; на дологинном маршруте это несколько лишних
    /// попыток пароля, а здесь - ровно тот залп, от которого защита и
    /// ставится. `INCR` атомарен, поэтому залп считается целиком.
    ///
    /// Недоступность Redis приводит к отказу (см. заголовок модуля). Случай
    /// «пул не подключен» - другой: так собирается состояние в тестах и в
    /// `api openapi`, ограничивать там нечего и нечем.
    pub async fn hit(&self, bucket: &str, subject: &str, limit: Limit) -> Result<(), ApiError> {
        let Some(pool) = self.pool.as_ref() else {
            return Ok(());
        };
        let key = key_of(bucket, subject);

        let count: i64 = match pool.incr(&key).await {
            Ok(count) => count,
            Err(error) => {
                tracing::warn!(%error, bucket, "счетчик обращений недоступен - обращение отклонено");
                return Err(ApiError::TooManyRequests {
                    retry_after_seconds: OUTAGE_RETRY_AFTER_SECONDS,
                });
            }
        };

        // Срок - на первом обращении окна: продление на каждом превратило бы
        // окно в вечную блокировку (см. `record`)
        if count == 1
            && let Err(error) = pool.expire::<(), _>(&key, limit.window_seconds, None).await
        {
            tracing::warn!(%error, bucket, "срок счетчика не поставлен - счетчик сбрасывается");
            let _: Result<(), _> = pool.del(&key).await;
        }

        // Обращение уже засчитано, поэтому порог перейден на `attempts + 1`:
        // ровно `attempts` обращений в окне обязаны проходить
        if count > i64::from(limit.attempts) {
            let ttl: i64 = pool.ttl(&key).await.unwrap_or(limit.window_seconds);
            tracing::warn!(bucket, "лимит обращений исчерпан");
            return Err(ApiError::TooManyRequests {
                retry_after_seconds: ttl.clamp(1, limit.window_seconds).unsigned_abs(),
            });
        }

        Ok(())
    }

    /// Обнулить счетчик: удачный вход снимает подозрение с учетной записи.
    pub async fn forget(&self, bucket: &str, subject: &str) {
        let Some(pool) = self.pool.as_ref() else {
            return;
        };
        if let Err(error) = pool.del::<(), _>(key_of(bucket, subject)).await {
            tracing::warn!(%error, bucket, "счетчик попыток не сброшен");
        }
    }
}

/// Middleware дорогих маршрутов (W-29): слой накладывается на весь роутер,
/// а решает [`cost_of`] - маршрут, не попавший в перечень, проходит без
/// единого обращения к Redis.
///
/// Слой стоит снаружи CSRF: обращение, отбитое проверкой токена, - такой же
/// повод считать, как и обычное, а вот успевать выполнить дорогую работу до
/// счетчика нельзя.
pub async fn enforce(
    State(limiter): State<RateLimiter>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if let Some((bucket, limit)) = cost_of(request.method(), request.uri().path()) {
        // Все, что нужно для опознания, снимается с запроса до первого await:
        // тело запроса не Sync, и ссылка на него не пережила бы точку
        // ожидания (будущее слоя обязано быть Send)
        let session = request.extensions().get::<Session>().cloned();
        let address = client_address(request.headers());

        if let Some(subject) = subject_of(session, address).await? {
            limiter.hit(bucket, &subject, limit).await?;
        }
    }

    Ok(next.run(request).await)
}

/// Дорогой ли это маршрут и с каким порогом.
///
/// Сверка идет по форме пути, а не по перечню из полутора сотен строк:
/// перечень разошелся бы с реестром маршрутов при первой же новой печатной
/// форме, и разошелся бы молча - в сторону «лимита нет». Форма пути в этом
/// API - часть контракта (ТЗ § 7): печатная форма всегда оканчивается на
/// `.pdf` либо `/pdf`, архив досье - на `.zip`, выгрузка реестра - на `.csv`.
fn cost_of(method: &Method, path: &str) -> Option<(&'static str, Limit)> {
    if path.ends_with(".pdf") || path.ends_with("/pdf") {
        return Some(("print", PRINT_FORMS));
    }
    if path.ends_with(".zip") {
        return Some(("archive", DOSSIER_ARCHIVES));
    }
    if path.ends_with(".csv") {
        return Some(("export", REGISTRY_EXPORTS));
    }
    if is_upload(method, path) {
        return Some(("upload", FILE_UPLOADS));
    }
    None
}

/// Загрузка файла: те же пути отдают файл по GET, поэтому метод здесь -
/// часть признака, а не формальность.
fn is_upload(method: &Method, path: &str) -> bool {
    (*method == Method::POST || *method == Method::PUT)
        && (path.ends_with("/files") || path.ends_with("/scan") || path.contains("/attachments/"))
}

/// Кого считать. Пользователь из сессии; для публичных маршрутов (объявление
/// открыто и анониму) - адрес обращения.
///
/// Ни того, ни другого - маршрут не ограничивается. Общая корзина «на всех
/// неопознанных» была бы не защитой, а самоблокировкой: первый же обход
/// порога запирал бы публичные печатные формы для всех сразу. В проде адрес
/// есть всегда - его ставит Caddy (см. [`client_address`]).
async fn subject_of(
    session: Option<Session>,
    address: Option<String>,
) -> Result<Option<String>, ApiError> {
    match session_user_id(session).await {
        Ok(Some(user_id)) => Ok(Some(user_id.to_string())),
        Ok(None) => Ok(address),
        // Сессия не прочитана - это тот самый недоступный Redis: опознать
        // обращение нечем, а маршрут дорогой (см. заголовок модуля)
        Err(error) => {
            tracing::warn!(%error, "сессия не прочитана - обращение к дорогому маршруту отклонено");
            Err(ApiError::TooManyRequests {
                retry_after_seconds: OUTAGE_RETRY_AFTER_SECONDS,
            })
        }
    }
}

/// Пользователь текущего запроса по сессии - для слоев, которые работают до
/// разбора маршрута и потому не могут взять экстрактор [`crate::extract::CurrentUser`]
/// (он ходит в БД за ролями, а слою нужен только идентификатор).
///
/// Сессия берется из расширений запроса (`request.extensions().get::<Session>()`),
/// как в [`crate::csrf::enforce`], и передается сюда копией: экстрактор
/// превратил бы любую ошибку сессионного слоя в 500 еще до того, как слой
/// решит, нужна ли ему сессия вообще. `None` - сессионного слоя нет вовсе
/// (стенды, тесты слоев).
pub(crate) async fn session_user_id(
    session: Option<Session>,
) -> Result<Option<Uuid>, tower_sessions::session::Error> {
    match session {
        Some(session) => session.get::<Uuid>(crate::extract::SESSION_USER_KEY).await,
        None => Ok(None),
    }
}

fn key_of(bucket: &str, subject: &str) -> String {
    format!("tou:rl:{bucket}:{}", fingerprint(subject))
}

/// Отпечаток обращения: в Redis уходит он, а не email или адрес (NFR-16).
fn fingerprint(subject: &str) -> String {
    let digest = Sha256::digest(subject.as_bytes());
    // Половины хеша хватает: коллизия здесь стоит одного лишнего счетчика
    digest[..16].iter().fold(String::new(), |mut acc, byte| {
        use std::fmt::Write as _;
        let _ = write!(acc, "{byte:02x}");
        acc
    })
}

/// Адрес клиента для прода: единственная точка входа - Caddy, он и ставит
/// `X-Forwarded-For`. Берется первый элемент - его пишет сам прокси.
///
/// Заголовок подделывается кем угодно, поэтому на нем висит только
/// вспомогательный лимит по адресу; основной - по учетной записи, он
/// заголовков не требует. Без заголовка (прямое обращение на дев-стенде)
/// проверки по адресу нет: общая корзина на всех превратила бы лимит
/// в самоблокировку.
pub fn client_address(headers: &axum::http::HeaderMap) -> Option<String> {
    let raw = headers.get("x-forwarded-for")?.to_str().ok()?;
    let first = raw.split(',').next()?.trim();
    (!first.is_empty()).then(|| first.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::{HeaderMap, Request as HttpRequest, StatusCode, header};
    use axum::routing::get;
    use tower::ServiceExt as _;
    use tower_sessions::{MemoryStore, SessionManagerLayer};

    #[test]
    fn fingerprint_hides_subject_and_is_stable() {
        let subject = "participant@tou.edu.kz";
        let digest = fingerprint(subject);

        assert_eq!(digest, fingerprint(subject));
        assert_ne!(digest, fingerprint("other@tou.edu.kz"));
        assert!(
            !digest.contains("tou.edu.kz"),
            "email не должен попасть в ключ"
        );
        assert_eq!(digest.len(), 32, "16 байт в hex");
    }

    #[test]
    fn address_comes_from_first_forwarded_hop() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.7, 10.0.0.1".parse().unwrap());
        assert_eq!(client_address(&headers).as_deref(), Some("203.0.113.7"));
    }

    #[test]
    fn address_is_absent_without_proxy() {
        assert_eq!(client_address(&HeaderMap::new()), None);
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "  ".parse().unwrap());
        assert_eq!(client_address(&headers), None);
    }

    /// Без Redis (тесты контракта, `api openapi`) проверка не должна ни
    /// падать, ни блокировать - в том числе на дорогих маршрутах: «пул не
    /// подключен» и «Redis не отвечает» - разные случаи.
    #[tokio::test]
    async fn detached_limiter_allows_everything() {
        let limiter = RateLimiter::default();
        for _ in 0..100 {
            assert!(
                limiter
                    .check("login:account", "a@b.kz", super::LOGIN_PER_ACCOUNT)
                    .await
                    .is_ok()
            );
            limiter
                .record("login:account", "a@b.kz", super::LOGIN_PER_ACCOUNT)
                .await;
            limiter.forget("login:account", "a@b.kz").await;
            assert!(limiter.hit("print", "someone", PRINT_FORMS).await.is_ok());
        }
    }

    /// Порог по адресу обязан быть заметно выше, чем по учетной записи:
    /// за одним IP стоит весь кампус, и уравнивать их значит запирать
    /// университет из-за десятка забытых паролей.
    #[test]
    fn address_threshold_leaves_room_for_a_shared_egress() {
        const {
            assert!(
                super::LOGIN_PER_ADDRESS.attempts >= super::LOGIN_PER_ACCOUNT.attempts * 5,
                "порог по адресу не оставляет запаса под общий выход в сеть"
            )
        };
    }

    /// Перечень дорогих маршрутов - по форме пути (ТЗ § 7). Проверяются
    /// настоящие пути реестра `lib.rs`, а не выдуманные.
    #[test]
    fn expensive_routes_are_recognised_by_their_path() {
        for path in [
            "/api/v1/tenders/0198f0d5-0000-7000-8000-000000000000/announcement.pdf",
            "/api/v1/tenders/0198f0d5-0000-7000-8000-000000000000/results-protocol.pdf",
            "/api/v1/special-requests/0198f0d5-0000-7000-8000-000000000000/decision.pdf",
            "/api/v1/amendments/0198f0d5-0000-7000-8000-000000000000/pdf",
            "/api/v1/contracts/0198f0d5-0000-7000-8000-000000000000/pdf",
        ] {
            assert_eq!(
                cost_of(&Method::GET, path).map(|(bucket, _)| bucket),
                Some("print"),
                "{path} - печатная форма"
            );
        }

        assert_eq!(
            cost_of(
                &Method::GET,
                "/api/v1/tenders/0198f0d5-0000-7000-8000-000000000000/dossier.zip"
            )
            .map(|(bucket, _)| bucket),
            Some("archive")
        );
        assert_eq!(
            cost_of(&Method::GET, "/api/v1/reports/contracts/export.csv").map(|(bucket, _)| bucket),
            Some("export")
        );
        for path in [
            "/api/v1/applications/0198f0d5-0000-7000-8000-000000000000/files",
            "/api/v1/special-requests/0198f0d5-0000-7000-8000-000000000000/files",
            "/api/v1/contracts/0198f0d5-0000-7000-8000-000000000000/scan",
            "/api/v1/acts/0198f0d5-0000-7000-8000-000000000000/scan",
            "/api/v1/investment-contracts/0198f0d5-0000-7000-8000-000000000000/attachments/plan",
        ] {
            assert_eq!(
                cost_of(&Method::POST, path).map(|(bucket, _)| bucket),
                Some("upload"),
                "{path} - загрузка файла"
            );
        }
    }

    /// Обычная работа кабинета мимо счетчика не идет вовсе: реестры,
    /// карточки и ставки - это не дорогие маршруты, и лишний поход в Redis
    /// на каждый такой запрос был бы платой ни за что.
    #[test]
    fn ordinary_routes_are_not_counted() {
        for (method, path) in [
            (Method::GET, "/api/v1/tenders"),
            (Method::GET, "/api/v1/reports/contracts"),
            (Method::POST, "/api/v1/auctions/0198f0d5/bids"),
            (Method::POST, "/api/v1/tenders/0198f0d5/applications"),
            // Тот же путь, что у загрузки, но выдача файла - не загрузка
            (Method::GET, "/api/v1/applications/0198f0d5/files"),
            (
                Method::GET,
                "/api/v1/investment-contracts/0198f0d5/attachments/plan",
            ),
        ] {
            assert!(
                cost_of(&method, path).is_none(),
                "{method} {path} не должен попадать под лимит дорогих маршрутов"
            );
        }
    }

    /// Пороги соотнесены с ценой операции, а не выставлены поштучно наугад:
    /// архив досье дороже печатной формы, и порог у него обязан быть ниже.
    #[test]
    fn thresholds_follow_the_cost_of_the_work() {
        const {
            assert!(
                DOSSIER_ARCHIVES.attempts < REGISTRY_EXPORTS.attempts
                    && REGISTRY_EXPORTS.attempts < FILE_UPLOADS.attempts
                    && FILE_UPLOADS.attempts < PRINT_FORMS.attempts,
                "пороги разошлись с порядком стоимости операций"
            )
        };
        const {
            assert!(
                PRINT_FORMS.attempts >= 2 * SECRETARY_BATCH,
                "пачка протоколов по тендеру не укладывается в порог печати"
            )
        };
    }

    /// Пачка документов, которую секретарь печатает за один заход по тендеру
    /// из десятка лотов: протоколы, объявление, договоры и акты.
    const SECRETARY_BATCH: u32 = 45;

    /// Счетчик дорогих маршрутов - в том же Redis, что и сессии. Переменной
    /// нет - проверка пропускается (машина разработчика без стенда); в
    /// пайплайне пропуск недопустим, там сервис redis объявлен, и молчаливый
    /// пропуск превратил бы рубеж в зеленый прочерк (ср. `tou_testkit`).
    async fn live_limiter() -> Option<RateLimiter> {
        match std::env::var("REDIS_URL") {
            Ok(url) if !url.trim().is_empty() => {
                let pool = crate::realtime::connect(&url)
                    .await
                    .expect("Redis дев-стенда доступен");
                Some(RateLimiter::new(pool))
            }
            _ => {
                assert!(
                    std::env::var_os("CI").is_none(),
                    "REDIS_URL не задан в пайплайне: счетчик дорогих маршрутов не проверен"
                );
                None
            }
        }
    }

    /// Субъект на каждый прогон свой: счетчики живут в общем Redis стенда,
    /// и прогоны не должны наследовать чужие остатки.
    fn subject() -> String {
        format!("test-{}", Uuid::now_v7())
    }

    /// Рубеж W-29: рабочая пачка проходит целиком, а поток упирается ровно
    /// на пороге - и получает паузу, а не голое «нельзя».
    #[tokio::test]
    async fn print_limit_admits_a_batch_and_stops_a_flood() {
        let Some(limiter) = live_limiter().await else {
            return;
        };
        let subject = subject();

        for number in 1..=SECRETARY_BATCH {
            assert!(
                limiter.hit("print", &subject, PRINT_FORMS).await.is_ok(),
                "документ {number} пачки секретаря уперся в лимит"
            );
        }
        for _ in SECRETARY_BATCH..PRINT_FORMS.attempts {
            assert!(limiter.hit("print", &subject, PRINT_FORMS).await.is_ok());
        }

        match limiter.hit("print", &subject, PRINT_FORMS).await {
            Err(ApiError::TooManyRequests {
                retry_after_seconds,
            }) => {
                assert!(retry_after_seconds >= 1);
                assert!(
                    retry_after_seconds <= PRINT_FORMS.window_seconds.unsigned_abs(),
                    "пауза длиннее окна: {retry_after_seconds}"
                );
            }
            other => panic!("порог печати не сработал: {other:?}"),
        }

        limiter.forget("print", &subject).await;
    }

    /// Счетчики разных корзин независимы: исчерпанная печать не должна
    /// запирать выгрузку реестра тому же человеку.
    #[tokio::test]
    async fn buckets_do_not_share_a_counter() {
        let Some(limiter) = live_limiter().await else {
            return;
        };
        let subject = subject();

        for _ in 0..DOSSIER_ARCHIVES.attempts {
            assert!(
                limiter
                    .hit("archive", &subject, DOSSIER_ARCHIVES)
                    .await
                    .is_ok()
            );
        }
        assert!(
            limiter
                .hit("archive", &subject, DOSSIER_ARCHIVES)
                .await
                .is_err()
        );
        assert!(
            limiter
                .hit("export", &subject, REGISTRY_EXPORTS)
                .await
                .is_ok(),
            "выгрузка реестра не должна зависеть от счетчика архивов"
        );

        limiter.forget("archive", &subject).await;
        limiter.forget("export", &subject).await;
    }

    /// Стенд слоя: дорогой маршрут, обычный маршрут и вход, кладущий
    /// пользователя в сессию. Полный роутер тянет Postgres и RustFS -
    /// проверять на нем один слой нельзя, а слой от них не зависит.
    fn stand(limiter: RateLimiter) -> Router {
        Router::new()
            .route(
                "/api/v1/tenders/{id}/dossier.zip",
                get(|| async { "архив" }),
            )
            .route("/api/v1/tenders/{id}", get(|| async { "карточка" }))
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
            .layer(axum::middleware::from_fn_with_state(limiter, enforce))
            .layer(SessionManagerLayer::new(MemoryStore::default()).with_secure(false))
    }

    async fn send(app: &Router, uri: &str, cookies: &str) -> Response {
        let mut builder = HttpRequest::builder()
            .uri(uri)
            .header("x-forwarded-for", "10.0.0.7");
        if !cookies.is_empty() {
            builder = builder.header(header::COOKIE, cookies);
        }
        app.clone()
            .oneshot(builder.body(Body::empty()).expect("запрос"))
            .await
            .expect("ответ")
    }

    /// Cookie сессии из ответа входа - дальше она и опознает пользователя.
    fn session_cookie(response: &Response) -> String {
        response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .filter_map(|value| value.split(';').next())
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Ключ счетчика - пользователь, а не адрес: секретарь, исчерпавший
    /// выгрузку досье, не должен запирать коллегу за тем же университетским
    /// выходом в сеть. Заодно проверяется, что обычный маршрут не считается.
    #[tokio::test]
    async fn the_layer_counts_users_and_not_addresses() {
        let Some(limiter) = live_limiter().await else {
            return;
        };
        let app = stand(limiter.clone());
        let (first, second) = (Uuid::now_v7(), Uuid::now_v7());

        let mine = session_cookie(&send(&app, &format!("/login/{first}"), "").await);
        let colleague = session_cookie(&send(&app, &format!("/login/{second}"), "").await);

        let archive = "/api/v1/tenders/0198f0d5-0000-7000-8000-000000000000/dossier.zip";
        for number in 1..=DOSSIER_ARCHIVES.attempts {
            assert_eq!(
                send(&app, archive, &mine).await.status(),
                StatusCode::OK,
                "выгрузка {number} уперлась в лимит раньше порога"
            );
        }

        let refused = send(&app, archive, &mine).await;
        assert_eq!(refused.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(
            refused.headers().contains_key(header::RETRY_AFTER),
            "отказ обязан называть паузу"
        );

        assert_eq!(
            send(&app, archive, &colleague).await.status(),
            StatusCode::OK,
            "счетчик коллеги не должен зависеть от чужого"
        );
        assert_eq!(
            send(
                &app,
                "/api/v1/tenders/0198f0d5-0000-7000-8000-000000000000",
                &mine
            )
            .await
            .status(),
            StatusCode::OK,
            "обычный маршрут под лимит дорогих не попадает"
        );

        limiter.forget("archive", &first.to_string()).await;
        limiter.forget("archive", &second.to_string()).await;
    }
}
