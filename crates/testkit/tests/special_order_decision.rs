//! Проверка подразделением и решение Правления против живой БД
//! (T34, FR-1202, INV-090, п. 89–90).
//!
//! INV-090: решение Правления невозможно без заключения подразделения -
//! проверяется отказом триггера на заявке без заключения. Заодно: заключение
//! выносит заявку на рассмотрение Правления, решение переводит ее в свое
//! терминальное состояние, оба факта неизменяемы, сроки п. 89–90 закрываются
//! исполнением (FR-1702).
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use sqlx::Acquire as _;
use tou_domain::obligation::ObligationAction;
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - решение по заявке не проверялось");
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
    reviewer_id: Uuid,
}

/// Заявка особого порядка в состоянии «подана» и сотрудник, который ее ведет.
async fn fixture(tx: &mut sqlx::PgConnection) -> Result<Fixture, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let applicant_id = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т34 заявитель', now()) RETURNING id",
        format!("t34-applicant-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let reviewer_id = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т34 подразделение', now()) RETURNING id",
        format!("t34-reviewer-{tag}@tou.test")
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

    Ok(Fixture {
        request_id,
        reviewer_id,
    })
}

/// Заключение подразделения и решение Правления. Общий текст остался в одном
/// месте, но теперь это макросы: проверяемому запросу нужен литерал, а
/// `$N::text::core.special_decision` оставляет приведение к перечислению БД.
macro_rules! review {
    ($request_id:expr, $reviewer_id:expr, $recommendation:expr) => {
        sqlx::query!(
            "INSERT INTO core.special_reviews
                (special_request_id, reviewer_id, conclusion, recommendation)
             VALUES ($1, $2, 'Заявка соответствует требованиям категории',
                     $3::text::core.special_decision)",
            $request_id,
            $reviewer_id,
            $recommendation
        )
    };
}

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

/// INV-090 (п. 89–90): решение Правления по заявке без заключения
/// подразделения отклоняется триггером.
#[tokio::test]
async fn inv090_decision_without_a_conclusion_is_rejected() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    let error = rejected!(
        tx,
        decision!(f.request_id, SpecialDecision::Grant.as_str(), f.reviewer_id),
        "решение без заключения обязано быть отклонено"
    );
    assert!(error.contains("INV-090"), "{error}");

    // С заключением то же решение проходит
    review!(f.request_id, f.reviewer_id, SpecialDecision::Grant.as_str())
        .execute(&mut *tx)
        .await
        .expect("заключение подразделения");

    decision!(f.request_id, SpecialDecision::Grant.as_str(), f.reviewer_id)
        .execute(&mut *tx)
        .await
        .expect("решение по заключению");

    let status = sqlx::query_scalar!(
        r#"SELECT status::text AS "status!" FROM core.special_requests WHERE id = $1"#,
        f.request_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("состояние заявки");
    assert_eq!(
        status, "granted",
        "решение переводит заявку в свое состояние"
    );
}

/// FR-1202 (п. 89): заключение выносит заявку на рассмотрение Правления;
/// второе заключение по той же заявке невозможно.
#[tokio::test]
async fn conclusion_moves_the_request_to_the_board() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    review!(
        f.request_id,
        f.reviewer_id,
        SpecialDecision::Refuse.as_str()
    )
    .execute(&mut *tx)
    .await
    .expect("заключение");

    let status = sqlx::query_scalar!(
        r#"SELECT status::text AS "status!" FROM core.special_requests WHERE id = $1"#,
        f.request_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("состояние заявки");
    assert_eq!(status, "under_review");

    let second = rejected!(
        tx,
        review!(f.request_id, f.reviewer_id, SpecialDecision::Grant.as_str()),
        "второе заключение по заявке обязано быть отклонено"
    );
    assert!(
        second.contains("special_reviews_special_request_id_key") || second.contains("FR-1202"),
        "{second}"
    );
}

/// Заключение и решение - юридические факты: их не переписывают (п. 90, 97).
#[tokio::test]
async fn conclusion_and_decision_are_immutable() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    review!(f.request_id, f.reviewer_id, SpecialDecision::Grant.as_str())
        .execute(&mut *tx)
        .await
        .expect("заключение");
    decision!(f.request_id, SpecialDecision::Grant.as_str(), f.reviewer_id)
        .execute(&mut *tx)
        .await
        .expect("решение");

    let edited_review = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.special_reviews SET conclusion = 'иное' WHERE special_request_id = $1",
            f.request_id
        ),
        "правка заключения обязана быть отклонена"
    );
    // Первый рубеж - REVOKE (роль приложения не имеет UPDATE), второй -
    // триггер forbid_mutation: он работает и для владельца БД
    assert!(
        edited_review.contains("FR-1202") || edited_review.contains("permission denied"),
        "{edited_review}"
    );

    let edited_decision = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.special_board_decisions SET decision = 'refuse'
             WHERE special_request_id = $1",
            f.request_id
        ),
        "пересмотр решения правкой строки обязан быть отклонен"
    );
    assert!(edited_decision.contains("FR-1202"), "{edited_decision}");

    // Печатная форма догружается - это не пересмотр решения
    sqlx::query!(
        "UPDATE core.special_board_decisions SET pdf_key = 'k' WHERE special_request_id = $1",
        f.request_id
    )
    .execute(&mut *tx)
    .await
    .expect("ключ печатной формы");
}

/// Решения БД и домена - один перечень (паритет enum, G16).
#[tokio::test]
async fn decisions_match_the_domain_enum() {
    let db = require_db!();

    let mut values = sqlx::query_scalar!(
        r#"SELECT unnest(enum_range(NULL::core.special_decision))::text AS "value!"
           ORDER BY 1"#
    )
    .fetch_all(&db)
    .await
    .expect("значения enum");
    values.sort();

    let mut domain: Vec<String> = SpecialDecision::ALL
        .iter()
        .map(|decision| decision.as_str().to_owned())
        .collect();
    domain.sort();

    assert_eq!(values, domain, "перечень решений совпадает с доменом");
}

/// FR-1702: срок проверки ставится подачей заявки и берется из категории
/// (FR-1201), а заключение его закрывает и открывает срок решения (п. 90).
#[tokio::test]
async fn review_and_decision_terms_follow_the_process() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    // Подача через слой данных ставит срок проверки сама; здесь фикстура
    // вставляет заявку напрямую, поэтому срок ставится тем же вызовом
    tou_db::obligations::schedule(
        &mut tx,
        ObligationAction::SpecialReview,
        tou_db::obligations::Subject::special_request(f.request_id),
    )
    .await
    .expect("срок проверки");

    let term = sqlx::query!(
        r#"SELECT action, status::text AS "status!" FROM core.obligations
           WHERE special_request_id = $1"#,
        f.request_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("срок проверки заявки");
    assert_eq!(term.action, ObligationAction::SpecialReview.as_str());
    assert_eq!(term.status, "pending");

    // Заключение закрывает срок проверки и ставит срок решения Правления
    tou_db::obligations::complete(
        &mut tx,
        ObligationAction::SpecialReview,
        tou_db::obligations::Subject::special_request(f.request_id),
    )
    .await
    .expect("закрытие срока проверки");
    tou_db::obligations::schedule(
        &mut tx,
        ObligationAction::SpecialDecision,
        tou_db::obligations::Subject::special_request(f.request_id),
    )
    .await
    .expect("срок решения");

    let open = sqlx::query_scalar!(
        "SELECT action FROM core.obligations
         WHERE special_request_id = $1 AND status = 'pending'",
        f.request_id
    )
    .fetch_all(&mut *tx)
    .await
    .expect("открытые сроки");
    assert_eq!(open, vec![ObligationAction::SpecialDecision.as_str()]);
}

/// Отзыв заявки снимает открытые сроки: спрашивать за них больше не с кого.
#[tokio::test]
async fn withdrawal_cancels_open_terms() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("фикстура");

    tou_db::obligations::schedule(
        &mut tx,
        ObligationAction::SpecialReview,
        tou_db::obligations::Subject::special_request(f.request_id),
    )
    .await
    .expect("срок проверки");

    sqlx::query!(
        "UPDATE core.special_requests SET status = 'withdrawn' WHERE id = $1",
        f.request_id
    )
    .execute(&mut *tx)
    .await
    .expect("отзыв заявки");
    tou_db::obligations::cancel_for(
        &mut tx,
        tou_db::obligations::Subject::special_request(f.request_id),
    )
    .await
    .expect("снятие сроков");

    let statuses = sqlx::query_scalar!(
        r#"SELECT status::text AS "status!" FROM core.obligations
           WHERE special_request_id = $1"#,
        f.request_id
    )
    .fetch_all(&mut *tx)
    .await
    .expect("сроки заявки");
    assert_eq!(statuses, vec!["cancelled".to_owned()]);
}
