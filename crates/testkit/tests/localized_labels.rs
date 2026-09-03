//! Подписи справочников доезжают на трех языках (NFR-01, G-066, G-075).
//!
//! Часть подписей уходила с сервера готовой русской строкой: категория
//! особого порядка в карточке заявки и в реестре решений, назначение
//! земельного участка на портале, заголовок публикации. Справочники при этом
//! хранят `label_kk` и `label_en` - перевод существовал, но до кабинета
//! не доезжал, и на /kk/ и /en/ эти поля читались по-русски.
//!
//! Против живой БД (A-021): подписи приходят соединениями со справочником,
//! и проверить их можно только там, где эти справочники есть.

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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - подписи не проверялись");
                return;
            }
        }
    };
}

/// Категория без конкурса (п. 87.1) и публикуемая: на ней проходят и
/// решение Правления, и публикация результата.
const CATEGORY: &str = "category_1";
/// Пункт Правил этой категории - он входит в подпись предмета реестра.
const CATEGORY_RULE_REF: &str = "п. 87.1";

struct CategoryLabels {
    ru: String,
    kk: String,
    en: String,
}

async fn category_labels(db: &tou_db::Db) -> Result<CategoryLabels, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT label_ru, label_kk, label_en FROM refdata.special_categories WHERE code = $1",
        CATEGORY
    )
    .fetch_one(db)
    .await?;
    Ok(CategoryLabels {
        ru: row.label_ru,
        kk: row.label_kk.unwrap_or_default(),
        en: row.label_en.unwrap_or_default(),
    })
}

async fn user(db: &tou_db::Db, tag: &str) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'G-066 участник процедуры', core.now()) RETURNING id",
        format!("g066-{tag}-{}@tou.test", Uuid::now_v7().simple())
    )
    .fetch_one(db)
    .await
}

async fn special_request(db: &tou_db::Db, applicant: Uuid) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO core.special_requests
           (applicant_id, category, applicant_kind, purpose, applicant_details)
         VALUES ($1, $2, 'legal_entity', 'G-066 размещение пункта питания', '{}'::jsonb)
         RETURNING id",
        applicant,
        CATEGORY
    )
    .fetch_one(db)
    .await
}

/// Рубеж G-066: карточка заявки особого порядка несет подпись категории
/// на всех трех языках, а не только по-русски.
#[tokio::test]
async fn a_special_request_carries_its_category_in_three_languages() {
    let db = require_db!();
    let labels = category_labels(&db).await.expect("подписи категории");
    let applicant = user(&db, "applicant").await.expect("заявитель");
    let request = special_request(&db, applicant).await.expect("заявка");

    let record = tou_db::special::get(&db, request)
        .await
        .expect("чтение заявки")
        .expect("заявка на месте");

    assert_eq!(record.category_label, labels.ru);
    assert_eq!(
        record.category_label_kk.as_deref(),
        Some(labels.kk.as_str()),
        "казахская подпись категории лежит в справочнике и обязана доехать"
    );
    assert_eq!(
        record.category_label_en.as_deref(),
        Some(labels.en.as_str()),
        "английская подпись категории лежит в справочнике и обязана доехать"
    );

    let _ = tou_db::purge::purge(
        &db,
        applicant,
        tou_db::purge::PurgeScope::SpecialRequests,
        Some(&[request]),
    )
    .await;
    let _ = sqlx::query!("DELETE FROM core.users WHERE id = $1", applicant)
        .execute(&db)
        .await;
}

/// Рубеж G-066: заголовок опубликованного материала и строка реестра
/// решений говорят на трех языках.
///
/// Заголовок публикации собирается в момент публикации и с этого мгновения
/// хранится (досье показывает опубликованное, а не текущее состояние
/// справочника) - поэтому переводы обязаны сохраняться вместе с русским,
/// иначе взять их потом будет неоткуда.
#[tokio::test]
async fn a_published_decision_and_the_registry_row_speak_three_languages() {
    let db = require_db!();
    let labels = category_labels(&db).await.expect("подписи категории");
    let applicant = user(&db, "publish-applicant").await.expect("заявитель");
    let board = user(&db, "publish-board").await.expect("член правления");
    let request = special_request(&db, applicant).await.expect("заявка");

    // Решения Правления без заключения подразделения не бывает (INV-090,
    // п. 89–90) - порядок здесь тот же, что и в процедуре
    sqlx::query!(
        "INSERT INTO core.special_reviews
           (special_request_id, reviewer_id, conclusion, recommendation)
         VALUES ($1, $2, 'G-066: имущество занято учебным процессом', 'refuse')",
        request,
        board
    )
    .execute(&db)
    .await
    .expect("заключение подразделения");

    let decision = sqlx::query_scalar!(
        "INSERT INTO core.special_board_decisions
           (special_request_id, decision, rationale, decided_by, pdf_key)
         VALUES ($1, 'refuse', 'G-066: имущество занято учебным процессом', $2,
                 'protocols/g066.pdf')
         RETURNING id",
        request,
        board
    )
    .fetch_one(&db)
    .await
    .expect("решение Правления");

    let published = tou_db::public_records::publish(
        &db,
        board,
        tou_db::public_records::NewPublicRecord {
            kind: tou_domain::publication::PublicRecordKind::Decision,
            special_request_id: Some(request),
            contract_id: None,
            acceptance_id: None,
            title: "Результат рассмотрения: ru",
            title_kk: Some("Қарау нәтижесі: kk"),
            title_en: Some("Decision: en"),
            file_key: Some("protocols/g066.pdf"),
            payload: serde_json::json!({}),
        },
    )
    .await
    .expect("публикация результата");

    let stored = tou_db::public_records::get(&db, published.id)
        .await
        .expect("чтение публикации")
        .expect("публикация на месте");
    assert_eq!(stored.title, "Результат рассмотрения: ru");
    assert_eq!(
        stored.title_kk.as_deref(),
        Some("Қарау нәтижесі: kk"),
        "казахский заголовок обязан храниться вместе с русским"
    );
    assert_eq!(stored.title_en.as_deref(), Some("Decision: en"));

    // Реестр решений (арх. § 9): ячейка «Предмет» - подпись категории
    let mut conn = db.acquire().await.expect("соединение");
    let page =
        tou_db::reports::decisions_page(&mut conn, tou_db::reports::Period::default(), None, 200)
            .await
            .expect("реестр решений");
    let row = page
        .iter()
        .find(|row| row.id == decision)
        .expect("наше решение в реестре");
    assert_eq!(row.subject, format!("{} ({CATEGORY_RULE_REF})", labels.ru));
    assert_eq!(
        row.subject_kk,
        Some(format!("{} ({CATEGORY_RULE_REF})", labels.kk)),
        "предмет реестра на казахском собирается из той же строки справочника"
    );
    assert_eq!(
        row.subject_en,
        Some(format!("{} ({CATEGORY_RULE_REF})", labels.en))
    );
    drop(conn);

    let _ = tou_db::purge::purge(
        &db,
        board,
        tou_db::purge::PurgeScope::PublicRecords,
        Some(&[published.id]),
    )
    .await;
    let _ = tou_db::purge::purge(
        &db,
        board,
        tou_db::purge::PurgeScope::SpecialRequests,
        Some(&[request]),
    )
    .await;
    let _ = sqlx::query!(
        "DELETE FROM core.users WHERE id = ANY($1)",
        &[applicant, board][..]
    )
    .execute(&db)
    .await;
}

/// Рубеж G-075: назначение земельного участка на портале приходит
/// с переводами из `refdata.land_designations`.
#[tokio::test]
async fn a_land_plot_carries_its_designation_in_three_languages() {
    let db = require_db!();
    let labels = sqlx::query!(
        "SELECT label_ru, label_kk, label_en
         FROM refdata.land_designations WHERE code = 'dormitory'"
    )
    .fetch_one(&db)
    .await
    .expect("подписи назначения");

    let object = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('land_plot', 'G-075 участок под общежитие',
                 'г. Павлодар, ул. Ломова, 64', 1200.00)
         RETURNING id"
    )
    .fetch_one(&db)
    .await
    .expect("объект-участок");

    sqlx::query!(
        "INSERT INTO core.land_plots
           (object_id, cadastral_number, designation, permitted_use)
         VALUES ($1, '14-215-075-000', 'dormitory', 'строительство общежития')",
        object
    )
    .execute(&db)
    .await
    .expect("характеристики участка");

    let plot = tou_db::land::get_plot(&db, object)
        .await
        .expect("чтение участка")
        .expect("участок на месте");

    assert_eq!(plot.designation_label, labels.label_ru);
    assert_eq!(
        plot.designation_label_kk, labels.label_kk,
        "казахская подпись назначения лежит в справочнике и обязана доехать"
    );
    assert_eq!(plot.designation_label_en, labels.label_en);

    let _ = sqlx::query!("DELETE FROM core.land_plots WHERE object_id = $1", object)
        .execute(&db)
        .await;
    let _ = sqlx::query!("DELETE FROM core.objects WHERE id = $1", object)
        .execute(&db)
        .await;
}
