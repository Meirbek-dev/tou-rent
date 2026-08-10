//! Договорный конвейер против живой БД (T24, FR-901–902, FR-905, INV-115).
//!
//! Проверяется последний рубеж: существенные условия неизменяемы, подпись
//! наймодателя невозможна без завершенной сверки, регистрация - только
//! у договора, подписанного обеими сторонами.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use sqlx::Acquire as _;
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - конвейер не проверялся");
                return;
            }
        }
    };
}

macro_rules! rejected {
    ($tx:expr, $query:expr, $why:expr) => {{
        let mut sp = $tx.begin().await.expect("savepoint");
        let error = $query.execute(&mut *sp).await.expect_err($why);
        sp.rollback().await.expect("rollback savepoint");
        error.to_string()
    }};
}

struct Fixture {
    contract_id: Uuid,
    object_id: Uuid,
}

/// Договор в состоянии «документы представлены»: остались сверка и подписи.
async fn fixture(tx: &mut sqlx::PgConnection) -> Result<Fixture, sqlx::Error> {
    let mut user = async |tag: &str| -> Result<Uuid, sqlx::Error> {
        sqlx::query_scalar!(
            "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
             VALUES ($1::citext, 'x', $2, now()) RETURNING id",
            format!("t24-{tag}-{}@tou.test", Uuid::now_v7().simple()),
            format!("Т24 {tag}")
        )
        .fetch_one(&mut *tx)
        .await
    };
    let tenant = user("наниматель").await?;

    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Т24 объект', 'адрес', 10.00) RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await?;

    let contract_id = sqlx::query_scalar!(
        "INSERT INTO core.contracts
           (object_id, tenant_id, status, monthly_rate, lease_months,
            drafted_at, handed_to_tenant_at, tenant_signed_at, documents_received_at)
         VALUES ($1, $2, 'draft', 79750.00, 12, now(), now(), now(), now())
         RETURNING id",
        object_id,
        tenant
    )
    .fetch_one(&mut *tx)
    .await?;

    // Чек-лист п. 113: две позиции, ни одна не отмечена
    sqlx::query!(
        "INSERT INTO core.contract_checklists (contract_id, item_code)
         VALUES ($1, 'bank_details'), ($1, 'fee_receipt')",
        contract_id
    )
    .execute(&mut *tx)
    .await?;

    Ok(Fixture {
        contract_id,
        object_id,
    })
}

/// FR-901: существенные условия договора неизменяемы после составления.
#[tokio::test]
async fn fr901_essential_terms_are_frozen() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    // Столбец больше не подставляется в текст запроса: каждое существенное
    // условие правится своим запросом, и схему проверяет сборка
    for (column, error) in [
        (
            "monthly_rate",
            rejected!(
                tx,
                sqlx::query!(
                    "UPDATE core.contracts SET monthly_rate = 1000 WHERE id = $1",
                    f.contract_id
                ),
                "правка существенного условия обязана быть отклонена"
            ),
        ),
        (
            "lease_months",
            rejected!(
                tx,
                sqlx::query!(
                    "UPDATE core.contracts SET lease_months = 6 WHERE id = $1",
                    f.contract_id
                ),
                "правка существенного условия обязана быть отклонена"
            ),
        ),
    ] {
        assert!(
            error.contains("FR-901"),
            "ожидали отказ FR-901 для {column}: {error}"
        );
    }

    // Прочие поля правятся свободно: конвейеру нужно двигаться
    sqlx::query!(
        "UPDATE core.contracts SET copy_sent_at = now() WHERE id = $1",
        f.contract_id
    )
    .execute(&mut *tx)
    .await
    .expect("шаг конвейера не блокируется");
}

/// INV-115: наймодатель не подписывает договор без завершенной сверки.
#[tokio::test]
async fn inv115_landlord_signature_requires_the_checklist() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    let error = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.contracts SET landlord_signed_at = now() WHERE id = $1",
            f.contract_id
        ),
        "подпись без сверки обязана быть отклонена"
    );
    assert!(error.contains("INV-115"), "ожидали отказ INV-115: {error}");

    // Отмечаем перечень целиком - подпись проходит
    sqlx::query!(
        "UPDATE core.contract_checklists SET checked_at = now() WHERE contract_id = $1",
        f.contract_id
    )
    .execute(&mut *tx)
    .await
    .expect("сверка");

    sqlx::query!(
        "UPDATE core.contracts SET landlord_signed_at = now() WHERE id = $1",
        f.contract_id
    )
    .execute(&mut *tx)
    .await
    .expect("подпись после сверки");

    let done = sqlx::query_scalar!(
        "SELECT checklist_done_at FROM core.contracts WHERE id = $1",
        f.contract_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("отметка сверки");
    assert!(done.is_some(), "БД фиксирует момент завершения сверки");
}

/// FR-905: регистрируется договор, подписанный обеими сторонами, и только
/// с номером журнала; период найма защищает объект от пересечений.
#[tokio::test]
async fn fr905_registration_requires_both_signatures_and_a_number() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    let unsigned = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.contracts SET registered_at = now(), reg_number = 'Д-1' WHERE id = $1",
            f.contract_id
        ),
        "регистрация без подписи наймодателя обязана быть отклонена"
    );
    assert!(unsigned.contains("FR-905"), "{unsigned}");

    sqlx::query!(
        "UPDATE core.contract_checklists SET checked_at = now() WHERE contract_id = $1",
        f.contract_id
    )
    .execute(&mut *tx)
    .await
    .expect("сверка");
    sqlx::query!(
        "UPDATE core.contracts SET landlord_signed_at = now() WHERE id = $1",
        f.contract_id
    )
    .execute(&mut *tx)
    .await
    .expect("подпись");

    let no_number = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.contracts SET registered_at = now() WHERE id = $1",
            f.contract_id
        ),
        "регистрация без номера журнала обязана быть отклонена"
    );
    assert!(no_number.contains("FR-905"), "{no_number}");

    sqlx::query!(
        "UPDATE core.contracts
         SET registered_at = now(), reg_number = $2, status = 'signing',
             lease_period = tstzrange(now(), now() + interval '12 months', '[)')
         WHERE id = $1",
        f.contract_id,
        format!("Д-{}", Uuid::now_v7().simple())
    )
    .execute(&mut *tx)
    .await
    .expect("регистрация подписанного договора");

    // INV-DB-02: тот же объект не сдается на пересекающийся период
    let overlap = rejected!(
        tx,
        sqlx::query!(
            "INSERT INTO core.contracts
               (object_id, tenant_id, status, monthly_rate, lease_months, lease_period)
             SELECT $1::uuid, tenant_id, 'signing', 1000, 12,
                    tstzrange(now(), now() + interval '6 months', '[)')
             FROM core.contracts WHERE id = $2",
            f.object_id,
            f.contract_id
        ),
        "пересекающаяся аренда обязана быть отклонена"
    );
    assert!(
        overlap.contains("no_overlapping_lease") || overlap.contains("exclusion"),
        "ожидали отказ INV-DB-02: {overlap}"
    );
}

/// Перечень сверки - закрытый справочник п. 113 (FK).
#[tokio::test]
async fn checklist_items_come_from_the_reference_list() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    let error = rejected!(
        tx,
        sqlx::query!(
            "INSERT INTO core.contract_checklists (contract_id, item_code)
             VALUES ($1, 'выдуманное')",
            f.contract_id
        ),
        "позиция вне перечня п. 113 обязана быть отклонена"
    );
    assert!(
        error.contains("item_code") || error.contains("checklist_item_known"),
        "{error}"
    );

    let count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM refdata.checklist_items"#)
        .fetch_one(&mut *tx)
        .await
        .expect("перечень");
    assert!(count > 0, "перечень п. 113 заполнен");
}
