//! Конкуренция заявок особого порядка против живой БД
//! (T35, FR-1203, INV-086, п. 86, 97).
//!
//! INV-086: пока за объект спорят несколько заявок, «предоставить» закрыто -
//! по категориям 4–5 вопрос уходит в общий порядок (п. 86), а по
//! инвестиционной категории приоритет у большей суммы (п. 97). Отказать
//! и направить в общий порядок Правление вправе всегда.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use rust_decimal::Decimal;
use sqlx::Acquire as _;
use tou_domain::special::{CompetitionRule, SpecialCategory, SpecialDecision};
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - конкуренция не проверялась");
                return;
            }
        }
    };
}

/// Отказ БД на savepoint'е. `fetch_all`, а не `execute`: проверенный макросом
/// запрос с `RETURNING` - это Map, у которого `execute` нет.
macro_rules! rejected {
    ($tx:expr, $query:expr, $why:expr) => {{
        let mut sp = $tx.begin().await.expect("savepoint");
        let error = $query.fetch_all(&mut *sp).await.expect_err($why);
        sp.rollback().await.expect("rollback savepoint");
        error.to_string()
    }};
}

/// Пользователь стенда (заявитель либо сотрудник подразделения).
async fn user(tx: &mut sqlx::PgConnection, tag: &str) -> Result<Uuid, sqlx::Error> {
    let unique = Uuid::now_v7().simple().to_string();
    sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т35 участник', now()) RETURNING id",
        format!("t35-{tag}-{unique}@tou.test")
    )
    .fetch_one(tx)
    .await
}

async fn object(tx: &mut sqlx::PgConnection) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Т35 помещение', 'адрес', 42.00) RETURNING id"
    )
    .fetch_one(tx)
    .await
}

/// Подача заявки на объект. Общий текст остался в одном месте, но теперь это
/// макрос, а не константа: проверяемому запросу нужен литерал.
macro_rules! request {
    ($applicant_id:expr, $category:expr, $object_id:expr, $amount:expr) => {
        sqlx::query_scalar!(
            "INSERT INTO core.special_requests
                (applicant_id, category, applicant_kind, applicant_details, purpose,
                 object_id, investment_amount)
             VALUES ($1, $2, 'legal_entity', '{}'::jsonb, 'использование помещения', $3, $4)
             RETURNING id",
            $applicant_id,
            $category,
            $object_id,
            $amount
        )
    };
}

/// Заявка с заключением подразделения (иначе решение невозможно, INV-090).
async fn reviewed_request(
    tx: &mut sqlx::PgConnection,
    category: SpecialCategory,
    object_id: Option<Uuid>,
    amount: Option<Decimal>,
) -> Result<Uuid, sqlx::Error> {
    let applicant = user(&mut *tx, "applicant").await?;
    let reviewer = user(&mut *tx, "reviewer").await?;

    let request_id = request!(applicant, category.as_str(), object_id, amount)
        .fetch_one(&mut *tx)
        .await?;

    sqlx::query!(
        "INSERT INTO core.special_reviews
           (special_request_id, reviewer_id, conclusion, recommendation)
         VALUES ($1, $2, 'соответствует требованиям категории', 'grant')",
        request_id,
        reviewer
    )
    .execute(&mut *tx)
    .await?;

    Ok(request_id)
}

/// Правило конкуренции категории правит только миграция (роль приложения
/// имеет на refdata лишь SELECT). Тест имитирует каталог, заполненный
/// инженером по Правилам: поднимается до владельца схемы внутри своей
/// транзакции (`SET LOCAL` откатывается вместе с ней).
async fn set_competition_rule(
    tx: &mut sqlx::PgConnection,
    category: SpecialCategory,
    rule: CompetitionRule,
) -> Result<(), sqlx::Error> {
    sqlx::query!("SET LOCAL ROLE NONE")
        .execute(&mut *tx)
        .await?;
    sqlx::query!(
        "UPDATE refdata.special_categories
         SET competition = $2::text::core.special_competition WHERE code = $1",
        category.as_str(),
        rule.as_str()
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!("SET LOCAL ROLE tou_rent_app")
        .execute(&mut *tx)
        .await?;
    Ok(())
}

/// Решение Правления по заявке.
macro_rules! decision {
    ($request_id:expr, $decision:expr, $decided_by:expr) => {
        sqlx::query!(
            "INSERT INTO core.special_board_decisions
                (special_request_id, decision, rationale, decided_by)
             VALUES ($1, $2::text::core.special_decision, 'обоснование решения', $3)",
            $request_id,
            $decision,
            $decided_by
        )
    };
}

/// INV-086 (п. 86): по категории с правилом общего порядка вторая заявка
/// на тот же объект закрывает «предоставить», но не отказ и не перевод.
#[tokio::test]
async fn inv086_competing_applications_close_the_grant() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let object_id = object(&mut tx).await.expect("объект");
    let board = user(&mut tx, "board").await.expect("Правление");

    // Категория 4 - правило общего порядка задано миграцией по ТЗ (FR-1203)
    let first = reviewed_request(&mut tx, SpecialCategory::Category4, Some(object_id), None)
        .await
        .expect("первая заявка");
    let _second = reviewed_request(&mut tx, SpecialCategory::Category4, Some(object_id), None)
        .await
        .expect("вторая заявка");

    let error = rejected!(
        tx,
        decision!(first, SpecialDecision::Grant.as_str(), board),
        "«предоставить» при конкуренции обязано быть отклонено"
    );
    assert!(error.contains("INV-086"), "{error}");

    // Перевод в общий порядок Правлению доступен
    decision!(first, SpecialDecision::Redirect.as_str(), board)
        .execute(&mut *tx)
        .await
        .expect("перевод в общий порядок");
}

/// Заявка на другой объект конкурентом не является: спорить не о чем.
#[tokio::test]
async fn requests_for_different_objects_do_not_compete() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let board = user(&mut tx, "board").await.expect("Правление");
    let first_object = object(&mut tx).await.expect("объект 1");
    let second_object = object(&mut tx).await.expect("объект 2");

    let first = reviewed_request(
        &mut tx,
        SpecialCategory::Category4,
        Some(first_object),
        None,
    )
    .await
    .expect("первая заявка");
    let _other = reviewed_request(
        &mut tx,
        SpecialCategory::Category4,
        Some(second_object),
        None,
    )
    .await
    .expect("заявка на другой объект");

    decision!(first, SpecialDecision::Grant.as_str(), board)
        .execute(&mut *tx)
        .await
        .expect("заявки на разные объекты не конкурируют");
}

/// INV-086 (п. 97): по инвестиционной категории приоритет у большей суммы.
#[tokio::test]
async fn inv086_higher_investment_wins() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let board = user(&mut tx, "board").await.expect("Правление");
    let object_id = object(&mut tx).await.expect("объект");

    // Инвестиционная категория Правилами не названа (Q-009): правило
    // ставится данными - так же, как его заполнит инженер
    set_competition_rule(
        &mut tx,
        SpecialCategory::Category7,
        CompetitionRule::HighestAmount,
    )
    .await
    .expect("правило категории");

    let modest = reviewed_request(
        &mut tx,
        SpecialCategory::Category7,
        Some(object_id),
        Some(Decimal::from(30_000_000)),
    )
    .await
    .expect("заявка с меньшей суммой");
    let generous = reviewed_request(
        &mut tx,
        SpecialCategory::Category7,
        Some(object_id),
        Some(Decimal::from(100_000_000)),
    )
    .await
    .expect("заявка с большей суммой");

    let error = rejected!(
        tx,
        decision!(modest, SpecialDecision::Grant.as_str(), board),
        "заявка с меньшей суммой не может быть удовлетворена"
    );
    assert!(error.contains("INV-086"), "{error}");

    decision!(generous, SpecialDecision::Grant.as_str(), board)
        .execute(&mut *tx)
        .await
        .expect("приоритет у большей суммы (п. 97)");
}

/// FR-1203 (п. 97): заявка инвестиционной категории подается с суммой.
#[tokio::test]
async fn investment_category_requires_an_amount() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let applicant = user(&mut tx, "applicant").await.expect("заявитель");
    let object_id = object(&mut tx).await.expect("объект");

    set_competition_rule(
        &mut tx,
        SpecialCategory::Category8,
        CompetitionRule::HighestAmount,
    )
    .await
    .expect("правило категории");

    let error = rejected!(
        tx,
        request!(
            applicant,
            SpecialCategory::Category8.as_str(),
            Some(object_id),
            Option::<Decimal>::None
        ),
        "заявка инвестиционной категории без суммы обязана быть отклонена"
    );
    assert!(error.contains("FR-1203"), "{error}");
}

/// Правила конкуренции БД и домена - один перечень (паритет enum, G16),
/// а категории 4–5 идут в общий порядок по самому ТЗ (FR-1203).
#[tokio::test]
async fn competition_rules_match_the_domain_and_the_spec() {
    let db = require_db!();

    let mut values = sqlx::query_scalar!(
        r#"SELECT unnest(enum_range(NULL::core.special_competition))::text AS "value!"
           ORDER BY 1"#
    )
    .fetch_all(&db)
    .await
    .expect("значения enum");
    values.sort();

    let mut domain: Vec<String> = CompetitionRule::ALL
        .iter()
        .map(|rule| rule.as_str().to_owned())
        .collect();
    domain.sort();
    assert_eq!(values, domain, "перечень правил совпадает с доменом");

    let redirecting = sqlx::query_scalar!(
        "SELECT ordinal FROM refdata.special_categories
         WHERE competition = 'redirect' ORDER BY ordinal"
    )
    .fetch_all(&db)
    .await
    .expect("категории общего порядка");
    assert_eq!(redirecting, vec![4, 5], "категории 4–5 (FR-1203, п. 86)");
}
