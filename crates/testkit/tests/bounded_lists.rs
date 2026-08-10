//! Выборки с потолком строк выполняются против живой БД (T73, NFR-02).
//!
//! Потолок добавлен к тем запросам, размер которых не ограничен предметной
//! областью: реестрам портала, рабочим спискам, журналам, отчетам. Правка
//! у них однотипная - `LIMIT $n` в хвосте и лишняя привязка, - и ошибиться
//! в ней можно ровно одним способом: перепутать номер плейсхолдера. Такая
//! опечатка не видна ни компилятору, ни линту, а проявляется отказом БД
//! в рантайме на том экране, который тестом не покрыт.
//!
//! Поэтому здесь не проверяется поведение - только то, что каждый запрос
//! разбирается и выполняется. Данные не нужны: пустая выборка - такой же
//! успешный разбор, как и полная.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use tou_db::{
    applications, contracts, evasion, investment, land, ledger, obligations, public_records,
    publications, reports, special,
};
use tou_domain::role::Role;
use uuid::Uuid;

async fn try_pool() -> Result<Option<tou_db::Db>, sqlx::Error> {
    // Пропуск без адреса допустим локально, но не в пайплайне (G2/G15):
    // молча пройденный интеграционный тест ничего не проверяет
    match tou_testkit::database_url().map_err(|e| sqlx::Error::Configuration(Box::new(e)))? {
        Some(url) => tou_db::connect(&url).await.map(Some),
        None => Ok(None),
    }
}

macro_rules! require_db {
    () => {
        match try_pool()
            .await
            .expect("TESTKIT_DATABASE_URL: подключение не удалось")
        {
            Some(db) => db,
            None => {
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - потолки не проверялись");
                return;
            }
        }
    };
}

/// Потолок объявлен один на слой и заведомо больше любого экрана.
///
/// Обе величины - константы, поэтому проверку делает компилятор: сведение
/// потолка к десятку строк должно ломать сборку, а не падать на прогоне.
#[test]
fn cap_is_declared_once_and_sane() {
    const {
        assert!(tou_db::MAX_ROWS >= 100, "потолок меньше разумной страницы")
    };
    const {
        assert!(
            tou_db::BATCH_ROWS <= tou_db::MAX_ROWS,
            "пачка воркера не должна превышать потолок выборки"
        )
    };
}

/// Реестры и списки без родителя: потолок - единственный параметр.
#[tokio::test]
async fn global_registries_run_with_cap() {
    let db = require_db!();

    evasion::registry(&db).await.expect("реестр уклонившихся");
    investment::list(&db).await.expect("инвест-договоры");
    land::list_all(&db).await.expect("земельные участки");
    land::list_published(&db)
        .await
        .expect("опубликованные участки");
    land::list_all_applications(&db)
        .await
        .expect("заявки на участки");
    public_records::list_public(&db)
        .await
        .expect("публикации портала");
    public_records::pending(&db).await.expect("ждет публикации");
}

/// Списки по владельцу: потолок идет вторым параметром - здесь и живет
/// опечатка в номере плейсхолдера.
#[tokio::test]
async fn per_owner_lists_run_with_cap() {
    let db = require_db!();
    let nobody = Uuid::now_v7();

    applications::list_own(&db, nobody)
        .await
        .expect("заявки участника");
    contracts::list_for_tenant(&db, nobody)
        .await
        .expect("договоры нанимателя");
    publications::list_for_participant(&db, nobody)
        .await
        .expect("протоколы участника");
    special::list_own(&db, nobody)
        .await
        .expect("заявки особого порядка");
    land::list_own(&db, nobody)
        .await
        .expect("заявки инвестора на участки");
    ledger::entries(&db, nobody).await.expect("журнал счета");
}

/// Рабочие списки и лента торгов.
#[tokio::test]
async fn worklists_run_with_cap() {
    let db = require_db!();

    obligations::for_roles(&db, &[Role::Organizer, Role::Secretary])
        .await
        .expect("мои сроки");
    special::list_by_status(&db, &["submitted", "under_review"])
        .await
        .expect("заявки в работе");
    tou_db::auctions::bids_of(&db, Uuid::now_v7(), None)
        .await
        .expect("лента ставок");
    tou_db::auctions::bids_of(&db, Uuid::now_v7(), Some(0))
        .await
        .expect("лента ставок с курсора");
}

/// Реестры отчетности: потолок идет третьим параметром, после периода.
/// Проверяются обе ветки - с периодом и без него (без периода это вся
/// история, ради чего потолок и появился).
#[tokio::test]
async fn report_registries_run_with_cap() {
    let db = require_db!();
    let mut conn = db.acquire().await.expect("соединение");

    let today = sqlx::query_scalar!(r#"SELECT core.now()::date AS "today!""#)
        .fetch_one(&mut *conn)
        .await
        .expect("дата сервера");

    for period in [
        reports::Period {
            from: None,
            to: None,
        },
        reports::Period {
            from: Some(today - time::Duration::days(30)),
            to: Some(today),
        },
    ] {
        reports::decisions(&mut conn, period)
            .await
            .expect("реестр решений");
        reports::contracts(&mut conn, period)
            .await
            .expect("реестр договоров");
        reports::receipts(&mut conn, period)
            .await
            .expect("реестр поступлений");
    }
}

/// Проходы фонового воркера идут пачками: `LIMIT` внутри `UPDATE`
/// поставить нельзя, поэтому строки отбираются подзапросом
/// (`WHERE id IN (SELECT … LIMIT $1 FOR UPDATE SKIP LOCKED)`) - форма
/// достаточно нетривиальная, чтобы проверять ее исполнением.
///
/// Проход не откатывается и правда что-то меняет: снимает публикации
/// с истекшим сроком и переводит просроченные обязательства в `overdue`.
/// Это не порча стенда - ровно то же самое делает сам воркер `jobs` раз
/// в минуту, а повторный проход уже ничего не находит (`escalated_at`,
/// `unpublished_at`). Поведение эскалации проверяет `obligations_engine`;
/// здесь - только исполнимость запроса.
#[tokio::test]
async fn worker_batches_run() {
    let db = require_db!();

    obligations::take_overdue(&db).await.expect("просроченные");
    publications::take_expired(&db)
        .await
        .expect("снятие протоколов");
    public_records::take_expired(&db)
        .await
        .expect("снятие публикаций особого порядка");
}
