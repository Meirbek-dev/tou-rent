//! Досье решения особого порядка и WORM-хранение против живой БД
//! (T38, FR-1206, FR-1602, INV-042, п. 97, 16.15, 42).
//!
//! Проверяется то, что делает БД сама: досье решения собирается триггерами
//! в момент событий (заявка, ее документы, заключение, решение), собирается
//! идемпотентно, и каждый материал получает срок хранения - три года
//! решениям особого порядка, пять лет тендерным материалам. Срок не
//! сокращается, предмет досье не переписывается, файл не отвязывается.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use sqlx::Acquire as _;
use tou_domain::publication::DossierSubject;
use tou_domain::special::{SpecialCategory, SpecialDecision};
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - досье решения не проверялось");
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
    request_id: Uuid,
    staff_id: Uuid,
}

/// Заявка особого порядка с документом и сотрудник, который ее ведет.
async fn fixture(tx: &mut sqlx::PgConnection) -> Result<Fixture, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let applicant_id = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т38 заявитель', now()) RETURNING id",
        format!("t38-applicant-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let staff_id = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т38 подразделение', now()) RETURNING id",
        format!("t38-staff-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let request_id = sqlx::query_scalar!(
        "INSERT INTO core.special_requests
           (applicant_id, category, applicant_kind, applicant_details, purpose)
         VALUES ($1, $2, 'legal_entity', '{}'::jsonb, 'размещение оборудования')
         RETURNING id",
        applicant_id,
        SpecialCategory::Category4.as_str()
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query!(
        "INSERT INTO core.special_request_files
           (special_request_id, file_key, filename, content_type, size_bytes)
         VALUES ($1, $2, 'смета.pdf', 'application/pdf', 1024)",
        request_id,
        format!("special-requests/{request_id}/estimate.pdf")
    )
    .execute(&mut *tx)
    .await?;

    Ok(Fixture {
        request_id,
        staff_id,
    })
}

/// Заявка проведена через заключение и решение Правления.
async fn decided(tx: &mut sqlx::PgConnection) -> Result<Fixture, sqlx::Error> {
    let f = fixture(&mut *tx).await?;

    // `::text::core.special_decision`: значение приходит строкой доменного
    // типа, а приведение к перечислению делает БД (домен не знает sqlx)
    sqlx::query!(
        "INSERT INTO core.special_reviews
           (special_request_id, reviewer_id, conclusion, recommendation)
         VALUES ($1, $2, 'Заявка соответствует требованиям категории',
                 $3::text::core.special_decision)",
        f.request_id,
        f.staff_id,
        SpecialDecision::Grant.as_str()
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        "INSERT INTO core.special_board_decisions
           (special_request_id, decision, rationale, decided_by)
         VALUES ($1, $2::text::core.special_decision, 'обоснование решения', $3)",
        f.request_id,
        SpecialDecision::Grant.as_str(),
        f.staff_id
    )
    .execute(&mut *tx)
    .await?;

    Ok(f)
}

/// FR-1206 (п. 97): досье решения собирается само - заявка, ее документы,
/// заключение подразделения и решение Правления.
#[tokio::test]
async fn fr1206_decision_dossier_collects_materials_by_itself() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = decided(&mut tx).await.expect("рассмотренная заявка");

    let kinds: Vec<(String, i64)> = sqlx::query!(
        r#"SELECT kind, count(*) AS "count!" FROM core.dossier_items
           WHERE special_request_id = $1 GROUP BY kind ORDER BY kind"#,
        f.request_id
    )
    .fetch_all(&mut *tx)
    .await
    .expect("состав досье")
    .into_iter()
    .map(|row| (row.kind, row.count))
    .collect();

    assert_eq!(
        kinds,
        vec![
            ("application".to_owned(), 2), // сама заявка и ее документ
            ("decision".to_owned(), 1),
            ("review".to_owned(), 1),
        ],
        "досье решения содержит заявку, документы, заключение и решение"
    );

    let file_in_dossier = sqlx::query_scalar!(
        "SELECT file_key FROM core.dossier_items
         WHERE special_request_id = $1 AND source_table = 'core.special_request_files'",
        f.request_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("документ заявки в досье");
    assert!(
        file_in_dossier.is_some_and(|key| key.contains("special-requests/")),
        "в досье лежит ссылка на файл документа"
    );
}

/// FR-1602: досье собирается идемпотентно - печатная форма решения
/// догружается в уже записанный материал, дубля не возникает.
#[tokio::test]
async fn decision_pdf_lands_in_the_existing_material() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = decided(&mut tx).await.expect("рассмотренная заявка");

    sqlx::query!(
        "UPDATE core.special_board_decisions SET pdf_key = $2 WHERE special_request_id = $1",
        f.request_id,
        format!("special-requests/{}/decision.pdf", f.request_id)
    )
    .execute(&mut *tx)
    .await
    .expect("печатная форма решения");

    let materials = sqlx::query_scalar!(
        "SELECT file_key FROM core.dossier_items
         WHERE special_request_id = $1 AND kind = 'decision'",
        f.request_id
    )
    .fetch_all(&mut *tx)
    .await
    .expect("материалы решения");

    assert_eq!(materials.len(), 1, "решение остается одним материалом");
    assert!(
        materials[0]
            .as_deref()
            .is_some_and(|key| key.ends_with("decision.pdf")),
        "протокол решения дописан в тот же материал"
    );
}

/// INV-042 (п. 16.15, 42): срок хранения задает предмет досье - три года
/// решениям особого порядка, пять лет тендерным материалам.
#[tokio::test]
async fn inv042_retention_follows_the_dossier_subject() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = decided(&mut tx).await.expect("рассмотренная заявка");

    // `::int` - приведение: планировщик считает его потенциально NULL,
    // хотя оба столбца NOT NULL
    let years = sqlx::query_scalar!(
        r#"SELECT extract(year FROM age(retain_until, occurred_at))::int AS "years!"
           FROM core.dossier_items WHERE special_request_id = $1"#,
        f.request_id
    )
    .fetch_all(&mut *tx)
    .await
    .expect("сроки хранения");

    assert!(!years.is_empty(), "материалы досье получили срок хранения");
    for term in years {
        assert_eq!(
            term,
            DossierSubject::SpecialRequest.retention_years(),
            "решение особого порядка хранится три года (FR-1206)"
        );
    }

    // Тендерные материалы - пять лет (п. 16.15, 42)
    let organizer = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т38 организатор', now()) RETURNING id",
        format!("t38-org-{}@tou.test", Uuid::now_v7().simple())
    )
    .fetch_one(&mut *tx)
    .await
    .expect("организатор");

    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id, announced_at,
                                   submission_deadline, opening_at, opened_at)
         VALUES ('Т38 тендер', 'summed_up', $1, now() - interval '30 days',
                 now() - interval '10 days', now() - interval '9 days',
                 now() - interval '9 days')
         RETURNING id",
        organizer
    )
    .fetch_one(&mut *tx)
    .await
    .expect("тендер");

    let tender_term = sqlx::query_scalar!(
        r#"INSERT INTO core.dossier_items (tender_id, kind, title, source_table, source_id)
           VALUES ($1, 'announcement', 'Объявление о тендере', 'core.tenders', $1)
           RETURNING extract(year FROM age(retain_until, occurred_at))::int AS "years!""#,
        tender_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("материал досье тендера");

    assert_eq!(
        tender_term,
        DossierSubject::Tender.retention_years(),
        "тендерные материалы хранятся пять лет (п. 16.15, 42)"
    );
}

/// INV-042: WORM - срок хранения не сокращается, предмет досье не
/// переписывается, а файл у материала не отвязывается.
#[tokio::test]
async fn inv042_retention_cannot_be_shortened() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = decided(&mut tx).await.expect("рассмотренная заявка");

    let shortened = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.dossier_items SET retain_until = now()
             WHERE special_request_id = $1",
            f.request_id
        ),
        "сокращение срока хранения обязано быть отклонено"
    );
    assert!(shortened.contains("INV-042"), "{shortened}");

    let moved = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.dossier_items SET special_request_id = NULL, tender_id = NULL
             WHERE special_request_id = $1",
            f.request_id
        ),
        "подмена предмета досье обязана быть отклонена"
    );
    assert!(moved.contains("INV-042"), "{moved}");

    let detached = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.dossier_items SET file_key = NULL
             WHERE special_request_id = $1 AND file_key IS NOT NULL",
            f.request_id
        ),
        "отвязка файла материала обязана быть отклонена"
    );
    assert!(detached.contains("INV-042"), "{detached}");

    // Продление срока хранения - законный ход: он «не менее», а не «ровно»
    sqlx::query!(
        "UPDATE core.dossier_items SET retain_until = retain_until + interval '1 year'
         WHERE special_request_id = $1",
        f.request_id
    )
    .execute(&mut *tx)
    .await
    .expect("продление срока хранения");
}

/// FR-1602: материал досье решения не изымается - как и материал тендера.
#[tokio::test]
async fn decision_dossier_items_cannot_be_removed() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = decided(&mut tx).await.expect("рассмотренная заявка");

    let error = rejected!(
        tx,
        sqlx::query!(
            "DELETE FROM core.dossier_items WHERE special_request_id = $1",
            f.request_id
        ),
        "удаление материала досье обязано быть отклонено"
    );
    assert!(
        error.contains("FR-1602") || error.contains("permission denied"),
        "{error}"
    );
}
