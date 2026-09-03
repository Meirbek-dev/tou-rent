//! Рубеж «родитель существует» против живой БД (G-034, G-035).
//!
//! Подресурс несуществующего тендера отвечал пустым перечнем, а мутация
//! несуществующей записи - отказом по правилу: и то и другое неотличимо от
//! рабочего ответа. Отличать «нет записи» от «нечего показывать» стало
//! делом одного запроса на родителя, и проверять его надо на живой БД:
//! ошибиться в нем можно ровно одним способом - спросить не ту таблицу,
//! а такая опечатка компилируется и линтуется без единого замечания.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use tou_db::{ledger, tenders};
use uuid::Uuid;

async fn try_pool() -> Result<Option<tou_db::Db>, sqlx::Error> {
    match tou_testkit::database_ready()
        .await
        .map_err(|e| sqlx::Error::Configuration(Box::new(e)))?
    {
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - рубеж родителя не проверялся");
                return;
            }
        }
    };
}

/// Тендер с одним лотом и депозитный счет договора: три родителя, чье
/// отсутствие обработчики обязаны отличать от пустого содержимого.
struct Parents {
    tender_id: Uuid,
    lot_id: Uuid,
    account_id: Uuid,
}

async fn parents(tx: &mut sqlx::PgConnection) -> Result<Parents, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let organizer = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'G34 организатор', now()) RETURNING id",
        format!("g34-organizer-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id)
         VALUES ('G34 тендер', 'draft', $1) RETURNING id",
        organizer
    )
    .fetch_one(&mut *tx)
    .await?;

    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'G34 помещение', 'г. Павлодар, ул. Тестовая, 34', 25)
         RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await?;

    let lot_id = sqlx::query_scalar!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                base_rate_monthly, guarantee_fee, rate_calculation)
         VALUES ($1, 1, $2, 'офис', 12, 1000, 1000, '{}'::jsonb) RETURNING id",
        tender_id,
        object_id
    )
    .fetch_one(&mut *tx)
    .await?;

    let contract_id = sqlx::query_scalar!(
        "INSERT INTO core.contracts (object_id, tenant_id, monthly_rate, status, drafted_at)
         VALUES ($1, $2, 1000, 'draft', core.now()) RETURNING id",
        object_id,
        organizer
    )
    .fetch_one(&mut *tx)
    .await?;

    let account_id = sqlx::query_scalar!(
        "INSERT INTO core.ledger_accounts (kind, contract_id, owner_user_id)
         VALUES ('contract_deposit', $1, $2) RETURNING id",
        contract_id,
        organizer
    )
    .fetch_one(&mut *tx)
    .await?;

    Ok(Parents {
        tender_id,
        lot_id,
        account_id,
    })
}

/// Родитель либо есть, либо нет - и рубеж говорит именно это.
///
/// Случайный uuid обязан читаться как «нет»: на нем и держится 404
/// подресурса. Заведенная запись - как «есть»: рубеж, который всегда
/// отвечает «нет», закрыл бы работающие экраны, и такую подмену
/// (запрос не к той таблице) поймает только живая БД.
#[tokio::test]
async fn parent_guards_tell_a_missing_record_from_an_existing_one() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let parents = parents(&mut tx).await.expect("родители");

    let nobody = Uuid::now_v7();
    assert!(
        !tenders::exists(&mut tx, nobody).await.expect("тендер"),
        "случайный id тендера не должен находиться"
    );
    assert!(
        !tenders::lot_exists(&mut tx, nobody).await.expect("лот"),
        "случайный id лота не должен находиться"
    );
    assert!(
        !ledger::account_exists(&mut tx, nobody)
            .await
            .expect("счет книги"),
        "случайный id счета не должен находиться"
    );

    assert!(
        tenders::exists(&mut tx, parents.tender_id)
            .await
            .expect("тендер"),
        "заведенный тендер обязан находиться"
    );
    assert!(
        tenders::lot_exists(&mut tx, parents.lot_id)
            .await
            .expect("лот"),
        "заведенный лот обязан находиться"
    );
    assert!(
        ledger::account_exists(&mut tx, parents.account_id)
            .await
            .expect("счет книги"),
        "открытый счет обязан находиться"
    );

    tx.rollback().await.expect("rollback");
}
