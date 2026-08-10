//! Шифрование ценовых предложений против живой БД (T29, INV-040, п. 40).
//!
//! RLS запрещает читать цену до вскрытия, шифрование добавляет второй
//! рубеж: в таблице лежит не число, а шифртекст на ключе тендера. Без
//! ключа цена не читается и не записывается, чужой ключ ее не раскрывает,
//! а ключи разных тендеров не подходят друг к другу.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use rust_decimal::Decimal;
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - шифрование не проверялось");
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
    tender_id: Uuid,
    application_id: Uuid,
}

/// Тендер с поданной заявкой: цена записывается ключом этого тендера.
async fn fixture(tx: &mut sqlx::PgConnection) -> Result<Fixture, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let organizer = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т29 организатор', now()) RETURNING id",
        format!("t29-org-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Т29 объект', 'адрес', 12.00) RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await?;

    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id, announced_at,
                                   submission_deadline, opening_at)
         VALUES ('Т29 тендер', 'accepting', $1, now() - interval '15 days',
                 now() + interval '5 days', now() + interval '6 days')
         RETURNING id",
        organizer
    )
    .fetch_one(&mut *tx)
    .await?;

    let lot_id = sqlx::query_scalar!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                base_rate_monthly, rate_calculation, guarantee_fee)
         VALUES ($1, 1, $2, 'офис', 12, 30000.00, '{}'::jsonb, 30000.00)
         RETURNING id",
        tender_id,
        object_id
    )
    .fetch_one(&mut *tx)
    .await?;

    let application_id = sqlx::query_scalar!(
        r#"INSERT INTO core.applications
             (tender_id, lot_id, participant_id, applicant_kind, applicant_details)
           VALUES ($1, $2, $3, 'legal_entity', '{"name": "ТОО Т29"}'::jsonb)
           RETURNING id"#,
        tender_id,
        lot_id,
        organizer
    )
    .fetch_one(&mut *tx)
    .await?;

    // RLS INV-040: ценовое предложение подает сам участник - актор запроса
    // должен совпадать с ним (политика `insert_own_proposal`)
    sqlx::query!(
        "SELECT set_config('app.user_id', $1, true)",
        organizer.to_string()
    )
    .fetch_one(&mut *tx)
    .await?;

    Ok(Fixture {
        tender_id,
        application_id,
    })
}

/// Одно и то же ценовое предложение во всех проверках. Макрос, а не
/// константа: sqlx проверяет текст запроса на месте вызова.
macro_rules! proposal {
    ($application_id:expr) => {
        sqlx::query!(
            "INSERT INTO core.price_proposals (application_id, amount) VALUES ($1, 36000.00)",
            $application_id
        )
    };
}

/// INV-040 (п. 40): в таблице лежит шифртекст, открытой цены нет,
/// а расшифровка ключом тендера возвращает исходное значение.
#[tokio::test]
async fn inv040_price_is_stored_encrypted() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    proposal!(f.application_id)
        .execute(&mut *tx)
        .await
        .expect("ценовое предложение");

    let row = sqlx::query!(
        "SELECT amount, amount_enc FROM core.price_proposals WHERE application_id = $1",
        f.application_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("предложение");
    let (plain, cipher) = (row.amount, row.amount_enc);

    assert!(
        plain.is_none(),
        "открытая цена в таблице не остается (п. 40)"
    );
    let cipher = cipher.expect("шифртекст");
    assert!(cipher.len() > 16, "цена зашифрована pgp_sym_encrypt");
    assert!(
        !String::from_utf8_lossy(&cipher).contains("36000"),
        "исходное число в шифртексте не читается"
    );

    // Без `!`: NULL здесь - осмысленный ответ (цена запечатана или ключа нет)
    let decrypted = sqlx::query_scalar!(
        "SELECT core.price_amount(p) FROM core.price_proposals p WHERE p.application_id = $1",
        f.application_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("расшифровка");
    assert_eq!(
        decrypted,
        Some(Decimal::new(3600000, 2)),
        "ключ тендера раскрывает цену"
    );
}

/// Без ключа цена не записывается (INV-040): запись «в открытую» невозможна.
#[tokio::test]
async fn price_cannot_be_written_without_the_key() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    sqlx::query!("SELECT set_config('app.price_key', '', true)")
        .fetch_one(&mut *tx)
        .await
        .expect("сброс ключа");

    let error = rejected!(
        tx,
        proposal!(f.application_id),
        "запись цены без ключа обязана быть отклонена"
    );
    assert!(error.contains("INV-040"), "{error}");
}

/// Без ключа и с чужим ключом цена не раскрывается (п. 40).
#[tokio::test]
async fn wrong_key_does_not_reveal_the_price() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    proposal!(f.application_id)
        .execute(&mut *tx)
        .await
        .expect("ценовое предложение");

    for key in ["", "чужой-ключ"] {
        sqlx::query!("SELECT set_config('app.price_key', $1, true)", key)
            .fetch_one(&mut *tx)
            .await
            .expect("подмена ключа");

        // Без `!`: невскрытая цена и есть NULL - это и проверяет тест
        let value = sqlx::query_scalar!(
            "SELECT core.price_amount(p) FROM core.price_proposals p WHERE p.application_id = $1",
            f.application_id
        )
        .fetch_one(&mut *tx)
        .await
        .expect("чтение цены");

        assert_eq!(value, None, "с ключом «{key}» цена не раскрывается (п. 40)");
    }
}

/// Ключ у каждого тендера свой (п. 40): ключ одного не расшифровывает другой.
#[tokio::test]
async fn tender_keys_are_derived_per_tender() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let first = fixture(&mut tx).await.expect("первый тендер");
    let second = fixture(&mut tx).await.expect("второй тендер");

    // Псевдонимы нужны только макросу: без них у обеих колонок одно имя
    let keys = sqlx::query!(
        "SELECT core.tender_price_key($1) AS key_a, core.tender_price_key($2) AS key_b",
        first.tender_id,
        second.tender_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("ключи тендеров");

    let key_a = keys.key_a.expect("ключ первого тендера");
    let key_b = keys.key_b.expect("ключ второго тендера");
    assert_ne!(key_a, key_b, "ключ выводится из идентификатора тендера");
    assert_eq!(key_a.len(), 64, "производный ключ - sha256 в hex");

    // Мастер-ключ в базе не хранится: он приходит соединением
    let stored = sqlx::query_scalar!("SELECT core.price_key()")
        .fetch_one(&mut *tx)
        .await
        .expect("ключ соединения");
    assert!(stored.is_some(), "ключ приходит из окружения приложения");
}
