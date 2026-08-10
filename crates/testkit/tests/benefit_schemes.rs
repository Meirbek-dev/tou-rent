//! Льготные схемы против живой БД (T37, FR-1205, INV-095, INV-096, п. 95–96).
//!
//! INV-095: льгота образовательного оборудования применяется по согласованию
//! Ученого совета. INV-096: спин-офф обучает не менее пяти кредитов в семестр.
//! Расписание платы БД и домена - одна формула (паритет, как у календаря G12).
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use rust_decimal::Decimal;
use sqlx::Acquire as _;
use tou_domain::benefit::{Benefit, SPIN_OFF_MIN_CREDITS};
use tou_domain::money::Money;
use tou_domain::special::BenefitScheme;
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - льготы не проверялись");
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
    actor_id: Uuid,
}

/// Договор со ставкой 100 000 ₸ в месяц (база льготного расписания).
async fn fixture(tx: &mut sqlx::PgConnection) -> Result<Fixture, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let actor_id = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т37 организатор', now()) RETURNING id",
        format!("t37-actor-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let tenant_id = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т37 наниматель', now()) RETURNING id",
        format!("t37-tenant-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Т37 помещение', 'адрес', 60.00) RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await?;

    let contract_id = sqlx::query_scalar!(
        "INSERT INTO core.contracts (object_id, tenant_id, monthly_rate)
         VALUES ($1, $2, 100000.00) RETURNING id",
        object_id,
        tenant_id
    )
    .fetch_one(&mut *tx)
    .await?;

    Ok(Fixture {
        contract_id,
        actor_id,
    })
}

/// Одна и та же выдача льготы во всех проверках. Макрос, а не константа:
/// sqlx проверяет текст запроса на месте вызова.
macro_rules! grant {
    ($contract:expr, $scheme:expr, $decision:expr, $date:expr, $credits:expr, $actor:expr) => {
        sqlx::query!(
            "INSERT INTO core.benefit_grants
                (contract_id, scheme, communal_monthly, council_decision, council_date,
                 study_credits, internships, granted_by)
             VALUES ($1, $2, 18000.00, $3, $4, $5, 0, $6)",
            $contract,
            $scheme,
            $decision,
            $date,
            $credits,
            $actor
        )
    };
}

/// INV-095 (п. 95): без согласования Ученого совета льготы нет.
#[tokio::test]
async fn inv095_educational_benefit_needs_the_council() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    let error = rejected!(
        tx,
        grant!(
            f.contract_id,
            BenefitScheme::EducationalEquipment.as_str(),
            Option::<String>::None,
            Option::<time::Date>::None,
            0_i32,
            f.actor_id
        ),
        "льгота без согласования Ученого совета обязана быть отклонена"
    );
    assert!(error.contains("INV-095"), "{error}");

    grant!(
        f.contract_id,
        BenefitScheme::EducationalEquipment.as_str(),
        Some("Протокол Ученого совета № 7"),
        Some(time::macros::date!(2026 - 09 - 01)),
        0_i32,
        f.actor_id
    )
    .execute(&mut *tx)
    .await
    .expect("льгота по согласованию Ученого совета");
}

/// INV-096 (п. 96): спин-офф обучает не менее пяти кредитов в семестр.
#[tokio::test]
async fn inv096_spin_off_teaches_five_credits() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    let error = rejected!(
        tx,
        grant!(
            f.contract_id,
            BenefitScheme::SpinOff.as_str(),
            Option::<String>::None,
            Option::<time::Date>::None,
            SPIN_OFF_MIN_CREDITS - 1,
            f.actor_id
        ),
        "спин-офф без пяти кредитов обязан быть отклонен"
    );
    assert!(error.contains("INV-096"), "{error}");

    // Согласования Ученого совета спин-оффу не требуется (п. 96)
    grant!(
        f.contract_id,
        BenefitScheme::SpinOff.as_str(),
        Option::<String>::None,
        Option::<time::Date>::None,
        SPIN_OFF_MIN_CREDITS,
        f.actor_id
    )
    .execute(&mut *tx)
    .await
    .expect("льгота спин-оффа");
}

/// Расписание платы БД и домена - одна формула (п. 95–96).
#[tokio::test]
async fn schedule_matches_the_domain() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    grant!(
        f.contract_id,
        BenefitScheme::SpinOff.as_str(),
        Option::<String>::None,
        Option::<time::Date>::None,
        SPIN_OFF_MIN_CREDITS,
        f.actor_id
    )
    .execute(&mut *tx)
    .await
    .expect("льгота");

    let benefit = Benefit::new(BenefitScheme::SpinOff);
    let base = Money::new(Decimal::from(100_000));
    let communal = Money::new(Decimal::from(18_000));

    for year in 1..=3 {
        let from_db =
            sqlx::query_scalar!("SELECT core.benefit_monthly($1, $2)", f.contract_id, year)
                .fetch_one(&mut *tx)
                .await
                .expect("плата за год");
        let from_domain = benefit
            .monthly_for(year, base, communal)
            .expect("плата домена");
        assert_eq!(
            from_db.map(|value| value.normalize()),
            Some(from_domain.amount().normalize()),
            "год {year}: расписание БД и домена расходятся"
        );
    }
}

/// Без льготы плата равна ставке договора; у социальной схемы расписания
/// нет - Ксоц уже внутри ставки (FR-201).
#[tokio::test]
async fn plain_and_social_contracts_pay_the_rate() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    let plain = sqlx::query_scalar!("SELECT core.benefit_monthly($1, 1)", f.contract_id)
        .fetch_one(&mut *tx)
        .await
        .expect("плата без льготы");
    assert_eq!(plain.map(|v| v.normalize()), Some(Decimal::from(100_000)));

    grant!(
        f.contract_id,
        BenefitScheme::Social.as_str(),
        Option::<String>::None,
        Option::<time::Date>::None,
        0_i32,
        f.actor_id
    )
    .execute(&mut *tx)
    .await
    .expect("социальная льгота");

    for year in [1, 2] {
        let social =
            sqlx::query_scalar!("SELECT core.benefit_monthly($1, $2)", f.contract_id, year)
                .fetch_one(&mut *tx)
                .await
                .expect("плата социальной схемы");
        assert_eq!(
            social.map(|v| v.normalize()),
            Some(Decimal::from(100_000)),
            "Ксоц применяется расчетом ставки, а не расписанием"
        );
    }
}

/// Перечень схем БД и домена - один (паритет, G16).
#[tokio::test]
async fn scheme_catalog_matches_the_domain() {
    let db = require_db!();

    let mut codes = sqlx::query_scalar!("SELECT code FROM refdata.benefit_schemes ORDER BY code")
        .fetch_all(&db)
        .await
        .expect("каталог схем");
    codes.sort();

    let mut domain: Vec<String> = BenefitScheme::ALL
        .iter()
        .map(|scheme| scheme.as_str().to_owned())
        .collect();
    domain.sort();
    assert_eq!(codes, domain, "перечень льготных схем совпадает с доменом");

    // Условия схем - из ТЗ FR-1205
    let educational = sqlx::query!(
        "SELECT requires_council, min_study_credits FROM refdata.benefit_schemes
         WHERE code = $1",
        BenefitScheme::EducationalEquipment.as_str()
    )
    .fetch_one(&db)
    .await
    .expect("образовательная схема");
    let (council, credits) = (educational.requires_council, educational.min_study_credits);
    assert!(council, "п. 95: согласование Ученого совета");
    assert_eq!(credits, 0, "кредиты - условие спин-оффа, не оборудования");

    let credits = sqlx::query_scalar!(
        "SELECT min_study_credits FROM refdata.benefit_schemes WHERE code = $1",
        BenefitScheme::SpinOff.as_str()
    )
    .fetch_one(&db)
    .await
    .expect("спин-офф");
    assert_eq!(credits, SPIN_OFF_MIN_CREDITS, "п. 96: пять кредитов");
}
