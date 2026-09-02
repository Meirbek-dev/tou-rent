//! Выборки с потолком строк выполняются против живой БД (T73, NFR-02).
//!
//! Потолок добавлен к тем запросам, размер которых не ограничен предметной
//! областью: реестрам портала, рабочим спискам, журналам, отчетам. Правка
//! у них однотипная - `LIMIT $n` в хвосте и лишняя привязка, - и ошибиться
//! в ней можно ровно одним способом: перепутать номер плейсхолдера. Такая
//! опечатка не видна ни компилятору, ни линту, а проявляется отказом БД
//! в рантайме на том экране, который тестом не покрыт.
//!
//! Поэтому большая часть тестов здесь не проверяет поведение - только то,
//! что каждый запрос разбирается и выполняется. Данные для этого не нужны:
//! пустая выборка - такой же успешный разбор, как и полная.
//!
//! Отдельно проверяется то, ради чего потолок перестал быть молчаливым
//! (W-17): признак усечения в ответе и курсор. Здесь без данных уже нельзя -
//! пустая выборка не усекается, и утверждать по ней нечего.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use tou_db::reports::{self, RegistryRow};
use tou_db::{
    applications, contracts, evasion, investment, land, ledger, obligations, public_records,
    publications, special,
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

    evasion::registry(&db, None, tou_db::MAX_ROWS)
        .await
        .expect("реестр уклонившихся");
    investment::list(&db).await.expect("инвест-договоры");
    land::list_all(&db).await.expect("земельные участки");
    land::list_published(&db)
        .await
        .expect("опубликованные участки");
    land::list_all_applications(&db)
        .await
        .expect("заявки на участки");
    public_records::list_public(&db, None, tou_db::MAX_ROWS)
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
    ledger::entries(&db, nobody, None, tou_db::MAX_ROWS)
        .await
        .expect("журнал счета");
}

/// Пустая выборка не усечена: пробная строка не должна поднимать признак
/// на ровном месте - ложный «показано не все» так же вреден, как молчание.
#[tokio::test]
async fn an_empty_selection_is_not_truncated() {
    let db = require_db!();
    let nobody = Uuid::now_v7();

    assert!(
        !contracts::list_for_tenant(&db, nobody)
            .await
            .expect("договоры нанимателя")
            .truncated
    );
    assert!(
        !publications::list_for_participant(&db, nobody)
            .await
            .expect("протоколы участника")
            .truncated
    );
    assert!(
        !ledger::entries(&db, nobody, None, tou_db::MAX_ROWS)
            .await
            .expect("журнал счета")
            .truncated
    );
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

/// Поступления одного счета: `count` приходных проводок с одинаковым
/// `occurred_at`.
///
/// Одинаковым намеренно: курсор по голому времени такую пачку либо
/// потеряет, либо покажет дважды, и проверять его надо ровно на ней.
/// Взносы одной транзакции так и ложатся - секунда у них общая.
async fn receipts_of_one_account(
    tx: &mut sqlx::PgConnection,
    count: i32,
) -> Result<(), sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let payer = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'W17 плательщик', now()) RETURNING id",
        format!("w17-payer-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let object = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'W17 помещение', 'г. Павлодар, ул. Тестовая, 17', 30)
         RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await?;

    let contract = sqlx::query_scalar!(
        "INSERT INTO core.contracts
           (object_id, tenant_id, monthly_rate, status, lease_period,
            drafted_at, tenant_signed_at, documents_received_at)
         VALUES ($1, $2, 1000, 'active',
                 tstzrange(now(), now() + interval '365 days'),
                 core.now(), core.now(), core.now())
         RETURNING id",
        object,
        payer
    )
    .fetch_one(&mut *tx)
    .await?;

    // Регистрация - через сверку и подписи (INV-115, FR-905), а не одной вставкой
    sqlx::query!(
        "INSERT INTO core.contract_checklists (contract_id, item_code, checked_at)
         VALUES ($1, 'bank_details', core.now())",
        contract
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "UPDATE core.contracts
         SET landlord_signed_at = core.now(), registered_at = core.now(), reg_number = $2
         WHERE id = $1",
        contract,
        format!("Д-W17-{tag}")
    )
    .execute(&mut *tx)
    .await?;

    let account = sqlx::query_scalar!(
        "INSERT INTO core.ledger_accounts (kind, contract_id, owner_user_id)
         VALUES ('contract_deposit', $1, $2) RETURNING id",
        contract,
        payer
    )
    .fetch_one(&mut *tx)
    .await?;

    for _ in 0..count {
        sqlx::query!(
            "INSERT INTO core.ledger_entries
               (account_id, op, credit, rule_ref, recorded_by, paid_at, occurred_at)
             VALUES ($1, 'receipt_confirmed', 100, 'W17', $2, current_date, core.now())",
            account,
            payer
        )
        .execute(&mut *tx)
        .await?;
    }

    Ok(())
}

/// Реестр, который не поместился, так и говорит - и дает, чем продолжить.
///
/// Проверяется то, ради чего потолок перестал быть молчаливым: страница
/// меньше выборки поднимает `truncated`, курсор ведет к следующим строкам,
/// а не к тем же самым, и последняя страница признак опускает. Без этого
/// ответ выглядел бы полным, будучи обрезанным, - а заметила бы разницу
/// бухгалтерия, у которой в реестре не сошлись бы записи.
#[tokio::test]
async fn a_truncated_registry_says_so_and_offers_a_cursor() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    receipts_of_one_account(&mut tx, 3)
        .await
        .expect("поступления");

    let period = reports::Period::default();

    let first = reports::receipts_page(&mut tx, period, None, 2)
        .await
        .expect("первая страница");
    assert_eq!(first.len(), 2, "страница отдает ровно запрошенное");
    assert!(first.truncated, "за страницей есть строки - и это видно");

    let cursor = first.last().map(RegistryRow::cursor).expect("курсор");
    let second = reports::receipts_page(&mut tx, period, Some(cursor), 2)
        .await
        .expect("вторая страница");

    let first_ids: Vec<_> = first.iter().map(|row| row.id).collect();
    assert!(
        second.iter().all(|row| !first_ids.contains(&row.id)),
        "курсор ведет за страницу, а не повторяет ее"
    );
    assert!(
        !second.is_empty(),
        "третье поступление достается со второй страницы"
    );

    // Страница шире выборки признак не поднимает: обходить дальше нечего.
    // Период берется однодневным из будущего - что накопилось на стенде,
    // на утверждение влиять не должно
    let today = sqlx::query_scalar!(r#"SELECT core.now()::date AS "today!""#)
        .fetch_one(&mut *tx)
        .await
        .expect("дата сервера");
    let empty = reports::Period {
        from: Some(today + time::Duration::days(3650)),
        to: None,
    };
    let tail = reports::receipts_page(&mut tx, empty, None, 2)
        .await
        .expect("пустой период");
    assert!(tail.is_empty() && !tail.truncated, "показано все, что есть");

    tx.rollback().await.expect("откат");
}

/// Курсор реестра решений идет по обеим ветвям `UNION ALL` сразу.
///
/// Условие курсора в этом запросе написано дважды - по разу на ветвь, -
/// и ошибиться в нем можно молча: реестр продолжал бы листаться, отдавая
/// решения только одного вида. Проверка идет на пустом периоде из будущего:
/// он гарантированно ничего не содержит, поэтому утверждение про пустой
/// хвост не зависит от того, что накопилось на стенде.
#[tokio::test]
async fn the_decisions_cursor_walks_both_union_branches() {
    let db = require_db!();
    let mut conn = db.acquire().await.expect("соединение");

    let today = sqlx::query_scalar!(r#"SELECT core.now()::date AS "today!""#)
        .fetch_one(&mut *conn)
        .await
        .expect("дата сервера");

    let future = reports::Period {
        from: Some(today + time::Duration::days(3650)),
        to: None,
    };

    let page = reports::decisions_page(&mut conn, future, None, 10)
        .await
        .expect("реестр решений");
    assert!(page.is_empty(), "в будущем решений нет");
    assert!(!page.truncated, "пустая выборка не усечена");

    // Курсор из прошлого не ломает разбор ни одной из ветвей
    let cursor = tou_db::RowCursor::new(
        time::OffsetDateTime::UNIX_EPOCH,
        Uuid::from_u128(0xffff_ffff_ffff_ffff_ffff_ffff_ffff_ffff),
    );
    let page = reports::decisions_page(&mut conn, reports::Period::default(), Some(cursor), 10)
        .await
        .expect("реестр решений с курсора");
    assert!(
        page.is_empty(),
        "курсор на начале времен не пускает вперед - выборка идет вниз"
    );
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
