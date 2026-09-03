//! Подписи материалов досье - словами, а не кодом перечня (G-118; FR-1602,
//! NFR-01, п. 16, 42, 116).
//!
//! Заголовок материала собирался в SQL прямо из значения столбца-
//! перечисления, и в русском делопроизводственном документе оказывались
//! английские коды: «Уклонение от подписания договора:
//! documents_deadline_missed», «Протокол admission №Д-4f2a1c», «Публикация
//! протокола results (п. 75)». Досье читает секретарь, организатор и
//! проверяющий - код перечня не читается никем и не переводится.
//!
//! Здесь проверяется то, что делает БД сама: основание уклонения приходит
//! в подпись формулировкой справочника `refdata.evasion_grounds`, а вид
//! протокола - словами делопроизводства.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use uuid::Uuid;

async fn try_pool() -> Result<Option<tou_db::Db>, sqlx::Error> {
    // Пропуск без адреса допустим локально, но не в пайплайне (G2/G15):
    // молча пройденный интеграционный тест ничего не проверяет
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - подписи досье не проверялись");
                return;
            }
        }
    };
}

struct Fixture {
    tender: Uuid,
    lot: Uuid,
    object: Uuid,
    tenant: Uuid,
}

async fn fixture(tx: &mut sqlx::PgConnection) -> Result<Fixture, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let organizer = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'argon2-заглушка', 'G118 организатор', core.now()) RETURNING id",
        format!("g118-organizer-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let tenant = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'argon2-заглушка', 'G118 наниматель', core.now()) RETURNING id",
        format!("g118-tenant-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let object = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', $1, 'адрес', 12.00) RETURNING id",
        format!("G118 объект {tag}")
    )
    .fetch_one(&mut *tx)
    .await?;

    let tender = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id)
         VALUES ($1, 'summed_up', $2) RETURNING id",
        format!("G118 тендер {tag}"),
        organizer
    )
    .fetch_one(&mut *tx)
    .await?;

    let lot = sqlx::query_scalar!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                base_rate_monthly, guarantee_fee, rate_calculation)
         VALUES ($1, 1, $2, 'офис', 12, 50000.00, 50000.00, '{}'::jsonb) RETURNING id",
        tender,
        object
    )
    .fetch_one(&mut *tx)
    .await?;

    Ok(Fixture {
        tender,
        lot,
        object,
        tenant,
    })
}

async fn title_of(
    tx: &mut sqlx::PgConnection,
    tender: Uuid,
    kind: &str,
) -> Result<String, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT coalesce(title, '') AS "title!" FROM core.dossier_items
           WHERE tender_id = $1 AND kind = $2
           ORDER BY occurred_at DESC LIMIT 1"#,
        tender,
        kind
    )
    .fetch_one(tx)
    .await
}

/// FR-1602, п. 55, 73–76: вид протокола в подписи материала назван словами
/// делопроизводства, а не значением перечисления `core.protocol_kind`.
#[tokio::test]
async fn protocol_items_name_the_kind_in_words() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    let content = serde_json::json!({ "number": "И-g118" });
    let protocol = sqlx::query_scalar!(
        "INSERT INTO core.protocols (tender_id, kind, number, content, pdf_key)
         VALUES ($1, 'results', 'И-g118', $2, 'protocols/g118/results.pdf') RETURNING id",
        f.tender,
        content
    )
    .fetch_one(&mut *tx)
    .await
    .expect("протокол итогов");

    let title = title_of(&mut tx, f.tender, "protocol")
        .await
        .expect("подпись протокола");
    assert_eq!(
        title, "Протокол итогов №И-g118",
        "вид протокола в досье называется словами (FR-1602)"
    );
    assert!(
        !title.contains("results"),
        "код перечня в подписи материала недопустим: {title}"
    );

    sqlx::query!(
        "UPDATE core.protocols SET published_at = core.now() WHERE id = $1",
        protocol
    )
    .execute(&mut *tx)
    .await
    .expect("публикация протокола");

    let published = title_of(&mut tx, f.tender, "publication")
        .await
        .expect("подпись публикации");
    assert_eq!(
        published, "Публикация протокола итогов (п. 75)",
        "подпись публикации строится тем же словом"
    );

    tx.rollback().await.expect("rollback");
}

/// FR-1602, п. 116: основание уклонения приходит в подпись формулировкой
/// справочника `refdata.evasion_grounds`, а не своим кодом.
#[tokio::test]
async fn evasion_items_name_the_ground_from_the_catalogue() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    let contract = sqlx::query_scalar!(
        "INSERT INTO core.contracts
           (tender_id, lot_id, object_id, tenant_id, place, status, monthly_rate,
            lease_months, drafted_at, handed_to_tenant_at)
         VALUES ($1, $2, $3, $4, 'winner', 'draft', 79750.00, 12,
                 core.now() - interval '2 days', core.now() - interval '1 day')
         RETURNING id",
        f.tender,
        f.lot,
        f.object,
        f.tenant
    )
    .fetch_one(&mut *tx)
    .await
    .expect("договор победителя");

    sqlx::query!(
        "INSERT INTO core.evasions (contract_id, tender_id, lot_id, user_id, place, ground)
         VALUES ($1, $2, $3, $4, 'winner', 'documents_deadline_missed')",
        contract,
        f.tender,
        f.lot,
        f.tenant
    )
    .execute(&mut *tx)
    .await
    .expect("факт уклонения");

    let title = title_of(&mut tx, f.tender, "evasion")
        .await
        .expect("подпись уклонения");
    let expected = sqlx::query_scalar!(
        "SELECT label_ru FROM refdata.evasion_grounds WHERE code = 'documents_deadline_missed'"
    )
    .fetch_one(&mut *tx)
    .await
    .expect("формулировка основания");

    assert_eq!(
        title,
        format!("Уклонение от подписания договора: {expected}"),
        "основание уклонения берется из справочника (п. 116)"
    );
    assert!(
        !title.contains("documents_deadline_missed"),
        "код перечня в подписи материала недопустим: {title}"
    );

    tx.rollback().await.expect("rollback");
}
