//! Очистка данных стенда администратором (М15, FR-1503, FR-1601) против
//! живой БД.
//!
//! Проверяется то, что нельзя закрепить типом: что `core.purge_data()`
//! проходит сквозь append-only сторожей и возвращает их на место, что
//! чужой тендер и объект остаются, что след ложится в ту же hash-цепочку
//! (INV-A01), что после очистки сторожа снова отбивают прямое удаление,
//! что объект уносит тендеры, где он выставлен лотом, а точечное удаление
//! лота или материала досье не трогает их тендер. Отдельно - полнота
//! перечня: каждая таблица схемы `core` либо в порядке удаления функции,
//! либо в явном перечне оставляемых, третьего нет; и у каждого вида
//! данных кабинета есть перечень записей.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use std::collections::BTreeSet;

use tou_db::purge::PurgeScope;
use uuid::Uuid;

async fn try_pool() -> Result<Option<tou_db::Db>, sqlx::Error> {
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - очистка данных не проверялась");
                return;
            }
        }
    };
}

/// Таблицы схемы `core`, которые очистка оставляет намеренно: учетные
/// записи с ролями и внешними идентичностями, одноразовые коды входа,
/// состав комиссии и объявление на главной.
const KEPT: [&str; 7] = [
    "users",
    "role_grants",
    "user_identities",
    "account_verifications",
    "commissions",
    "commission_members",
    "site_announcements",
];

/// Порядок удаления из текста функции: строки шагов вида `['table', '...']`.
fn purged_tables() -> BTreeSet<&'static str> {
    include_str!("../../db/migrations/20260902120000_admin_data_purge_kinds.sql")
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("['")?;
            let end = rest.find('\'')?;
            Some(&rest[..end])
        })
        .collect()
}

/// Каждая таблица `core` либо стирается, либо названа оставляемой, а
/// каждый вид данных кабинета - одна из стираемых таблиц.
///
/// Новая таблица следующей миграции сюда не попадет сама: тест заставит
/// решить, уходит она при очистке или остается, - молча остаться с данными
/// после «пустого» стенда она не должна.
#[tokio::test]
async fn purge_covers_every_core_table() {
    let db = require_db!();

    let purged = purged_tables();
    assert!(purged.len() > 40, "перечень шагов не прочитан: {purged:?}");

    let tables: Vec<String> = sqlx::query_scalar!(
        r#"SELECT table_name AS "table_name!"
           FROM information_schema.tables
           WHERE table_schema = 'core' AND table_type = 'BASE TABLE'
           ORDER BY 1"#
    )
    .fetch_all(&db)
    .await
    .expect("таблицы схемы core");

    let undecided: Vec<&String> = tables
        .iter()
        .filter(|t| !purged.contains(t.as_str()) && !KEPT.contains(&t.as_str()))
        .collect();
    assert!(
        undecided.is_empty(),
        "таблицы core вне очистки и вне перечня оставляемых: {undecided:?}. \
         Добавьте шаг в core.purge_data (миграция очистки) либо в KEPT этого теста"
    );

    let missing: Vec<&&str> = purged
        .iter()
        .filter(|t| !tables.iter().any(|x| x == *t))
        .collect();
    assert!(
        missing.is_empty(),
        "шаги очистки ссылаются на несуществующие таблицы: {missing:?}"
    );

    for kind in PurgeScope::KINDS {
        assert!(
            purged.contains(kind.as_str()),
            "вид данных {} не входит в порядок удаления",
            kind.as_str()
        );
    }
}

/// У каждого вида данных есть перечень записей - запрос отрабатывает на
/// живой схеме, а не только компилируется по слепку.
#[tokio::test]
async fn every_kind_lists_its_records() {
    let db = require_db!();
    for kind in PurgeScope::KINDS {
        tou_db::purge::list_records(&db, kind)
            .await
            .unwrap_or_else(|e| panic!("перечень {}: {e}", kind.as_str()));
    }
}

/// Хеш пароля здесь не настоящий: проверяется очистка, а не вход.
const HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c3RhcnlqLXNhbHQtMTIz$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

struct Fixture {
    actor: Uuid,
    object: Uuid,
    tender: Uuid,
    lot: Uuid,
    dossier_item: Uuid,
}

/// Организатор, объект и тендер с лотом на этом объекте и материалом досье -
/// самой строгой из append-only таблиц (INV-042): ее и стирает очистка.
async fn fixture(db: &tou_db::Db, tag: &str) -> Result<Fixture, sqlx::Error> {
    let nonce = Uuid::now_v7().simple();
    let actor = sqlx::query_scalar!(
        "INSERT INTO core.users (email, full_name, password_hash, is_active)
         VALUES ($1::citext, 'Тест очистки', $2, true) RETURNING id",
        format!("purge-{tag}-{nonce}@tou.test"),
        HASH
    )
    .fetch_one(db)
    .await?;

    let object = sqlx::query_scalar!(
        "INSERT INTO core.objects
           (kind, name, address, area_m2,
            premises_type_code, premises_kind_code, comfort_code, location_code)
         VALUES ('premises', $1, 'г. Павлодар, ул. Тестовая, 1', 10.00,
                 'default', 'default', 'default', 'default')
         RETURNING id",
        format!("Объект очистки {tag} {nonce}")
    )
    .fetch_one(db)
    .await?;

    let tender = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, organizer_id) VALUES ($1, $2) RETURNING id",
        format!("Тендер очистки {tag} {nonce}"),
        actor
    )
    .fetch_one(db)
    .await?;

    let lot = sqlx::query_scalar!(
        "INSERT INTO core.lots
           (tender_id, seq, object_id, purpose, purpose_kk, lease_months,
            base_rate_monthly, guarantee_fee, rate_calculation)
         VALUES ($1, 1, $2, 'проба очистки', 'тазалау сынағы', 12,
                 1000.00, 1000.00, '{}'::jsonb)
         RETURNING id",
        tender,
        object
    )
    .fetch_one(db)
    .await?;

    let dossier_item = sqlx::query_scalar!(
        "INSERT INTO core.dossier_items (tender_id, kind, source_table, source_id, title)
         VALUES ($1, 'test', 'core.tenders', $1, 'Проба очистки') RETURNING id",
        tender
    )
    .fetch_one(db)
    .await?;

    Ok(Fixture {
        actor,
        object,
        tender,
        lot,
        dossier_item,
    })
}

async fn exists(db: &tou_db::Db, kind: PurgeScope, id: Uuid) -> Result<bool, sqlx::Error> {
    tou_db::purge::record_exists(db, kind, id).await
}

/// Уборка того, что очистка оставляет по замыслу: объект и пользователь.
async fn cleanup(db: &tou_db::Db, fixtures: &[&Fixture]) -> Result<(), sqlx::Error> {
    for f in fixtures {
        sqlx::query!("DELETE FROM core.objects WHERE id = $1", f.object)
            .execute(db)
            .await?;
        sqlx::query!("DELETE FROM core.users WHERE id = $1", f.actor)
            .execute(db)
            .await?;
    }
    Ok(())
}

/// Удаление тендера уносит его append-only хвост, не задевая соседей,
/// оставляет след в цепочке и возвращает сторожей.
#[tokio::test]
async fn purge_removes_a_tender_and_restores_the_guards() {
    let db = require_db!();
    let target = fixture(&db, "target").await.expect("тендер на удаление");
    let control = fixture(&db, "control").await.expect("контрольный тендер");

    let deleted = tou_db::purge::purge(
        &db,
        target.actor,
        PurgeScope::Tenders,
        Some(&[target.tender]),
    )
    .await
    .expect("очистка одного тендера");
    assert_eq!(
        deleted.get("tenders"),
        Some(&1),
        "удален ровно один тендер: {deleted:?}"
    );
    assert_eq!(
        deleted.get("lots"),
        Some(&1),
        "лот ушел с тендером: {deleted:?}"
    );
    assert_eq!(
        deleted.get("dossier_items"),
        Some(&1),
        "материал досье ушел вместе с тендером: {deleted:?}"
    );
    assert!(
        !deleted.contains_key("objects"),
        "объекты при удалении тендера остаются"
    );

    assert!(
        !exists(&db, PurgeScope::Tenders, target.tender)
            .await
            .expect("проверка тендера")
    );
    assert!(
        !exists(&db, PurgeScope::DossierItems, target.dossier_item)
            .await
            .expect("проверка материала досье")
    );
    assert!(
        exists(&db, PurgeScope::Objects, target.object)
            .await
            .expect("проверка объекта")
    );
    assert!(
        exists(&db, PurgeScope::Tenders, control.tender)
            .await
            .expect("проверка тендера"),
        "чужой тендер не тронут"
    );

    // След: сводное событие с актором в той же цепочке, и цепочка цела
    let event = sqlx::query!(
        r#"SELECT actor_id, payload -> 'old' ->> 'scope' AS "scope!",
                  payload -> 'old' -> 'tender_ids' AS "tender_ids!"
           FROM audit.log WHERE table_name = 'core.data_purge'
           ORDER BY id DESC LIMIT 1"#
    )
    .fetch_one(&db)
    .await
    .expect("событие очистки");
    assert_eq!(event.actor_id, Some(target.actor));
    assert_eq!(event.scope, "tenders");
    assert_eq!(
        event.tender_ids,
        serde_json::json!([target.tender]),
        "в событии - удаленные тендеры"
    );
    let intact = tou_db::audit::verify_chain(&db)
        .await
        .expect("сверка цепочки");
    assert!(intact.intact, "цепочка аудита порвана после очистки");

    // Сторожа вернулись в ALWAYS - и работают: прямое удаление отбивается
    let disabled: Vec<String> = sqlx::query_scalar!(
        r#"SELECT c.relname || '/' || t.tgname AS "name!"
           FROM pg_trigger t
           JOIN pg_class c ON c.oid = t.tgrelid
           JOIN pg_proc p ON p.oid = t.tgfoid
           WHERE NOT t.tgisinternal AND p.proname = 'forbid_mutation' AND t.tgenabled <> 'A'"#
    )
    .fetch_all(&db)
    .await
    .expect("режим сторожей");
    assert!(
        disabled.is_empty(),
        "сторожа не вернулись в ALWAYS: {disabled:?}"
    );

    let direct = sqlx::query!(
        "DELETE FROM core.dossier_items WHERE id = $1",
        control.dossier_item
    )
    .execute(&db)
    .await;
    assert!(
        direct.is_err(),
        "прямое удаление материала досье должно отбиваться"
    );
    assert!(
        exists(&db, PurgeScope::DossierItems, control.dossier_item)
            .await
            .expect("проверка материала досье")
    );

    // Уборка: контрольный тендер уходит той же очисткой, остальное - руками
    tou_db::purge::purge(
        &db,
        control.actor,
        PurgeScope::Tenders,
        Some(&[control.tender]),
    )
    .await
    .expect("уборка контрольного тендера");
    cleanup(&db, &[&target, &control]).await.expect("уборка");
}

/// Объект уносит тендеры, где он выставлен лотом, а соседний объект с его
/// тендером остается: область `objects` идет от объекта к процедурам.
#[tokio::test]
async fn purge_object_takes_its_tenders_along() {
    let db = require_db!();
    let target = fixture(&db, "obj-target")
        .await
        .expect("объект на удаление");
    let control = fixture(&db, "obj-control")
        .await
        .expect("контрольный объект");

    let deleted = tou_db::purge::purge(
        &db,
        target.actor,
        PurgeScope::Objects,
        Some(&[target.object]),
    )
    .await
    .expect("очистка одного объекта");
    assert_eq!(
        deleted.get("objects"),
        Some(&1),
        "удален ровно один объект: {deleted:?}"
    );
    assert_eq!(
        deleted.get("tenders"),
        Some(&1),
        "тендер по объекту ушел с ним: {deleted:?}"
    );
    assert_eq!(
        deleted.get("lots"),
        Some(&1),
        "лот ушел с тендером: {deleted:?}"
    );

    assert!(
        !exists(&db, PurgeScope::Objects, target.object)
            .await
            .expect("проверка объекта")
    );
    assert!(
        !exists(&db, PurgeScope::Tenders, target.tender)
            .await
            .expect("проверка тендера")
    );
    assert!(
        exists(&db, PurgeScope::Objects, control.object)
            .await
            .expect("проверка объекта")
    );
    assert!(
        exists(&db, PurgeScope::Tenders, control.tender)
            .await
            .expect("проверка тендера")
    );

    // Уборка: контрольный объект - той же областью, пользователи - руками
    tou_db::purge::purge(
        &db,
        control.actor,
        PurgeScope::Objects,
        Some(&[control.object]),
    )
    .await
    .expect("уборка контрольного объекта");
    for actor in [target.actor, control.actor] {
        sqlx::query!("DELETE FROM core.users WHERE id = $1", actor)
            .execute(&db)
            .await
            .expect("уборка пользователя");
    }
}

/// Точечное удаление идет вниз по графу, но не вверх: лот и материал досье
/// уходят по одному, а их тендер остается на месте.
#[tokio::test]
async fn point_deletion_keeps_the_parent() {
    let db = require_db!();
    let f = fixture(&db, "point").await.expect("тендер с лотом и досье");

    let deleted = tou_db::purge::purge(
        &db,
        f.actor,
        PurgeScope::DossierItems,
        Some(&[f.dossier_item]),
    )
    .await
    .expect("удаление материала досье");
    assert_eq!(deleted.len(), 1, "ушел только материал досье: {deleted:?}");
    assert_eq!(deleted.get("dossier_items"), Some(&1));

    let deleted = tou_db::purge::purge(&db, f.actor, PurgeScope::Lots, Some(&[f.lot]))
        .await
        .expect("удаление лота");
    assert_eq!(deleted.get("lots"), Some(&1), "{deleted:?}");
    assert!(
        !deleted.contains_key("tenders"),
        "тендер при удалении лота остается: {deleted:?}"
    );

    assert!(
        exists(&db, PurgeScope::Tenders, f.tender)
            .await
            .expect("проверка тендера")
    );
    assert!(
        !exists(&db, PurgeScope::Lots, f.lot)
            .await
            .expect("проверка лота")
    );
    assert!(
        !exists(&db, PurgeScope::DossierItems, f.dossier_item)
            .await
            .expect("проверка материала досье")
    );
    assert!(
        !exists(&db, PurgeScope::Everything, f.tender)
            .await
            .expect("у полной очистки перечня нет"),
        "everything не вид данных"
    );

    // Уборка: тендер - очисткой, остальное - руками
    tou_db::purge::purge(&db, f.actor, PurgeScope::Tenders, Some(&[f.tender]))
        .await
        .expect("уборка тендера");
    cleanup(&db, &[&f]).await.expect("уборка");
}

/// Без актора очистка отказывает целиком: событие без автора в аудите
/// недопустимо, а транзакция без `app.user_id` - это и есть вызов мимо
/// http-слоя. Неизвестная область отказывает так же вслух - до того, как
/// хоть один сторож будет снят.
#[tokio::test]
async fn purge_refuses_without_an_actor_or_with_an_unknown_scope() {
    let db = require_db!();

    let refused =
        sqlx::query_scalar!(r#"SELECT core.purge_data('tenders', ARRAY[]::uuid[]) AS "deleted!""#)
            .fetch_one(&db)
            .await;
    let message = match refused {
        Ok(_) => panic!("очистка без актора прошла"),
        Err(e) => e.to_string(),
    };
    assert!(
        message.contains("актора"),
        "отказ должен называть причину, а не падать на чем-то еще: {message}"
    );

    // Актор проверяется первым: вызов мимо http-слоя не должен узнавать
    // о перечне областей раньше, чем о запрете
    let bogus = sqlx::query_scalar!(
        r#"SELECT core.purge_data('everything-and-more', NULL::uuid[]) AS "deleted!""#
    )
    .fetch_one(&db)
    .await;
    let message = match bogus {
        Ok(_) => panic!("неизвестная область прошла"),
        Err(e) => e.to_string(),
    };
    assert!(message.contains("актора"), "{message}");

    let under_actor = sqlx::query_scalar!(
        r#"SELECT core.purge_data('everything-and-more', NULL::uuid[]) AS "deleted!""#
    );
    let rejected = tou_db::with_actor(&db, Uuid::now_v7(), async |tx| {
        under_actor.fetch_one(&mut *tx).await
    })
    .await;
    let message = match rejected {
        Ok(_) => panic!("неизвестная область прошла под актором"),
        Err(e) => e.to_string(),
    };
    assert!(message.contains("неизвестная область"), "{message}");
}
