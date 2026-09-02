//! Очистка данных стенда администратором (М15, FR-1503, FR-1601) против
//! живой БД.
//!
//! Проверяется то, что нельзя закрепить типом: что `core.purge_data()`
//! проходит сквозь append-only сторожей и возвращает их на место, что
//! чужой тендер и объект остаются, что след ложится в ту же hash-цепочку
//! (INV-A01) и что после очистки сторожа снова отбивают прямое удаление.
//! Отдельно - полнота перечня: каждая таблица схемы `core` либо в порядке
//! удаления функции, либо в явном перечне оставляемых, третьего нет.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use std::collections::BTreeSet;

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
    include_str!("../../db/migrations/20260902100000_admin_data_purge.sql")
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("['")?;
            let end = rest.find('\'')?;
            Some(&rest[..end])
        })
        .collect()
}

/// Каждая таблица `core` либо стирается, либо названа оставляемой.
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
}

/// Хеш пароля здесь не настоящий: проверяется очистка, а не вход.
const HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c3RhcnlqLXNhbHQtMTIz$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

struct Fixture {
    actor: Uuid,
    object: Uuid,
    tender: Uuid,
    dossier_item: Uuid,
}

/// Организатор, объект и тендер с материалом досье - самой строгой из
/// append-only таблиц (INV-042): ее и стирает очистка.
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
        dossier_item,
    })
}

async fn tender_exists(db: &tou_db::Db, id: Uuid) -> Result<bool, sqlx::Error> {
    tou_db::purge::tender_exists(db, id).await
}

async fn dossier_item_exists(db: &tou_db::Db, id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM core.dossier_items WHERE id = $1) AS "exists!""#,
        id
    )
    .fetch_one(db)
    .await
}

/// Удаление тендера уносит его append-only хвост, не задевая соседей,
/// оставляет след в цепочке и возвращает сторожей.
#[tokio::test]
async fn purge_removes_a_tender_and_restores_the_guards() {
    let db = require_db!();
    let target = fixture(&db, "target").await.expect("тендер на удаление");
    let control = fixture(&db, "control").await.expect("контрольный тендер");

    let deleted = tou_db::purge::purge(&db, target.actor, Some(&[target.tender]))
        .await
        .expect("очистка одного тендера");
    assert_eq!(
        deleted.get("tenders"),
        Some(&1),
        "удален ровно один тендер: {deleted:?}"
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
        !tender_exists(&db, target.tender)
            .await
            .expect("проверка тендера")
    );
    assert!(
        !dossier_item_exists(&db, target.dossier_item)
            .await
            .expect("проверка материала досье")
    );
    assert!(
        tender_exists(&db, control.tender)
            .await
            .expect("проверка тендера"),
        "чужой тендер не тронут"
    );
    assert!(
        dossier_item_exists(&db, control.dossier_item)
            .await
            .expect("проверка материала досье")
    );

    // След: сводное событие с актором в той же цепочке, и цепочка цела
    let event = sqlx::query!(
        r#"SELECT actor_id, payload -> 'old' -> 'tender_ids' AS "tender_ids!"
           FROM audit.log WHERE table_name = 'core.data_purge'
           ORDER BY id DESC LIMIT 1"#
    )
    .fetch_one(&db)
    .await
    .expect("событие очистки");
    assert_eq!(event.actor_id, Some(target.actor));
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
        dossier_item_exists(&db, control.dossier_item)
            .await
            .expect("проверка материала досье")
    );

    // Уборка: контрольный тендер уходит той же очисткой, остальное - руками
    tou_db::purge::purge(&db, control.actor, Some(&[control.tender]))
        .await
        .expect("уборка контрольного тендера");
    for object in [target.object, control.object] {
        sqlx::query!("DELETE FROM core.objects WHERE id = $1", object)
            .execute(&db)
            .await
            .expect("уборка объекта");
    }
    sqlx::query!(
        "DELETE FROM core.users WHERE id = ANY($1)",
        &[target.actor, control.actor]
    )
    .execute(&db)
    .await
    .expect("уборка пользователей");
}

/// Без актора очистка отказывает целиком: событие без автора в аудите
/// недопустимо, а транзакция без `app.user_id` - это и есть вызов мимо
/// http-слоя.
#[tokio::test]
async fn purge_refuses_without_an_actor() {
    let db = require_db!();

    let refused = sqlx::query_scalar!(r#"SELECT core.purge_data(ARRAY[]::uuid[]) AS "deleted!""#)
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
}
