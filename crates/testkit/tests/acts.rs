//! Акты приема-передачи и возврата против живой БД (T25, FR-904).
//!
//! Проверяется то, что делает БД сама: порядок актов, начисление платы
//! с даты передачи, закрытие договора возвратом и освобождение объекта
//! (FR-103) - следствия наступают и при вставке мимо приложения.
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - акты не проверялись");
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

/// Зарегистрированный договор: акты возможны только после регистрации.
async fn fixture(tx: &mut sqlx::PgConnection, registered: bool) -> Result<Fixture, sqlx::Error> {
    let tenant = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т25 наниматель', now()) RETURNING id",
        format!("t25-{}@tou.test", Uuid::now_v7().simple())
    )
    .fetch_one(&mut *tx)
    .await?;

    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Т25 объект', 'адрес', 10.00) RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await?;

    let contract_id = sqlx::query_scalar!(
        "INSERT INTO core.contracts
           (object_id, tenant_id, status, monthly_rate, lease_months,
            drafted_at, handed_to_tenant_at, tenant_signed_at, documents_received_at)
         VALUES ($1, $2, 'draft', 50000.00, 12, now(), now(), now(), now())
         RETURNING id",
        object_id,
        tenant
    )
    .fetch_one(&mut *tx)
    .await?;

    if registered {
        // Регистрация требует сверки и обеих подписей (T24)
        sqlx::query!(
            "INSERT INTO core.contract_checklists (contract_id, item_code, checked_at)
             VALUES ($1, 'bank_details', now())",
            contract_id
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(
            "UPDATE core.contracts
             SET landlord_signed_at = now(), registered_at = now(), reg_number = $2,
                 status = 'signing',
                 lease_period = tstzrange(now(), now() + interval '12 months', '[)')
             WHERE id = $1",
            contract_id,
            format!("Д-{}", Uuid::now_v7().simple())
        )
        .execute(&mut *tx)
        .await?;
    }

    Ok(Fixture {
        contract_id,
        object_id,
    })
}

/// Один и тот же акт во всех проверках. Макрос, а не константа: sqlx
/// проверяет текст запроса на месте вызова. Вид акта приходит текстом,
/// приведение к перечислению делает БД - доменного типа у макроса нет.
macro_rules! act {
    ($contract:expr, $kind:expr, $date:expr) => {
        sqlx::query!(
            "INSERT INTO core.acts (contract_id, kind, act_date)
             VALUES ($1, $2::text::core.act_kind, $3)",
            $contract,
            $kind,
            $date
        )
    };
}

/// FR-904: передавать можно зарегистрированный договор, возвращать -
/// только переданный объект.
#[tokio::test]
async fn fr904_acts_follow_the_order() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");

    let draft = fixture(&mut tx, false).await.expect("незарегистрированный");
    let early = rejected!(
        tx,
        act!(
            draft.contract_id,
            "handover",
            time::macros::date!(2026 - 08 - 10)
        ),
        "передача незарегистрированного договора обязана быть отклонена"
    );
    assert!(early.contains("FR-904"), "{early}");

    let f = fixture(&mut tx, true).await.expect("зарегистрированный");
    let no_handover = rejected!(
        tx,
        act!(f.contract_id, "return", time::macros::date!(2026 - 09 - 15)),
        "возврат непереданного объекта обязан быть отклонен"
    );
    assert!(no_handover.contains("FR-904"), "{no_handover}");
}

/// FR-904 (п. 122, 128–129): с даты акта приема-передачи начисляется плата,
/// договор действует, объект считается сданным (FR-103).
#[tokio::test]
async fn handover_starts_the_rent_and_marks_the_object_leased() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, true).await.expect("фикстура");

    act!(
        f.contract_id,
        "handover",
        time::macros::date!(2026 - 08 - 10)
    )
    .execute(&mut *tx)
    .await
    .expect("акт приема-передачи");

    let row = sqlx::query!(
        r#"SELECT status::text AS "status!", rent_starts_on,
                  lower(lease_period) AS period_from
             FROM core.contracts WHERE id = $1"#,
        f.contract_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("договор");
    let (status, rent_from, period_from) = (row.status, row.rent_starts_on, row.period_from);

    assert_eq!(status, "active", "передача вводит договор в действие");
    assert_eq!(
        rent_from,
        Some(time::macros::date!(2026 - 08 - 10)),
        "плата начисляется с даты акта (п. 128–129)"
    );
    assert_eq!(
        period_from.map(|ts| ts.date()),
        Some(time::macros::date!(2026 - 08 - 10)),
        "период найма начинается датой передачи"
    );

    // `!` - потому что это представление: планировщик отдает его колонки
    // как потенциально NULL, хотя статус у объекта есть всегда
    let object_status = sqlx::query_scalar!(
        r#"SELECT status AS "status!" FROM core.object_statuses WHERE object_id = $1"#,
        f.object_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("статус объекта");
    assert_eq!(object_status, "leased", "объект сдан (FR-103)");
}

/// Возврат закрывает договор и освобождает объект (п. 129, FR-103).
#[tokio::test]
async fn return_closes_the_contract_and_frees_the_object() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, true).await.expect("фикстура");

    for (kind, date) in [
        ("handover", time::macros::date!(2026 - 08 - 10)),
        ("return", time::macros::date!(2026 - 09 - 15)),
    ] {
        act!(f.contract_id, kind, date)
            .execute(&mut *tx)
            .await
            .expect("акт");
    }

    let row = sqlx::query!(
        r#"SELECT status::text AS "status!", upper(lease_period) AS period_to
             FROM core.contracts WHERE id = $1"#,
        f.contract_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("договор");
    let (status, period_to) = (row.status, row.period_to);

    assert_eq!(status, "completed", "возврат закрывает договор");
    assert_eq!(
        period_to.map(|ts| ts.date()),
        Some(time::macros::date!(2026 - 09 - 15)),
        "период найма закрывается датой возврата"
    );

    let object_status = sqlx::query_scalar!(
        r#"SELECT status AS "status!" FROM core.object_statuses WHERE object_id = $1"#,
        f.object_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("статус объекта");
    assert_eq!(object_status, "free", "объект снова свободен (FR-103)");
}

/// Акт - юридический факт: удалить его нельзя.
#[tokio::test]
async fn acts_cannot_be_deleted() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, true).await.expect("фикстура");

    act!(
        f.contract_id,
        "handover",
        time::macros::date!(2026 - 08 - 10)
    )
    .execute(&mut *tx)
    .await
    .expect("акт");

    let error = rejected!(
        tx,
        sqlx::query!(
            "DELETE FROM core.acts WHERE contract_id = $1",
            f.contract_id
        ),
        "удаление акта обязано быть отклонено"
    );
    assert!(
        error.contains("FR-904") || error.contains("permission denied"),
        "акт удаляться не должен: {error}"
    );
}
