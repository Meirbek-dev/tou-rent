//! Слой данных (арх. § 4, 6): пул PostgreSQL (dev - 19beta, ADR-0002),
//! sqlx-запросы, миграции.
//!
//! Схемы БД: `core` / `audit` / `refdata`. Ключевые инварианты (INV-DB-*, INV-021,
//! INV-037, INV-040, INV-063, INV-A01) закреплены на уровне СУБД - constraint'ы,
//! триггеры и RLS в `migrations/`.

pub use sqlx;

use sqlx::Executor as _;
use uuid::Uuid;

pub mod acts;
pub mod admission;
pub mod amendments;
pub mod applications;
pub mod auction_turns;
pub mod auctions;
pub mod audit;
pub mod benefit;
pub mod commission;
pub mod contract_amendments;
pub mod contracts;
pub mod evasion;
pub mod failure;
pub mod identities;
pub mod investment;
pub mod land;
pub mod ledger;
pub mod notifications;
pub mod objects;
pub mod obligations;
pub mod prices;
pub mod public_records;
pub mod publications;
pub mod refdata;
pub mod reports;
pub mod results;
pub mod seed;
pub mod special;
pub mod tenders;
pub mod users;

pub type Db = sqlx::PgPool;

/// Потолок строк для выборок, размер которых не ограничен предметной
/// областью (NFR-02).
///
/// Большая часть выборок этого слоя ограничена самими Правилами: категорий
/// особого порядка тринадцать, лотов у тендера единицы, оснований отклонения
/// закрытый перечень. Такие запросы потолка не требуют - у них его ставит
/// сама область.
///
/// Остальные растут со временем: реестр уклонившихся, журнал счета, реестры
/// портала, рабочие списки подразделения. У них нет ни естественной границы,
/// ни курсора в контракте, и однажды такой запрос вытянет таблицу целиком -
/// в память процесса, в JSON ответа и в память браузера.
///
/// Тысяча - заведомо больше любого разумного экрана и заведомо меньше того,
/// что способно уронить процесс. Достижение потолка не проходит молча:
/// [`warn_if_capped`] пишет предупреждение, и это сигнал, что выборке пора
/// курсор, а не что данные потерялись.
pub const MAX_ROWS: i64 = 1_000;

/// Размер пачки для проходов фонового воркера (FR-1702, INV-076).
///
/// Здесь потолок - не защита, а нормальный режим работы: воркер идет раз
/// в минуту, и недобранное подберет следующий тик. Без потолка первый
/// проход после длительного простоя (или после сдвига часов стенда)
/// поднял бы все накопившееся одной транзакцией - и держал бы ее открытой,
/// пока рассылает уведомления.
pub const BATCH_ROWS: i64 = 200;

/// Предупреждение, когда выборка уперлась в [`MAX_ROWS`].
///
/// Тихое усечение хуже отсутствия потолка: ответ выглядит полным, а он
/// обрезан. Поэтому в лог уходит и место, и размер.
pub(crate) fn warn_if_capped(rows: usize, query: &'static str) {
    if rows as i64 >= MAX_ROWS {
        tracing::warn!(
            query,
            rows,
            limit = MAX_ROWS,
            "выборка уперлась в потолок строк - показана только часть, нужна страничная выдача"
        );
    }
}

/// Транзакция «от имени пользователя»: GUC `app.user_id` читают audit-триггеры
/// (INV-A01, актор события) и RLS-политики. Каждая мутация домена обязана
/// проходить здесь (регламент А.5) - иначе аудит запишет «системную операцию».
/// Commit только при `Ok` замыкания; ошибка - откат.
pub async fn with_actor<T, E>(
    db: &Db,
    actor: Uuid,
    op: impl AsyncFnOnce(&mut sqlx::PgConnection) -> Result<T, E>,
) -> Result<T, E>
where
    E: From<sqlx::Error>,
{
    let mut tx = db.begin().await.map_err(E::from)?;
    set_actor(&mut tx, actor).await.map_err(E::from)?;
    let value = op(&mut tx).await?;
    tx.commit().await.map_err(E::from)?;
    Ok(value)
}

/// Актор события для уже открытой транзакции. Отдельно от [`with_actor`] -
/// чтобы сценарий можно было выполнить в транзакции вызывающего (например,
/// в тесте, который откатывает свои изменения).
pub async fn set_actor(conn: &mut sqlx::PgConnection, actor: Uuid) -> Result<(), sqlx::Error> {
    // `fetch_one`, а не `execute`: set_config возвращает столбец, и проверенный
    // макросом запрос с выходными столбцами - это Map, а не голый Query
    sqlx::query!(
        "SELECT set_config('app.user_id', $1, true)",
        actor.to_string()
    )
    .fetch_one(conn)
    .await
    .map(|_| ())
}

/// Подключение к PostgreSQL. URL приходит из конфигурации приложения
/// (env / SOPS, NFR-09) - в коде значения не хардкодятся.
///
/// Каждое соединение пула переводится в роль `tou_rent_app` без BYPASSRLS (A-011):
/// иначе запечатанность цен до вскрытия (RLS INV-040) не действует.
/// Superuser-подключение используется только для миграций.
pub async fn connect(database_url: &str) -> Result<Db, sqlx::Error> {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                conn.execute("SET ROLE tou_rent_app").await?;
                // Мастер-ключ шифрования цен (INV-040, п. 40): в базе он не
                // хранится, а приходит соединением из окружения (NFR-09).
                // Без него ценовые предложения не читаются и не пишутся.
                if let Ok(key) = std::env::var("PRICE_ENCRYPTION_KEY")
                    && !key.trim().is_empty()
                {
                    sqlx::query!("SELECT set_config('app.price_key', $1, false)", key)
                        .fetch_one(&mut *conn)
                        .await?;
                }
                Ok(())
            })
        })
        .connect(database_url)
        .await
}
