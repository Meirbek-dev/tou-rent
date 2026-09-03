//! Регистрация договора заканчивает процедуру тендера (G-155; FR-302,
//! FR-905, INV-021, п. 108, 126).
//!
//! Круг 7 гаунтлета. Переход `summed_up → contracted` стоял в
//! `refdata.tender_status_transitions`, но выполнял его один только seed
//! демо-стенда: живой тендер доходил до договора, договор ложился в журнал
//! регистрации - и тендер навсегда оставался «итоги подведены». Бейдж
//! карточки портала говорил неправду, а `contracted` не появлялся ни у
//! одной процедуры, пройденной приложением.
//!
//! Проверяется то, что делает слой данных против живой БД: регистрация
//! переводит тендер, след остается в аудите, а тендер не в статусе
//! «итоги подведены» тем же действием не трогается.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021). Транзакция откатывается:
//! `register_on` - вариант в транзакции вызывающего, и убирать за собой
//! процедурой очистки стенда не приходится.

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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - заключение договора не проверялось");
                return;
            }
        }
    };
}

struct Fixture {
    actor: Uuid,
    tender: Uuid,
    contract: Uuid,
}

/// Тендер в заданном статусе и договор победителя, подписанный обеими
/// сторонами: остается регистрация в журнале (FR-905, п. 126).
///
/// Статус тендера выставляется при вставке - переходы сторожит триггер
/// INV-021, а тесту нужна не цепочка переходов, а состояние. Подписи
/// проставляются отдельным UPDATE: подпись наймодателя невозможна без
/// завершенной сверки (INV-115, п. 113), а перечень сверки ссылается на
/// уже существующий договор.
async fn fixture(tx: &mut sqlx::PgConnection, status: &str) -> Result<Fixture, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let actor = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'argon2-заглушка', 'G155 организатор', core.now()) RETURNING id",
        format!("g155-organizer-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let tenant = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'argon2-заглушка', 'G155 наниматель', core.now()) RETURNING id",
        format!("g155-tenant-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    // Актор транзакции: события аудита пишутся под ним
    tou_db::set_actor(&mut *tx, actor).await?;

    let object = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', $1, 'адрес', 12.00) RETURNING id",
        format!("G155 объект {tag}")
    )
    .fetch_one(&mut *tx)
    .await?;

    let tender = sqlx::query_scalar!(
        "INSERT INTO core.tenders
           (title, status, organizer_id, announced_at, submission_deadline,
            opening_at, opened_at, trading_at)
         VALUES ($1, $2::text::core.tender_status, $3,
                 core.now() - interval '12 days', core.now() - interval '2 hours',
                 core.now() - interval '1 hour', core.now() - interval '1 hour',
                 core.now() - interval '30 minutes')
         RETURNING id",
        format!("G155 тендер {tag}"),
        status,
        actor
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

    let contract = sqlx::query_scalar!(
        "INSERT INTO core.contracts
           (tender_id, lot_id, object_id, tenant_id, status, monthly_rate, lease_months,
            drafted_at, handed_to_tenant_at, tenant_signed_at, documents_received_at)
         VALUES ($1, $2, $3, $4, 'draft', 60500.00, 12,
                 core.now(), core.now(), core.now(), core.now())
         RETURNING id",
        tender,
        lot,
        object,
        tenant
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query!(
        "INSERT INTO core.contract_checklists (contract_id, item_code, checked_by, checked_at)
         VALUES ($1, 'identity', $2, core.now())",
        contract,
        actor
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        "UPDATE core.contracts
         SET checklist_done_at = core.now(), landlord_signed_at = core.now(),
             copy_sent_at = core.now()
         WHERE id = $1",
        contract
    )
    .execute(&mut *tx)
    .await?;

    Ok(Fixture {
        actor,
        tender,
        contract,
    })
}

async fn status_of(tx: &mut sqlx::PgConnection, tender: Uuid) -> Result<String, sqlx::Error> {
    // `!` у status - это `::text`, который планировщик считает потенциально NULL
    sqlx::query_scalar!(
        r#"SELECT status::text AS "status!" FROM core.tenders WHERE id = $1"#,
        tender
    )
    .fetch_one(&mut *tx)
    .await
}

/// FR-302, FR-905, INV-021, п. 108, 126: запись договора в журнал
/// регистрации - это дата заключения, а значит и конец процедуры тендера.
#[tokio::test]
async fn registering_the_winners_contract_closes_the_tender() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, "summed_up").await.expect("фикстура");

    assert_eq!(
        status_of(&mut tx, f.tender).await.expect("статус тендера"),
        "summed_up"
    );

    let number = format!("Д-g155-{}", Uuid::now_v7().simple());
    let registered = tou_db::contracts::register_on(&mut tx, f.actor, f.contract, &number)
        .await
        .expect("регистрация договора");
    assert_eq!(registered.reg_number.as_deref(), Some(number.as_str()));
    assert_eq!(
        status_of(&mut tx, f.tender).await.expect("статус тендера"),
        "contracted",
        "регистрация договора заканчивает процедуру тендера (FR-905, п. 108, 126)"
    );

    // След перехода - событие аудита по строке тендера под актором (А.5)
    let audited = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM audit.log
           WHERE table_name = 'core.tenders' AND row_id = $1 AND action = 'UPDATE'
             AND actor_id = $2 AND payload -> 'new' ->> 'status' = 'contracted'"#,
        f.tender,
        f.actor
    )
    .fetch_one(&mut *tx)
    .await
    .expect("события аудита");
    assert_eq!(audited, 1, "заключение договора записано в аудит");

    tx.rollback().await.expect("rollback");
}

/// INV-021, FR-305, п. 78: тендер не в статусе «итоги подведены»
/// регистрация договора не трогает - переход идет условием запроса,
/// а не проверкой в коде. Отмененный тендер договором не оживает.
#[tokio::test]
async fn a_cancelled_tender_is_not_revived_by_a_contract() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, "cancelled").await.expect("фикстура");

    tou_db::contracts::register_on(
        &mut tx,
        f.actor,
        f.contract,
        &format!("Д-g155c-{}", Uuid::now_v7().simple()),
    )
    .await
    .expect("регистрация договора");
    assert_eq!(
        status_of(&mut tx, f.tender).await.expect("статус тендера"),
        "cancelled",
        "договор отмененного тендера статус не меняет (INV-021, FR-305)"
    );

    tx.rollback().await.expect("rollback");
}
