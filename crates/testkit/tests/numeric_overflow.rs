//! AC-1: число, не помещающееся в колонку, - ошибка клиента, а не сервера.
//!
//! `POST /objects` с `"area_m2":"100000000.00"`, `POST /special-requests`
//! с `investment_amount: 999999999999999.99` и подача заявки с такой же
//! ценой отвечали 500 `internal`: SQLSTATE 22003 (`numeric field overflow`)
//! доходил до обработчика обычной ошибкой БД. Маршруты открыты рядовому
//! пользователю, и лишний ноль в поле площади выглядел поломкой сервиса.
//!
//! Проверяется весь класс, а не три случая: переполнение вызывается на самих
//! колонках трех маршрутов, а результат прогоняется через ту же воронку
//! `ApiError`, что и в обработчике. Приведение к типу колонки PostgreSQL
//! делает до проверок FK и enum, поэтому фикстуры не нужны - несуществующий
//! идентификатор до ограничений не доходит.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021). Каждая вставка живет в
//! транзакции с откатом: стендовая база не засоряется.

use tou_http::error::ApiError;

async fn try_pool() -> Result<Option<tou_db::Db>, sqlx::Error> {
    match tou_testkit::database_url().map_err(|e| sqlx::Error::Configuration(Box::new(e)))? {
        Some(url) => tou_db::connect(&url).await.map(Some),
        None => Ok(None),
    }
}

/// Отказ обязан быть машинно объяснимым, а не «внутренней ошибкой».
fn assert_is_client_error(column: &str, err: sqlx::Error) {
    let shown = format!("{err:?}");
    let code = match &err {
        sqlx::Error::Database(db) => db.code().map(|c| c.into_owned()).unwrap_or_default(),
        _ => String::new(),
    };
    assert_eq!(
        code, "22003",
        "{column}: ожидалось переполнение numeric, пришло {shown}"
    );

    // Та же воронка, что и в обработчике: `?` на `sqlx::Error` приводит его
    // к `ApiError`, и до правки это был `internal` (500)
    let detail = match ApiError::from(err) {
        ApiError::Validation(detail) => detail,
        other => format!("{other:?}"),
    };
    assert_eq!(
        detail, "value_out_of_range",
        "{column}: переполнение ушло наружу не как отказ проверки"
    );
}

#[tokio::test]
async fn numeric_overflow_is_refused_as_a_client_error() {
    let Some(db) = try_pool()
        .await
        .expect("TESTKIT_DATABASE_URL: подключение не удалось")
    else {
        eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - переполнение numeric не проверялось");
        return;
    };

    // core.objects.area_m2 - numeric(10,2), маршрут POST /objects
    let mut tx = db.begin().await.expect("транзакция");
    let err = sqlx::query!(
        "INSERT INTO core.objects (kind, name, name_kk, address, address_kk, area_m2)
         VALUES ('premises', 'AC-1', 'AC-1', 'AC-1', 'AC-1', 100000000.00)"
    )
    .execute(&mut *tx)
    .await
    .expect_err("площадь сверх numeric(10,2) обязана быть отвергнута");
    tx.rollback().await.expect("откат");
    assert_is_client_error("core.objects.area_m2", err);

    // core.special_requests.investment_amount - numeric(14,2),
    // маршрут POST /special-requests
    let mut tx = db.begin().await.expect("транзакция");
    let err = sqlx::query!(
        "INSERT INTO core.special_requests
           (applicant_id, category, applicant_kind, purpose, investment_amount)
         VALUES ('00000000-0000-0000-0000-000000000000', 'investment', 'individual',
                 'AC-1', 999999999999999.99)"
    )
    .execute(&mut *tx)
    .await
    .expect_err("сумма инвестиций сверх numeric(14,2) обязана быть отвергнута");
    tx.rollback().await.expect("откат");
    assert_is_client_error("core.special_requests.investment_amount", err);

    // core.price_proposals.amount - numeric(14,2),
    // маршрут POST /tenders/{id}/applications
    let mut tx = db.begin().await.expect("транзакция");
    let err = sqlx::query!(
        "INSERT INTO core.price_proposals (application_id, amount)
         VALUES ('00000000-0000-0000-0000-000000000000', 999999999999999.99)"
    )
    .execute(&mut *tx)
    .await
    .expect_err("цена сверх numeric(14,2) обязана быть отвергнута");
    tx.rollback().await.expect("откат");
    assert_is_client_error("core.price_proposals.amount", err);
}
