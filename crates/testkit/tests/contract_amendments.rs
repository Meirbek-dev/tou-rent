//! Допсоглашения к договору против живой БД (T42, FR-906, FR-901, п. 125).
//!
//! Проверяется то, что делает БД сама: допсоглашение заключается только
//! к зарегистрированному действующему договору, существенное условие им
//! не меняется (FR-901 - и через FK перечня, и явным отказом триггера),
//! правка неизменяема, а сам факт ложится в досье (FR-1602).
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use sqlx::Acquire as _;
use tou_domain::contract::ContractField;
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - допсоглашения не проверялись");
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
    // У проверенного макросом запроса с RETURNING есть выходные столбцы,
    // а значит нет `execute` - отказ ловится на выборке строки
    (returning $tx:expr, $query:expr, $why:expr) => {{
        let mut sp = $tx.begin().await.expect("savepoint");
        let error = $query.fetch_one(&mut *sp).await.expect_err($why);
        sp.rollback().await.expect("rollback savepoint");
        error.to_string()
    }};
}

struct Fixture {
    tender_id: Uuid,
    contract_id: Uuid,
    staff_id: Uuid,
}

/// Договор тендера: `registered` - зарегистрирован ли он (п. 126).
async fn fixture(tx: &mut sqlx::PgConnection, registered: bool) -> Result<Fixture, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let staff_id = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т42 организатор', now()) RETURNING id",
        format!("t42-staff-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let tenant_id = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т42 наниматель', now()) RETURNING id",
        format!("t42-tenant-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id, announced_at,
                                   submission_deadline, opening_at, opened_at)
         VALUES ('Т42 тендер', 'contracted', $1, now() - interval '60 days',
                 now() - interval '40 days', now() - interval '39 days',
                 now() - interval '39 days')
         RETURNING id",
        staff_id
    )
    .fetch_one(&mut *tx)
    .await?;

    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Т42 помещение', 'г. Павлодар, ул. Тестовая, 3', 40)
         RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await?;

    let contract_id = sqlx::query_scalar!(
        "INSERT INTO core.contracts
           (tender_id, object_id, tenant_id, monthly_rate, status, lease_period,
            reg_number, registered_at)
         VALUES ($1, $2, $3, 79750, 'active',
                 tstzrange(now() - interval '10 days', now() + interval '350 days'),
                 $4, CASE WHEN $5 THEN core.now() END)
         RETURNING id",
        tender_id,
        object_id,
        tenant_id,
        registered.then(|| format!("Д-{tag}")),
        // Отметка ставится сервером (`core.now()`, ADR-0005): фикстура на
        // часах процесса разошлась бы с тем, что проверяет тест
        registered
    )
    .fetch_one(&mut *tx)
    .await?;

    Ok(Fixture {
        tender_id,
        contract_id,
        staff_id,
    })
}

/// Допсоглашение: запрос один на все проверки, но теперь он еще и сверяется
/// со схемой - макросу нужен литерал, а не константа.
macro_rules! amend {
    ($contract:expr, $staff:expr) => {
        sqlx::query_scalar!(
            "INSERT INTO core.contract_amendments
                (contract_id, seq, ground, effective_on, created_by)
             VALUES ($1, 1, 'смена банковских реквизитов нанимателя', current_date, $2)
             RETURNING id",
            $contract,
            $staff
        )
    };
}

/// FR-906 (п. 126): допсоглашение заключается к зарегистрированному договору.
#[tokio::test]
async fn fr906_amendment_needs_a_registered_contract() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let draft = fixture(&mut tx, false)
        .await
        .expect("незарегистрированный договор");

    let error = rejected!(
        returning tx,
        amend!(draft.contract_id, draft.staff_id),
        "допсоглашение к незарегистрированному договору обязано быть отклонено"
    );
    assert!(error.contains("FR-906"), "{error}");

    let f = fixture(&mut tx, true)
        .await
        .expect("зарегистрированный договор");
    let id = amend!(f.contract_id, f.staff_id)
        .fetch_one(&mut *tx)
        .await
        .expect("допсоглашение");
    assert!(!id.is_nil());
}

/// FR-901 (п. 108, 125): существенное условие допсоглашением не меняется -
/// его нет в перечне (FK), и триггер отказывает по имени поля.
#[tokio::test]
async fn fr901_essential_terms_are_not_amendable() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, true).await.expect("договор");

    let amendment = amend!(f.contract_id, f.staff_id)
        .fetch_one(&mut *tx)
        .await
        .expect("допсоглашение");

    // Первый рубеж: существенного условия нет в перечне п. 125
    for field in ContractField::PROTECTED {
        let error = rejected!(
            tx,
            sqlx::query!(
                "INSERT INTO core.contract_amendment_changes
                   (amendment_id, field_code, old_value, new_value)
                 VALUES ($1, $2, '79750', '60000')",
                amendment,
                field.as_str()
            ),
            "правка существенного условия обязана быть отклонена"
        );
        assert!(
            error.contains("FR-901") || error.contains("amendable_fields"),
            "{}: {error}",
            field.as_str()
        );
    }

    // Разрешенное поле проходит
    sqlx::query!(
        "INSERT INTO core.contract_amendment_changes
           (amendment_id, field_code, old_value, new_value)
         VALUES ($1, 'bank_details', 'KZ11', 'KZ22')",
        amendment
    )
    .execute(&mut *tx)
    .await
    .expect("правка разрешенного поля");

    // Правка без изменения значения - не правка (FR-906)
    let same = rejected!(
        tx,
        sqlx::query!(
            "INSERT INTO core.contract_amendment_changes
               (amendment_id, field_code, old_value, new_value)
             VALUES ($1, 'representative', 'Иванов', 'Иванов')",
            amendment
        ),
        "правка без изменения значения обязана быть отклонена"
    );
    assert!(
        same.contains("amendment_change_is_a_change") || same.contains("violates check"),
        "{same}"
    );
}

/// FR-906 (п. 125): соглашение и его правки неизменяемы, удалить их нельзя.
#[tokio::test]
async fn fr906_amendment_and_its_changes_are_final() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, true).await.expect("договор");

    let amendment = amend!(f.contract_id, f.staff_id)
        .fetch_one(&mut *tx)
        .await
        .expect("допсоглашение");

    sqlx::query!(
        "INSERT INTO core.contract_amendment_changes
           (amendment_id, field_code, old_value, new_value)
         VALUES ($1, 'bank_details', 'KZ11', 'KZ22')",
        amendment
    )
    .execute(&mut *tx)
    .await
    .expect("правка");

    let rewritten = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.contract_amendments SET ground = 'другое' WHERE id = $1",
            amendment
        ),
        "правка основания обязана быть отклонена"
    );
    assert!(rewritten.contains("FR-906"), "{rewritten}");

    // Печатная форма догружается - это единственное изменение
    sqlx::query!(
        "UPDATE core.contract_amendments SET pdf_key = 'contracts/x/amendment-1.pdf' WHERE id = $1",
        amendment
    )
    .execute(&mut *tx)
    .await
    .expect("печатная форма");

    let changed = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.contract_amendment_changes SET new_value = 'KZ33'
             WHERE amendment_id = $1",
            amendment
        ),
        "правка изменения обязана быть отклонена"
    );
    assert!(
        changed.contains("FR-906") || changed.contains("permission denied"),
        "{changed}"
    );

    let removed = rejected!(
        tx,
        sqlx::query!(
            "DELETE FROM core.contract_amendments WHERE id = $1",
            amendment
        ),
        "удаление допсоглашения обязано быть отклонено"
    );
    assert!(
        removed.contains("FR-906") || removed.contains("permission denied"),
        "{removed}"
    );
}

/// FR-1602: допсоглашение ложится в досье тендера вместе с печатной формой.
#[tokio::test]
async fn amendment_lands_in_the_dossier() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, true).await.expect("договор");

    let amendment = amend!(f.contract_id, f.staff_id)
        .fetch_one(&mut *tx)
        .await
        .expect("допсоглашение");

    sqlx::query!(
        "UPDATE core.contract_amendments SET pdf_key = $2 WHERE id = $1",
        amendment,
        "contracts/x/amendment-1.pdf"
    )
    .execute(&mut *tx)
    .await
    .expect("печатная форма");

    let dossier = sqlx::query!(
        r#"SELECT count(*) AS "count!", min(title) AS title, min(file_key) AS file_key
           FROM core.dossier_items
           WHERE tender_id = $1 AND kind = 'amendment'"#,
        f.tender_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("материал досье");

    assert_eq!(dossier.count, 1, "досье собирается идемпотентно");
    assert!(
        dossier
            .title
            .unwrap_or_default()
            .contains("Допсоглашение №1"),
        "материал подписан номером соглашения"
    );
    assert!(
        dossier.file_key.is_some(),
        "в досье лежит ссылка на печатную форму"
    );
}

/// Паритет перечня изменяемых полей с доменом (FR-906, п. 125).
#[tokio::test]
async fn amendable_fields_match_the_domain() {
    let db = require_db!();

    let mut from_db = sqlx::query_scalar!("SELECT code FROM refdata.amendable_fields")
        .fetch_all(&db)
        .await
        .expect("перечень изменяемых полей");
    from_db.sort();

    let mut from_domain: Vec<String> = ContractField::amendable()
        .into_iter()
        .map(|field| field.as_str().to_owned())
        .collect();
    from_domain.sort();

    assert_eq!(
        from_db, from_domain,
        "перечень изменяемых полей совпадает в БД и домене"
    );
    for code in &from_db {
        let field: ContractField = code.parse().expect("поле домена");
        assert!(!field.is_protected(), "{code} не является существенным");
    }
}
