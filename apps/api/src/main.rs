//! Точка сборки HTTP-сервера: композиция крейтов (арх. § 4).
//! REST + SSE-уведомления + WS-аукцион; вся логика - в `crates/*`.
//!
//! Подкоманды: `api` (сервер), `api migrate` (накат миграций, superuser),
//! `api seed` (seed-аккаунты ролей, Прил. Б; пароль - env SEED_PASSWORD).

use anyhow::Context;
use argon2::Argon2;
use argon2::password_hash::PasswordHasher as _;
use argon2::password_hash::phc::PasswordHash;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    match std::env::args().nth(1).as_deref() {
        None | Some("serve") => serve().await,
        Some("migrate") => migrate().await,
        Some("seed") => seed().await,
        Some("demo-tender") => demo_tender().await,
        Some("demo-summed-up") => demo_summed_up().await,
        Some("demo-single-application") => demo_single_application().await,
        Some("demo-special-site") => demo_special_site().await,
        Some("time-shift") => time_shift().await,
        Some("openapi") => openapi(),
        Some(other) => anyhow::bail!(
            "неизвестная подкоманда: {other} (serve | migrate | seed | demo-tender | \
             demo-summed-up | demo-single-application | demo-special-site | openapi)"
        ),
    }
}

/// `api time-shift <интервал>` - сдвиг часов стенда (T68, ADR-0005).
///
/// Второй рубеж из трех: явное намерение. Без `ALLOW_TIME_SHIFT=1` подкоманда
/// отказывает и ничего не делает; прод переменную не задает. Первый рубеж -
/// право (роль приложения сдвинуть часы не может в принципе), третий - след
/// в аудите. Подключение идет владельцем БД, как у миграций.
///
/// `api time-shift reset` возвращает обычные часы.
async fn time_shift() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var("ALLOW_TIME_SHIFT").is_ok_and(|v| v == "1" || v == "true"),
        "сдвиг часов запрещен: переменная ALLOW_TIME_SHIFT не задана (ADR-0005).          На проде она не задается никогда"
    );

    let raw = std::env::args()
        .nth(2)
        .context("укажите интервал: `api time-shift '11 days'` либо `reset`")?;
    let interval = if raw == "reset" { "0".to_owned() } else { raw };

    let database_url = env("DATABASE_URL")?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .context("подключение PostgreSQL (time-shift)")?;

    let who = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "cli".to_owned());
    let seconds = tou_db::refdata::set_clock_shift(&pool, &interval, &who)
        .await
        .context("сдвиг часов стенда")?;

    tracing::info!(
        interval,
        seconds,
        "часы стенда сдвинуты (ADR-0005); след записан в audit.log"
    );
    Ok(())
}

/// Печатает OpenAPI 3.1 контракт - вход цепочки кодогена G5
/// (`packages/api-client/openapi.json`).
fn openapi() -> anyhow::Result<()> {
    use std::io::Write as _;
    let json = tou_http::openapi().to_pretty_json()?;
    std::io::stdout().write_all(json.as_bytes())?;
    std::io::stdout().write_all(b"\n")?;
    Ok(())
}

/// Длительность торгов (FR-602, п. 66 - 60 минут). Для демо укорачивается
/// `api --demo-timer 5m` (ТЗ § 9, Т11) или переменной `AUCTION_TIMER`;
/// принимаются формы `5m`, `2h`, `45`.
fn auction_minutes() -> anyhow::Result<i64> {
    let mut args = std::env::args().skip(1);
    let raw = loop {
        match args.next() {
            Some(arg) if arg == "--demo-timer" => break args.next(),
            Some(arg) => {
                if let Some(value) = arg.strip_prefix("--demo-timer=") {
                    break Some(value.to_owned());
                }
            }
            None => break std::env::var("AUCTION_TIMER").ok(),
        }
    };

    match raw {
        Some(raw) => parse_duration(&raw),
        None => Ok(tou_domain::auction::DEFAULT_DURATION_MINUTES),
    }
}

/// `5m` / `2h` / `45` → минуты.
fn parse_duration(raw: &str) -> anyhow::Result<i64> {
    let (digits, multiplier) = match raw.strip_suffix('h') {
        Some(hours) => (hours, 60),
        None => (raw.strip_suffix('m').unwrap_or(raw), 1),
    };
    let value: i64 = digits
        .trim()
        .parse()
        .with_context(|| format!("длительность торгов «{raw}»: ожидались 5m, 2h или 45"))?;
    anyhow::ensure!(value > 0, "длительность торгов должна быть положительной");

    Ok(value * multiplier)
}

fn env(name: &str) -> anyhow::Result<String> {
    std::env::var(name).with_context(|| format!("переменная окружения {name} не задана"))
}

async fn serve() -> anyhow::Result<()> {
    let database_url = env("DATABASE_URL")?;
    let redis_url = env("REDIS_URL")?;
    let secure_cookies = std::env::var("COOKIE_SECURE").is_ok_and(|v| v == "1" || v == "true");

    let db = tou_db::connect(&database_url)
        .await
        .context("подключение PostgreSQL")?;
    // Один пул Redis на процесс: сессии и шина реалтайма (T58) делят его
    let redis = tou_http::realtime::connect(&redis_url)
        .await
        .map_err(|e| anyhow::anyhow!("подключение Redis: {e}"))?;
    let sessions = tou_http::session_layer(redis.clone(), secure_cookies);
    let storage = tou_http::storage::connect(&tou_http::storage::StorageConfig::from_env())
        .context("подключение RustFS (S3)")?;

    // Внешний провайдер идентичности (FR-1502, ADR-0003); без OIDC_* - вход
    // остается локальным, как в контуре 1
    let oidc = tou_http::oidc::OidcProvider::from_env();
    if let Some(provider) = oidc.as_ref() {
        tracing::info!(
            issuer = provider.config().issuer,
            "провайдер идентичности подключен"
        );
    }

    let state = tou_http::AppState::new(db, storage)
        .with_auction_minutes(auction_minutes()?)
        .with_secure_cookies(secure_cookies)
        // Счетчик попыток и журнал идемпотентности - в том же Redis, что
        // и сессии (NFR-12): при двух экземплярах api оба обязаны быть общими
        .with_redis(redis.clone())
        // Источники апгрейда WS-комнаты торгов (FR-603): окружение разбирает
        // композиция, как и остальную конфигурацию стенда
        .with_ws_origins(tou_http::state::allowed_ws_origins_from_env())
        .with_oidc(oidc);

    // Реалтайм между экземплярами (NFR-12): публикация уходит в Redis
    // и возвращается всем, включая этот процесс. Без Redis обработчики
    // работали бы только со своими подписчиками
    state
        .attach_realtime(&redis, &redis_url)
        .await
        .map_err(|e| anyhow::anyhow!("подписка на шину реалтайма: {e}"))?;

    let app = tou_http::router(state.clone()).layer(sessions);

    // Swagger UI в dev: `cargo run -p api --features swagger` (арх. § 5)
    #[cfg(feature = "swagger")]
    let app = app.merge(
        utoipa_swagger_ui::SwaggerUi::new("/docs").url("/docs/openapi.json", tou_http::openapi()),
    );

    let addr = std::env::var("API_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_owned());
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    tracing::info!(%addr, "api listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("serve api")?;

    tracing::info!("api остановлен");
    Ok(())
}

/// Сигнал остановки: SIGTERM от оркестратора, Ctrl+C у разработчика.
///
/// Без него процесс умирает вместе с открытыми запросами. Для этой системы
/// это не абстрактный вред: `deploy.sh` специально не выкатывает релиз, пока
/// идет комната торгов (п. 63–68), а обрыв на обычном перезапуске сводил бы
/// эту предосторожность на нет.
///
/// Ошибка подписки на сигнал не должна выигрывать гонку `select!` - иначе
/// сервер завершится сразу после старта, поэтому ветка уходит в вечное
/// ожидание, а причина остается в логе.
async fn shutdown_signal() {
    let interrupt = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "подписка на Ctrl+C не удалась");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "подписка на SIGTERM не удалась");
                std::future::pending::<()>().await;
            }
        }
    };

    // На Windows (дев-хост) SIGTERM нет - остается Ctrl+C
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = interrupt => tracing::info!("получен Ctrl+C - завершение начатых запросов"),
        () = terminate => tracing::info!("получен SIGTERM - завершение начатых запросов"),
    }
}

/// Миграции выполняются под владельцем схемы (superuser в dev) - без
/// `SET ROLE tou_rent_app`, поэтому пул здесь отдельный (A-011).
async fn migrate() -> anyhow::Result<()> {
    let database_url = env("DATABASE_URL")?;
    let dir = std::env::var("MIGRATIONS_DIR").unwrap_or_else(|_| "crates/db/migrations".to_owned());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .context("подключение PostgreSQL (migrate)")?;

    let migrator = sqlx::migrate::Migrator::new(std::path::Path::new(&dir))
        .await
        .with_context(|| format!("чтение миграций из {dir}"))?;
    migrator.run(&pool).await.context("применение миграций")?;

    // INV-040 (п. 40): цены, записанные до перехода на шифрование,
    // дошифровываются ключом приложения - под владельцем схемы, потому что
    // роли приложения менять ценовое предложение не разрешено
    let encrypted = tou_db::prices::encrypt_pending(&pool)
        .await
        .context("дошифровка ценовых предложений")?;
    if encrypted > 0 {
        tracing::info!(count = encrypted, "ценовые предложения зашифрованы");
    }

    tracing::info!(dir, "миграции применены");
    Ok(())
}

async fn seed() -> anyhow::Result<()> {
    let database_url = env("DATABASE_URL")?;
    // NFR-09: пароль не хранится в репозитории
    let password = env("SEED_PASSWORD")?;
    anyhow::ensure!(
        password.chars().count() >= 12,
        "SEED_PASSWORD короче 12 символов"
    );

    let password_hash: PasswordHash = Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(|e| anyhow::anyhow!("argon2: {e}"))?;
    let password_hash = password_hash.to_string();

    let db = tou_db::connect(&database_url)
        .await
        .context("подключение PostgreSQL")?;
    let created = tou_db::seed::seed_accounts(&db, &password_hash)
        .await
        .context("seed-аккаунты")?;
    let commission_created = tou_db::seed::seed_commission(&db)
        .await
        .context("seed демо-комиссии")?;
    // Прил. Б: демо-объекты и тендеры портала во всех статусах + «горячий»
    // тендер § 9.1 с тремя поданными заявками
    let objects = tou_db::seed::seed_objects(&db)
        .await
        .context("seed демо-объектов")?;
    let tenders = tou_db::seed::seed_tenders(&db)
        .await
        .context("seed демо-тендеров")?;

    tracing::info!(
        created,
        commission_created,
        objects,
        tenders = tenders.created,
        demo_tender = ?tenders.demo_tender_id,
        "seed завершен (повторный запуск безопасен)"
    );
    Ok(())
}

/// `api demo-tender [заголовок]` - свежий тендер под прогон e2e (T14):
/// три заявки поданы, прием закрыт, время заседания наступило. Печатает id
/// в stdout (единственная строка) - сценарий берет его оттуда.
async fn demo_tender() -> anyhow::Result<()> {
    use std::io::Write as _;

    let title = std::env::args().nth(2).unwrap_or_else(|| {
        format!(
            "E2E § 9.1 - аудитория 42 м² ({})",
            uuid::Uuid::now_v7().simple()
        )
    });

    let db = tou_db::connect(&env("DATABASE_URL")?)
        .await
        .context("подключение PostgreSQL")?;
    let tender_id = tou_db::seed::seed_demo_tender(&db, &title)
        .await
        .context("подготовка тендера сценария")?;

    writeln!(std::io::stdout(), "{tender_id}")?;
    Ok(())
}

/// `api demo-summed-up [заголовок]` - тендер с завершенными торгами
/// (T32): площадка сценариев «полный тендер до договора» и «уклонение
/// победителя → № 2». Печатает id в stdout.
async fn demo_summed_up() -> anyhow::Result<()> {
    use std::io::Write as _;

    let title = std::env::args().nth(2).unwrap_or_else(|| {
        format!(
            "E2E контур 2 - итоги торгов ({})",
            uuid::Uuid::now_v7().simple()
        )
    });

    let db = tou_db::connect(&env("DATABASE_URL")?)
        .await
        .context("подключение PostgreSQL")?;
    let tender_id = tou_db::seed::seed_summed_up_tender(&db, &title)
        .await
        .context("подготовка тендера с итогами торгов")?;

    writeln!(std::io::stdout(), "{tender_id}")?;
    Ok(())
}

/// `api demo-special-site [наименование объекта]` - площадка сценариев
/// контура 3 (T44): свой объект на прогон и помеченная инвестиционная
/// категория (Q-013). Печатает id объекта в stdout.
async fn demo_special_site() -> anyhow::Result<()> {
    use std::io::Write as _;

    let title = std::env::args().nth(2).unwrap_or_else(|| {
        format!(
            "E2E контур 3 - площадка ({})",
            uuid::Uuid::now_v7().simple()
        )
    });

    let db = tou_db::connect(&env("DATABASE_URL")?)
        .await
        .context("подключение PostgreSQL")?;
    let object_id = tou_db::seed::seed_special_site(&db, &title)
        .await
        .context("подготовка площадки особого порядка")?;

    writeln!(std::io::stdout(), "{object_id}")?;
    Ok(())
}

/// `api demo-single-application [заголовок]` - тендер с единственной
/// заявкой и истекшим приемом (T32): площадка сценария «несостоявшийся →
/// повтор» (основание п. 81.2). Печатает id в stdout.
async fn demo_single_application() -> anyhow::Result<()> {
    use std::io::Write as _;

    let title = std::env::args().nth(2).unwrap_or_else(|| {
        format!(
            "E2E контур 2 - одна заявка ({})",
            uuid::Uuid::now_v7().simple()
        )
    });

    let db = tou_db::connect(&env("DATABASE_URL")?)
        .await
        .context("подключение PostgreSQL")?;
    let tender_id = tou_db::seed::seed_single_application_tender(&db, &title)
        .await
        .context("подготовка тендера с одной заявкой")?;

    writeln!(std::io::stdout(), "{tender_id}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_duration;

    #[test]
    fn demo_timer_accepts_minutes_hours_and_bare_numbers() {
        assert_eq!(parse_duration("5m").unwrap(), 5);
        assert_eq!(parse_duration("2h").unwrap(), 120);
        assert_eq!(parse_duration("45").unwrap(), 45);
    }

    #[test]
    fn demo_timer_rejects_garbage_and_non_positive() {
        assert!(parse_duration("soon").is_err());
        assert!(parse_duration("0m").is_err());
        assert!(parse_duration("-5m").is_err());
    }
}
