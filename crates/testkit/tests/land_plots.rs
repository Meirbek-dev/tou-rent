//! Земельные участки против живой БД (T40, FR-1801, INV-105, п. 104–107).
//!
//! Проверяется то, что делает БД сама: заявка подается только по
//! опубликованному участку, решение Правления переводит заявку в свое
//! терминальное состояние и неизменяемо, договор заключается лишь по
//! удовлетворенной заявке, а особые условия п. 107 нельзя ни снять,
//! ни обойти подписанием (INV-105).
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use sqlx::Acquire as _;
use tou_domain::land::{Covenant, LandDecision, LandDesignation};
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - участки не проверялись");
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

/// То же, что [`rejected!`], но для запроса с `RETURNING`: проверенный макрос
/// возвращает строку, а не число затронутых записей, и `execute` у него нет.
macro_rules! rejected_returning {
    ($tx:expr, $query:expr, $why:expr) => {{
        let mut sp = $tx.begin().await.expect("savepoint");
        let error = $query.fetch_one(&mut *sp).await.expect_err($why);
        sp.rollback().await.expect("rollback savepoint");
        error.to_string()
    }};
}

struct Fixture {
    plot_id: Uuid,
    investor_id: Uuid,
    staff_id: Uuid,
}

/// Участок с характеристиками раздела 14, инвестор и сотрудник.
async fn fixture(tx: &mut sqlx::PgConnection, published: bool) -> Result<Fixture, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    // `$1::citext`: почта хранится регистронезависимым типом, а привязка
    // приходит текстом
    let investor_id = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'Т40 инвестор', now()) RETURNING id",
        format!("t40-investor-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let staff_id = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'Т40 Правление', now()) RETURNING id",
        format!("t40-board-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let plot_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('land_plot', 'Т40 участок', 'г. Павлодар, ул. Тестовая, 1', 5000)
         RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query!(
        "INSERT INTO core.land_plots
           (object_id, cadastral_number, designation, permitted_use, published_at)
         VALUES ($1, '14-000-000-000', $2, 'строительство общежития',
                 CASE WHEN $3 THEN core.now() END)",
        plot_id,
        LandDesignation::Dormitory.as_str(),
        // Отметка - от сервера (`core.now()`, ADR-0005)
        published
    )
    .execute(&mut *tx)
    .await?;

    Ok(Fixture {
        plot_id,
        investor_id,
        staff_id,
    })
}

/// Заявка инвестора (п. 105). Макрос вместо константы: sqlx проверяет запрос
/// по схеме и принимает только строковый литерал.
macro_rules! apply {
    ($plot_id:expr, $investor_id:expr) => {
        sqlx::query_scalar!(
            "INSERT INTO core.land_applications
                (plot_id, investor_id, project, investment_amount, term_months)
             VALUES ($1, $2, 'Строительство общежития на 500 мест', 500000000, 120)
             RETURNING id",
            $plot_id,
            $investor_id
        )
    };
}

/// Решение Правления (п. 106). `$2::text::core.land_decision`: значение
/// приходит строкой доменного типа, а приведение делает БД.
macro_rules! decide {
    ($application:expr, $decision:expr, $staff_id:expr) => {
        sqlx::query!(
            "INSERT INTO core.land_decisions
                (land_application_id, decision, rationale, decided_by)
             VALUES ($1, $2::text::core.land_decision, 'обоснование решения', $3)",
            $application,
            $decision,
            $staff_id
        )
    };
}

/// FR-1801 (п. 104–105): заявка подается только по опубликованному участку.
#[tokio::test]
async fn fr1801_application_needs_a_published_plot() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, false)
        .await
        .expect("участок без публикации");

    let error = rejected_returning!(
        tx,
        apply!(f.plot_id, f.investor_id),
        "заявка по неопубликованному участку обязана быть отклонена"
    );
    assert!(error.contains("FR-1801"), "{error}");

    // После публикации та же заявка проходит
    sqlx::query!(
        "UPDATE core.land_plots SET published_at = now() WHERE object_id = $1",
        f.plot_id
    )
    .execute(&mut *tx)
    .await
    .expect("публикация характеристик участка");

    let id = apply!(f.plot_id, f.investor_id)
        .fetch_one(&mut *tx)
        .await
        .expect("заявка по опубликованному участку");
    assert!(!id.is_nil());
}

/// FR-1801 (п. 106): решение переводит заявку в свое состояние, повторное
/// решение и правка принятого отклоняются.
#[tokio::test]
async fn fr1801_decision_is_final() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, true).await.expect("участок");

    let application = apply!(f.plot_id, f.investor_id)
        .fetch_one(&mut *tx)
        .await
        .expect("заявка");

    decide!(application, LandDecision::Grant.as_str(), f.staff_id)
        .execute(&mut *tx)
        .await
        .expect("решение Правления");

    // `!` у `status`: `::text` планировщик считает потенциально NULL,
    // хотя столбец NOT NULL
    let status = sqlx::query_scalar!(
        r#"SELECT status::text AS "status!" FROM core.land_applications WHERE id = $1"#,
        application
    )
    .fetch_one(&mut *tx)
    .await
    .expect("состояние заявки");
    assert_eq!(status, "granted", "решение переводит заявку (п. 106)");

    let again = rejected!(
        tx,
        decide!(application, LandDecision::Refuse.as_str(), f.staff_id),
        "повторное решение обязано быть отклонено"
    );
    assert!(
        again.contains("land_decisions_land_application_id_key") || again.contains("duplicate key"),
        "{again}"
    );

    let rewritten = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.land_decisions SET rationale = 'другое' WHERE land_application_id = $1",
            application
        ),
        "правка принятого решения обязана быть отклонена"
    );
    assert!(
        rewritten.contains("FR-1801") || rewritten.contains("permission denied"),
        "{rewritten}"
    );
}

/// FR-1801 (п. 106–107): договор заключается только по удовлетворенной заявке.
#[tokio::test]
async fn fr1801_contract_needs_a_granted_application() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, true).await.expect("участок");

    let application = apply!(f.plot_id, f.investor_id)
        .fetch_one(&mut *tx)
        .await
        .expect("заявка");

    let contract = sqlx::query_scalar!(
        "INSERT INTO core.contracts (object_id, tenant_id, monthly_rate)
         VALUES ($1, $2, 100000) RETURNING id",
        f.plot_id,
        f.investor_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("договор");

    let error = rejected!(
        tx,
        sqlx::query!(
            "INSERT INTO core.land_contracts
               (contract_id, land_application_id, investment_amount)
             VALUES ($1, $2, 500000000)",
            contract,
            application
        ),
        "договор по нерассмотренной заявке обязан быть отклонен"
    );
    assert!(error.contains("FR-1801"), "{error}");
}

/// INV-105 (п. 107): договор на участок не подписывается без особых условий,
/// а внесенное условие не снимается.
#[tokio::test]
async fn inv105_covenants_are_required_and_permanent() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, true).await.expect("участок");

    let application = apply!(f.plot_id, f.investor_id)
        .fetch_one(&mut *tx)
        .await
        .expect("заявка");

    decide!(application, LandDecision::Grant.as_str(), f.staff_id)
        .execute(&mut *tx)
        .await
        .expect("решение Правления");

    let contract = sqlx::query_scalar!(
        "INSERT INTO core.contracts (object_id, tenant_id, monthly_rate, lease_period)
         VALUES ($1, $2, 100000, tstzrange(now(), now() + interval '10 years'))
         RETURNING id",
        f.plot_id,
        f.investor_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("договор");

    sqlx::query!(
        "INSERT INTO core.land_contracts (contract_id, land_application_id, investment_amount)
         VALUES ($1, $2, 500000000)",
        contract,
        application
    )
    .execute(&mut *tx)
    .await
    .expect("договор на участок");

    macro_rules! sign {
        ($contract:expr) => {
            sqlx::query!(
                "UPDATE core.contracts SET status = 'signing' WHERE id = $1",
                $contract
            )
        };
    }

    macro_rules! add_covenant {
        ($contract:expr, $code:expr) => {
            sqlx::query!(
                "INSERT INTO core.land_contract_covenants (contract_id, code) VALUES ($1, $2)",
                $contract,
                $code
            )
        };
    }

    // Без условий подписание невозможно
    let bare = rejected!(
        tx,
        sign!(contract),
        "подписание без особых условий обязано быть отклонено"
    );
    assert!(bare.contains("INV-105"), "{bare}");

    // Одного условия мало: залог запрещен и для участка, и для зданий
    add_covenant!(contract, Covenant::NoPledgePlot.as_str())
        .execute(&mut *tx)
        .await
        .expect("первое условие");

    let partial = rejected!(
        tx,
        sign!(contract),
        "подписание с неполным комплектом обязано быть отклонено"
    );
    assert!(partial.contains("INV-105"), "{partial}");

    add_covenant!(contract, Covenant::NoPledgeBuildings.as_str())
        .execute(&mut *tx)
        .await
        .expect("второе условие");

    sign!(contract)
        .execute(&mut *tx)
        .await
        .expect("подписание с полным комплектом");

    // Снять условие нельзя - ни удалением, ни подменой кода
    let removed = rejected!(
        tx,
        sqlx::query!(
            "DELETE FROM core.land_contract_covenants WHERE contract_id = $1",
            contract
        ),
        "снятие особого условия обязано быть отклонено"
    );
    assert!(
        removed.contains("INV-105") || removed.contains("permission denied"),
        "{removed}"
    );

    let swapped = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.land_contract_covenants SET code = $2
             WHERE contract_id = $1 AND code = $3",
            contract,
            Covenant::NoPledgeBuildings.as_str(),
            Covenant::NoPledgePlot.as_str()
        ),
        "подмена особого условия обязана быть отклонена"
    );
    assert!(
        swapped.contains("INV-105") || swapped.contains("permission denied"),
        "{swapped}"
    );
}

/// Паритет справочника особых условий с доменом (INV-105): перечень п. 107
/// закрыт, и обе стороны знают один и тот же состав.
#[tokio::test]
async fn covenant_catalog_matches_the_domain() {
    let db = require_db!();

    let codes = sqlx::query_scalar!("SELECT code FROM refdata.land_covenants")
        .fetch_all(&db)
        .await
        .expect("справочник условий");

    let mut from_db: Vec<String> = codes;
    from_db.sort();
    let mut from_domain: Vec<String> = Covenant::ALL
        .into_iter()
        .map(|covenant| covenant.as_str().to_owned())
        .collect();
    from_domain.sort();

    assert_eq!(
        from_db, from_domain,
        "перечень особых условий п. 107 совпадает в БД и домене"
    );
}

/// Характеристики раздела 14 заводятся на земельный участок, а не на
/// помещение (FR-1801, FR-101).
#[tokio::test]
async fn fr1801_characteristics_belong_to_a_land_plot() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");

    let premises = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Т40 помещение', 'г. Павлодар, ул. Тестовая, 2', 40)
         RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await
    .expect("помещение");

    let error = rejected!(
        tx,
        sqlx::query!(
            "INSERT INTO core.land_plots (object_id, cadastral_number, designation, permitted_use)
             VALUES ($1, '14-000-000-001', 'other', 'иное')",
            premises
        ),
        "характеристики участка на помещении обязаны быть отклонены"
    );
    assert!(error.contains("FR-1801"), "{error}");
}
