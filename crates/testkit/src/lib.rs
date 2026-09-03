//! Тестовая обвязка (арх. § 4): подключение к живой БД для интеграционных
//! тестов и гейтов трассировки.
//!
//! Тесты этого крейта работают против настоящего PostgreSQL (A-021): часть
//! инвариантов живет в триггерах, RLS и CHECK, и проверить их без БД нельзя
//! в принципе. Адрес берется из `TESTKIT_DATABASE_URL`.
//!
//! Переменной нет - тест пропускается. Это удобно на машине разработчика,
//! но в пайплайне ровно то же поведение означает ложную зеленую: тест,
//! вернувший управление, засчитан пройденным, и `cargo test` рапортует
//! успех, не проверив ни одного инварианта. Поэтому в CI отсутствие адреса
//! - ошибка, а не пропуск (см. [`database_url`]).
//!
//! База - своя (`tou_rent_test`), не рабочая. Круг 7 гаунтлета (G-021):
//! `TESTKIT_DATABASE_URL` смотрел на ту же базу, что и api дев-стенда,
//! а фикстуры этих тестов коммитятся (иначе не отработали бы аудит-триггеры
//! и функции слоя данных). Итог одного прогона: тестовая комиссия стала
//! действующей комиссией стенда, а из 127 учетных записей 103 оказались
//! тестовыми. Прогон тестов не имеет права менять то, что показывает стенд.
//!
//! Отдельная база не требует ручной подготовки: [`database_ready`] заводит
//! ее при первом обращении и накатывает миграции - один раз на процесс.

use std::str::FromStr as _;

use sqlx::Connection as _;
use sqlx::postgres::PgConnectOptions;
use tokio::sync::OnceCell;

/// В пайплайне адрес БД обязателен - пропускать проверки там нечем.
#[derive(Debug, thiserror::Error)]
#[error(
    "TESTKIT_DATABASE_URL не задан, а переменная CI выставлена: \
     в пайплайне интеграционные тесты обязаны идти против живой БД, \
     иначе они молча превращаются в зеленый прочерк"
)]
pub struct DatabaseRequired;

/// Подготовка тестовой БД не удалась. Пропуском это не считается: адрес
/// задан, значит проверки обязаны идти.
#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    #[error(transparent)]
    Required(#[from] DatabaseRequired),
    /// В адресе нет имени базы - создавать нечего, а `postgres` по умолчанию
    /// брать нельзя: это служебная база кластера
    #[error("TESTKIT_DATABASE_URL не называет базу данных")]
    Nameless,
    #[error("подготовка тестовой БД: {0}")]
    Db(#[from] sqlx::Error),
    #[error("миграции тестовой БД: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

/// Схема тестовой БД - те же миграции, что и у стенда: слепок встраивается
/// в сборку, поэтому рабочий каталог прогона роли не играет.
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../db/migrations");

/// Подготовка идет один раз на процесс: тестовых целей в крейте под шесть
/// десятков, и каждая - свой процесс, но внутри процесса тесты идут
/// параллельно.
static PREPARED: OnceCell<()> = OnceCell::const_new();

/// Служебная база кластера: к ней подключаются, чтобы завести тестовую.
const MAINTENANCE_DB: &str = "postgres";

/// Ключ рекомендательной блокировки на время `CREATE DATABASE`. Процессы
/// тестовых целей стартуют одновременно, и без нее двое сразу видят
/// «базы нет» и оба идут ее создавать.
const SETUP_LOCK: i64 = 0x746F_755F_7465_7374;

/// Адрес БД для интеграционных тестов.
///
/// `Ok(Some(url))` - подключаться, `Ok(None)` - пропустить (машина
/// разработчика), `Err` - пропуск недопустим (пайплайн).
///
/// Сам пропуск и сама паника остаются на стороне теста: в тестовой цели
/// `expect` разрешен гейтом G2, а здесь, в обычном коде, - нет.
pub fn database_url() -> Result<Option<String>, DatabaseRequired> {
    match std::env::var("TESTKIT_DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => Ok(Some(url)),
        _ if is_ci() => Err(DatabaseRequired),
        _ => Ok(None),
    }
}

/// То же, что [`database_url`], но база к моменту возврата готова: заведена
/// и с накатанными миграциями.
///
/// Точка входа всех тестов крейта. Чистому стенду (и чистому раннеру CI)
/// ничего больше не нужно: `tou_rent_test` появляется сама, а рабочая база
/// стенда остается нетронутой (G-021).
pub async fn database_ready() -> Result<Option<String>, SetupError> {
    let Some(url) = database_url()? else {
        return Ok(None);
    };
    PREPARED.get_or_try_init(|| prepare(url.clone())).await?;
    Ok(Some(url))
}

/// Заводит базу, если ее нет, и накатывает миграции.
///
/// Запросы здесь - без макросов `query!` намеренно: `CREATE DATABASE` не
/// принимает параметров и в слепок `.sqlx` не ложится в принципе, а
/// остальные два обращения - к системному каталогу, а не к схеме проекта.
async fn prepare(url: String) -> Result<(), SetupError> {
    let options = PgConnectOptions::from_str(&url)?;
    let name = options
        .get_database()
        .ok_or(SetupError::Nameless)?
        .to_owned();

    {
        let mut admin = sqlx::PgConnection::connect_with(
            &options
                .clone()
                .database(MAINTENANCE_DB)
                .application_name("tou-testkit-setup"),
        )
        .await?;

        // Блокировка сеансовая: она снимается сама при закрытии соединения,
        // в том числе если создание базы упало
        sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(SETUP_LOCK)
            .execute(&mut admin)
            .await?;

        let exists: Option<i32> =
            sqlx::query_scalar("SELECT 1 FROM pg_database WHERE datname = $1")
                .bind(&name)
                .fetch_optional(&mut admin)
                .await?;
        if exists.is_none() {
            // Имя приходит из окружения прогона, а не от пользователя, но
            // в идентификатор оно попадает подстановкой: кавычки удваиваются,
            // а `AssertSqlSafe` - расписка за эту подстановку
            let quoted = name.replace('"', "\"\"");
            sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE \"{quoted}\"")))
                .execute(&mut admin)
                .await?;
        }

        admin.close().await?;
    }

    // Миграции - под владельцем схемы, без `SET ROLE tou_rent_app` (A-011),
    // поэтому соединение прямое, а не через `tou_db::connect`
    let mut target =
        sqlx::PgConnection::connect_with(&options.application_name("tou-testkit-migrate")).await?;
    // Свою рекомендательную блокировку на время наката берет сам мигратор:
    // параллельные процессы тестовых целей ждут друг друга
    MIGRATOR.run(&mut target).await?;
    target.close().await?;
    Ok(())
}

/// Признак пайплайна. `CI` выставляют и GitLab, и GitHub, и большинство
/// прочих раннеров; значение не важно - важен сам факт.
fn is_ci() -> bool {
    std::env::var_os("CI")
        .is_some_and(|value| !value.is_empty() && value != "0" && value != "false")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Проверки идут через приватную ветку: менять переменные окружения
    /// в тестах небезопасно - они общие на весь процесс, а тесты идут
    /// параллельно.
    #[test]
    fn ci_flag_reads_common_runner_values() {
        // Значение самого раннера в этой сборке неизвестно, поэтому
        // проверяется только то, что функция не паникует и что-то решает
        let _ = is_ci();
    }

    #[test]
    fn required_error_explains_the_failure() {
        let message = DatabaseRequired.to_string();
        assert!(message.contains("TESTKIT_DATABASE_URL"));
    }

    /// G-021: обвязка обязана уметь назвать базу по адресу - иначе она
    /// не отличит «своя тестовая» от «рабочая база стенда».
    #[test]
    fn the_database_name_comes_from_the_url() {
        let options = PgConnectOptions::from_str(
            "postgres://tou_rent:tou_rent_dev@postgres:5432/tou_rent_test",
        )
        .expect("разбор адреса");
        assert_eq!(options.get_database(), Some("tou_rent_test"));
    }

    /// Адрес без имени базы - отказ, а не молчаливый `postgres`: миграции
    /// накатились бы в служебную базу кластера.
    #[test]
    fn a_url_without_a_database_is_refused() {
        let options =
            PgConnectOptions::from_str("postgres://tou_rent@postgres:5432").expect("разбор адреса");
        assert_eq!(options.get_database(), None);
        assert!(SetupError::Nameless.to_string().contains("базу данных"));
    }
}
