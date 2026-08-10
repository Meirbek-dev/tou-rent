//! Каркас особого порядка против живой БД (T33, FR-1201, INV-087, п. 87–88).
//!
//! Категория заявки - закрытый перечень п. 87: справочник заполнен ровно
//! тринадцатью позициями, их коды совпадают с enum домена, а FK не дает
//! завести заявку по выдуманной категории (INV-087). Порядок состояний
//! заявки и принадлежность документа категории стерегут триггеры.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use sqlx::Acquire as _;
use tou_domain::special::{SpecialCategory, SpecialRequestStatus};
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - особый порядок не проверялся");
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

/// Заявитель для заявки особого порядка.
async fn applicant(tx: &mut sqlx::PgConnection) -> Result<Uuid, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();
    sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т33 заявитель', now()) RETURNING id",
        format!("t33-applicant-{tag}@tou.test")
    )
    .fetch_one(tx)
    .await
}

/// Подача заявки. Общий текст остался в одном месте, но теперь это макрос,
/// а не константа: проверяемому запросу нужен литерал.
macro_rules! request {
    ($applicant_id:expr, $category:expr) => {
        sqlx::query_scalar!(
            "INSERT INTO core.special_requests
                (applicant_id, category, applicant_kind, applicant_details, purpose)
             VALUES ($1, $2, 'legal_entity', '{}'::jsonb, 'размещение оборудования')
             RETURNING id",
            $applicant_id,
            $category
        )
    };
}

/// INV-087 (FR-1201, п. 87): категорий ровно тринадцать, и коды справочника
/// совпадают с enum домена - «прочей» категории не существует.
#[tokio::test]
async fn inv087_thirteen_categories_match_the_domain_enum() {
    let db = require_db!();

    let codes = sqlx::query_scalar!("SELECT code FROM refdata.special_categories ORDER BY ordinal")
        .fetch_all(&db)
        .await
        .expect("каталог категорий");

    let domain: Vec<String> = SpecialCategory::ALL
        .iter()
        .map(|category| category.as_str().to_owned())
        .collect();
    assert_eq!(
        codes.len(),
        13,
        "перечень п. 87 закрыт тринадцатью позициями"
    );
    assert_eq!(codes, domain, "коды каталога совпадают с enum домена");

    // Каждая категория объявляет свои требования (FR-1201): срок проверки,
    // льготную схему, публикуемость и перечень документов
    let incomplete = sqlx::query_scalar!(
        r#"SELECT count(*) AS "incomplete!" FROM refdata.special_categories c
           WHERE c.review_days <= 0
              OR NOT EXISTS (SELECT 1 FROM refdata.special_category_documents d
                             WHERE d.category_code = c.code)"#
    )
    .fetch_one(&db)
    .await
    .expect("полнота деклараций");
    assert_eq!(
        incomplete, 0,
        "категория без срока проверки или без перечня документов"
    );
}

/// INV-087: заявка по категории вне перечня п. 87 отклоняется FK.
#[tokio::test]
async fn inv087_unknown_category_is_rejected() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let applicant_id = applicant(&mut tx).await.expect("заявитель");

    let error = rejected!(
        tx,
        request!(applicant_id, "category_14"),
        "категория вне перечня п. 87 обязана быть отклонена"
    );
    assert!(error.contains("special_requests_category_fkey"), "{error}");

    // Категория из перечня принимается
    request!(applicant_id, SpecialCategory::Category4.as_str())
        .fetch_one(&mut *tx)
        .await
        .expect("заявка по категории п. 87.4");
}

/// Состояния заявки БД и домена - один перечень (паритет enum, G16).
#[tokio::test]
async fn statuses_match_the_domain_enum() {
    let db = require_db!();

    let mut statuses = sqlx::query_scalar!(
        r#"SELECT unnest(enum_range(NULL::core.special_request_status))::text AS "value!"
           ORDER BY 1"#
    )
    .fetch_all(&db)
    .await
    .expect("значения enum");
    statuses.sort();

    let mut domain: Vec<String> = SpecialRequestStatus::ALL
        .iter()
        .map(|status| status.as_str().to_owned())
        .collect();
    domain.sort();

    assert_eq!(statuses, domain, "перечень состояний совпадает с доменом");
}

/// FR-1201 (п. 88–90): решение принимается по результатам проверки, а
/// принятое решение окончательно - тот же порядок, что и в домене.
#[tokio::test]
async fn decision_requires_review_and_is_final() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let applicant_id = applicant(&mut tx).await.expect("заявитель");

    let request_id = request!(applicant_id, SpecialCategory::Category1.as_str())
        .fetch_one(&mut *tx)
        .await
        .expect("заявка");

    let straight_to_decision = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.special_requests SET status = 'granted' WHERE id = $1",
            request_id
        ),
        "решение без проверки подразделения обязано быть отклонено"
    );
    assert!(
        straight_to_decision.contains("FR-1201"),
        "{straight_to_decision}"
    );
    assert!(
        !SpecialRequestStatus::Submitted.can_transition_to(SpecialRequestStatus::Granted),
        "домен запрещает тот же переход"
    );

    for status in ["under_review", "granted"] {
        sqlx::query!(
            "UPDATE core.special_requests
             SET status = $2::text::core.special_request_status WHERE id = $1",
            request_id,
            status
        )
        .execute(&mut *tx)
        .await
        .unwrap_or_else(|err| panic!("переход в {status}: {err}"));
    }

    let after_decision = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.special_requests SET status = 'refused' WHERE id = $1",
            request_id
        ),
        "пересмотр принятого решения обязан быть отклонен"
    );
    assert!(after_decision.contains("FR-1201"), "{after_decision}");
}

/// Отзыв заявки: время отзыва ставит сервер (NFR-03), CHECK его требует.
#[tokio::test]
async fn withdrawal_gets_its_timestamp_from_the_server() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let applicant_id = applicant(&mut tx).await.expect("заявитель");

    let request_id = request!(applicant_id, SpecialCategory::Category2.as_str())
        .fetch_one(&mut *tx)
        .await
        .expect("заявка");

    sqlx::query!(
        "UPDATE core.special_requests SET status = 'withdrawn' WHERE id = $1",
        request_id
    )
    .execute(&mut *tx)
    .await
    .expect("отзыв заявки");

    let withdrawn_at = sqlx::query_scalar!(
        "SELECT withdrawn_at FROM core.special_requests WHERE id = $1",
        request_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("время отзыва");
    assert!(withdrawn_at.is_some(), "время отзыва проставляет триггер");
}

/// Приложенный к заявке документ.
macro_rules! request_file {
    ($request_id:expr, $document_code:expr) => {
        sqlx::query!(
            "INSERT INTO core.special_request_files
                (special_request_id, document_code, file_key, filename, content_type, size_bytes)
             VALUES ($1, $2, 'special-requests/k', 'смета.pdf', 'application/pdf', 10)",
            $request_id,
            $document_code
        )
    };
}

/// FR-1201 (п. 88): документ закрывает позицию перечня своей категории -
/// чужая позиция отклоняется триггером.
#[tokio::test]
async fn document_belongs_to_the_category_list() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let applicant_id = applicant(&mut tx).await.expect("заявитель");

    let request_id = request!(applicant_id, SpecialCategory::Category3.as_str())
        .fetch_one(&mut *tx)
        .await
        .expect("заявка");

    let unknown_document = rejected!(
        tx,
        request_file!(request_id, "charter_of_mars"),
        "документ вне перечня категории обязан быть отклонен"
    );
    assert!(unknown_document.contains("FR-1201"), "{unknown_document}");

    let declared = sqlx::query_scalar!(
        "SELECT code FROM refdata.special_category_documents
         WHERE category_code = $1 ORDER BY ordinal LIMIT 1",
        SpecialCategory::Category3.as_str()
    )
    .fetch_one(&mut *tx)
    .await
    .expect("позиция перечня категории");

    request_file!(request_id, declared.as_str())
        .execute(&mut *tx)
        .await
        .expect("документ по объявленной позиции перечня");
}

/// Регламент А.5: заявка особого порядка - мутация домена, ее пишет аудит
/// (перечень INV-AUDIT, FR-1601).
#[tokio::test]
async fn special_request_is_audited() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let applicant_id = applicant(&mut tx).await.expect("заявитель");

    // `fetch_one`, а не `execute`: set_config возвращает столбец
    sqlx::query!(
        "SELECT set_config('app.user_id', $1, true)",
        applicant_id.to_string()
    )
    .fetch_one(&mut *tx)
    .await
    .expect("актор транзакции");

    let request_id = request!(applicant_id, SpecialCategory::Category5.as_str())
        .fetch_one(&mut *tx)
        .await
        .expect("заявка");

    let event = sqlx::query!(
        "SELECT action, actor_id FROM audit.log
         WHERE table_name = 'core.special_requests' AND row_id = $1",
        request_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("событие аудита заявки");

    assert_eq!(event.action, "INSERT");
    assert_eq!(
        event.actor_id,
        Some(applicant_id),
        "актор события - заявитель"
    );
}
