//! Рабочие списки, по которым идет работа отделов (круг 1 гаунтлета:
//! G-125, G-134, G-140), - против живой БД.
//!
//! Общее у всех трех - одно: список существует ради того, чтобы по строке
//! опознать предмет и взяться за него. Очередь без предмета - не список,
//! а счетчик: финблок не находил заявку, по которой пришли деньги
//! (G-125); подразделение публикаций видело несколько строк с одинаковым
//! заголовком (G-134); админ выбирал запись на удаление из строк,
//! различавшихся только временем (G-140).
//!
//! Проверки идут в откатываемой транзакции: выборки читают исполнителя
//! вызывающего, поэтому собственная запись теста им видна, а стенду
//! ничего не остается.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use rust_decimal::Decimal;
use tou_db::purge::PurgeScope;
use tou_domain::special::SpecialCategory;
use uuid::Uuid;

async fn try_pool() -> Result<Option<tou_db::Db>, sqlx::Error> {
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - рабочие списки не проверялись");
                return;
            }
        }
    };
}

/// Взнос лота фикстуры (FR-206): с ним сверяется поступление.
const GUARANTEE_FEE: i64 = 36_000;

/// Учетная запись под тег прогона; имя намеренно «учетное», а не то, что
/// стоит в форме заявки.
async fn user(tx: &mut sqlx::PgConnection, role: &str, tag: &str) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', $2, core.now()) RETURNING id",
        format!("queues-{role}-{tag}@tou.test"),
        format!("Учетная запись {role}")
    )
    .fetch_one(tx)
    .await
}

struct Fixture {
    tender_id: Uuid,
    tender_title: String,
    lot_id: Uuid,
    application_id: Uuid,
    /// Имя из формы Прил. 2 - не совпадает с именем учетной записи
    applicant_name: String,
    finance_id: Uuid,
}

/// Тендер с лотом и поданной заявкой, взнос по которой не подтвержден.
///
/// Имя в форме заявки намеренно другое, чем в учетной записи: очередь
/// финблока и перечень админа обязаны показывать то имя, которое стоит
/// в заявке и в выписке банка, а не имя владельца учетки.
async fn fixture(tx: &mut sqlx::PgConnection) -> Result<Fixture, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();
    let title = format!("G-125 тендер {tag}");
    let applicant_name = format!("ТОО «Заявитель {tag}»");

    let organizer = user(&mut *tx, "org", &tag).await?;
    let participant = user(&mut *tx, "participant", &tag).await?;
    let finance_id = user(&mut *tx, "finance", &tag).await?;

    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders
           (title, status, organizer_id, announced_at, submission_deadline, opening_at)
         VALUES ($1, 'accepting', $2, core.now() - interval '2 days',
                 core.now() + interval '12 days', core.now() + interval '13 days')
         RETURNING id",
        title,
        organizer
    )
    .fetch_one(&mut *tx)
    .await?;

    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', $1, 'адрес', 42.00) RETURNING id",
        format!("Аудитория {tag}")
    )
    .fetch_one(&mut *tx)
    .await?;

    let lot_id = sqlx::query_scalar!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                base_rate_monthly, guarantee_fee, rate_calculation)
         VALUES ($1, 1, $2, 'офис', 12, $3, $3, '{}'::jsonb) RETURNING id",
        tender_id,
        object_id,
        Decimal::from(GUARANTEE_FEE)
    )
    .fetch_one(&mut *tx)
    .await?;

    let details = serde_json::json!({ "name": applicant_name, "bin": "000000000000" });
    let application_id = sqlx::query_scalar!(
        "INSERT INTO core.applications
           (tender_id, lot_id, participant_id, applicant_kind, applicant_details,
            package_required)
         VALUES ($1, $2, $3, 'legal_entity', $4, false) RETURNING id",
        tender_id,
        lot_id,
        participant,
        details
    )
    .fetch_one(&mut *tx)
    .await?;

    Ok(Fixture {
        tender_id,
        tender_title: title,
        lot_id,
        application_id,
        applicant_name,
        finance_id,
    })
}

/// G-125: поданная заявка с неподтвержденным взносом стоит в очереди
/// финблока со всем, чем сверяется выписка банка, и уходит из нее, как
/// только поступление подтверждено.
#[tokio::test]
async fn the_fee_queue_shows_what_the_bank_statement_is_matched_against() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("fixture");

    let page = tou_db::ledger::pending_fees(&mut *tx, None, 100)
        .await
        .expect("очередь взносов");
    let row = page
        .iter()
        .find(|row| row.application_id == f.application_id)
        .expect("поданная заявка ждет подтверждения взноса");

    assert_eq!(row.tender_id, f.tender_id);
    assert_eq!(row.tender_title, f.tender_title);
    assert_eq!(row.lot_id, f.lot_id);
    assert_eq!(row.lot_seq, 1);
    assert_eq!(
        row.applicant_name, f.applicant_name,
        "в очереди стоит имя из формы Прил. 2, а не имя владельца учетки"
    );
    assert_eq!(
        row.guarantee_fee,
        Decimal::from(GUARANTEE_FEE),
        "размер взноса задает лот (FR-206) - с ним и сверяется поступление"
    );

    let today =
        sqlx::query_scalar!(r#"SELECT (core.now() AT TIME ZONE 'Asia/Almaty')::date AS "today!""#)
            .fetch_one(&mut *tx)
            .await
            .expect("деловая дата");

    tou_db::ledger::confirm_fee_on(
        &mut tx,
        f.finance_id,
        f.application_id,
        Decimal::from(GUARANTEE_FEE),
        today,
    )
    .await
    .expect("взнос подтвержден");

    let page = tou_db::ledger::pending_fees(&mut *tx, None, 100)
        .await
        .expect("очередь взносов");
    assert!(
        !page
            .iter()
            .any(|row| row.application_id == f.application_id),
        "подтвержденная заявка из очереди уходит - список разгружается работой"
    );
}

/// G-140: заявку в перечне на удаление опознает имя из формы, а подпись
/// называет предмет - тендер, лот и объект.
#[tokio::test]
async fn the_admin_record_list_identifies_an_application() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("fixture");

    let page = tou_db::purge::list_records(&mut *tx, PurgeScope::Applications)
        .await
        .expect("перечень заявок");
    let row = page
        .iter()
        .find(|row| row.id == f.application_id)
        .expect("заявка в перечне");

    assert_eq!(
        row.title, f.applicant_name,
        "заголовок - имя заявителя из формы, а не имя владельца учетной записи"
    );
    let details = row.details.clone().expect("подпись записи");
    assert!(
        details.contains(&f.tender_title) && details.contains("лот №1"),
        "подпись обязана называть предмет заявки: {details}"
    );
}

/// G-140: заявка особого порядка опознается заявителем, объектом,
/// категорией и датой подачи - раньше в перечне стояло имя владельца
/// учетной записи, и заявки одного заявителя различались только временем.
#[tokio::test]
async fn the_admin_record_list_identifies_a_special_request() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let tag = Uuid::now_v7().simple().to_string();
    let applicant_name = format!("ТОО «Особый {tag}»");

    let applicant = user(&mut tx, "special", &tag).await.expect("заявитель");

    let object_name = format!("Площадка {tag}");
    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', $1, 'адрес', 42.00) RETURNING id",
        object_name
    )
    .fetch_one(&mut *tx)
    .await
    .expect("объект");

    let category = sqlx::query!(
        "SELECT label_ru FROM refdata.special_categories WHERE code = $1",
        SpecialCategory::Category7.as_str()
    )
    .fetch_one(&mut *tx)
    .await
    .expect("категория Правил п. 87");

    let details = serde_json::json!({ "name": applicant_name });
    let request_id = sqlx::query_scalar!(
        "INSERT INTO core.special_requests
           (applicant_id, category, applicant_details, applicant_kind, object_id, purpose,
            investment_amount)
         VALUES ($1, $2, $3, 'legal_entity', $4, 'научная лаборатория', 1000000.00)
         RETURNING id",
        applicant,
        SpecialCategory::Category7.as_str(),
        details,
        object_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("заявка особого порядка");

    let page = tou_db::purge::list_records(&mut *tx, PurgeScope::SpecialRequests)
        .await
        .expect("перечень заявок особого порядка");
    let row = page
        .iter()
        .find(|row| row.id == request_id)
        .expect("заявка в перечне");

    assert_eq!(row.title, applicant_name, "заголовок - имя из формы заявки");
    let details = row.details.clone().expect("подпись записи");
    assert!(
        details.contains(&object_name)
            && details.contains(&category.label_ru)
            && details.contains("подана "),
        "подпись обязана назвать объект, категорию и дату подачи: {details}"
    );
}

/// G-134: очередь публикаций называет предмет материала.
///
/// Заголовок «Обоснование ставки договора особого порядка» одинаков у всех
/// таких материалов: без объекта, нанимателя и номера договора очередь из
/// двух строк не давала выбрать, какую публиковать.
#[tokio::test]
async fn the_publication_queue_names_its_subject() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let tag = Uuid::now_v7().simple().to_string();

    let reviewer = user(&mut tx, "reviewer", &tag).await.expect("сотрудник");
    let tenant_name = format!("ТОО «Наниматель {tag}»");
    let tenant = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', $2, core.now()) RETURNING id",
        format!("queues-pub-tenant-{tag}@tou.test"),
        tenant_name
    )
    .fetch_one(&mut *tx)
    .await
    .expect("наниматель");

    let object_name = format!("Корпус {tag}");
    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', $1, 'адрес', 42.00) RETURNING id",
        object_name
    )
    .fetch_one(&mut *tx)
    .await
    .expect("объект");

    // Путь заявки к «предоставить»: заключение подразделения → решение
    // Правления (INV-090). Инвестиционный договор заключается только по
    // удовлетворенной заявке (FR-1204)
    let request_id = sqlx::query_scalar!(
        "INSERT INTO core.special_requests
           (applicant_id, category, applicant_kind, applicant_details, purpose,
            object_id, investment_amount)
         VALUES ($1, $2, 'legal_entity', '{}'::jsonb, 'инвестиционный проект', $3, 1000000.00)
         RETURNING id",
        tenant,
        SpecialCategory::Category7.as_str(),
        object_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("заявка особого порядка");

    sqlx::query!(
        "INSERT INTO core.special_reviews
           (special_request_id, reviewer_id, conclusion, recommendation)
         VALUES ($1, $2, 'проект соответствует требованиям', 'grant')",
        request_id,
        reviewer
    )
    .execute(&mut *tx)
    .await
    .expect("заключение подразделения");
    sqlx::query!(
        "INSERT INTO core.special_board_decisions
           (special_request_id, decision, rationale, decided_by)
         VALUES ($1, 'grant', 'проект принят', $2)",
        request_id,
        reviewer
    )
    .execute(&mut *tx)
    .await
    .expect("решение Правления");

    // Номер журнала есть, регистрации еще нет: `check_contract_registration`
    // требует подписей только при `registered_at`, а номер - примета
    // материала в очереди
    let reg_number = format!("Д-G134-{tag}");
    let contract_id = sqlx::query_scalar!(
        "INSERT INTO core.contracts (object_id, tenant_id, monthly_rate, reg_number)
         VALUES ($1, $2, 60500.00, $3) RETURNING id",
        object_id,
        tenant,
        reg_number
    )
    .fetch_one(&mut *tx)
    .await
    .expect("договор");

    sqlx::query!(
        "INSERT INTO core.investment_contracts
           (contract_id, special_request_id, investment_amount, term_months)
         VALUES ($1, $2, 1000000.00, 36)",
        contract_id,
        request_id
    )
    .execute(&mut *tx)
    .await
    .expect("инвестиционный договор");

    let page = tou_db::public_records::pending(&mut *tx, None, 1000)
        .await
        .expect("очередь публикаций");
    let row = page
        .iter()
        .find(|row| row.source_id == contract_id)
        .expect("обоснование ставки ждет публикации");

    assert_eq!(
        row.title, "Обоснование ставки договора особого порядка",
        "заголовок остается тем, что уйдет в публикацию (п. 97)"
    );
    assert_eq!(
        row.object_name.as_deref(),
        Some(object_name.as_str()),
        "предмет материала - объект договора"
    );
    assert_eq!(
        row.party_name.as_deref(),
        Some(tenant_name.as_str()),
        "вторая примета - наниматель"
    );
    assert_eq!(
        row.reference.as_deref(),
        Some(reg_number.as_str()),
        "номер договора называет материал однозначно"
    );
}
