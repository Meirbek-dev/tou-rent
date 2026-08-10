//! Публикации особого порядка против живой БД
//! (T39, FR-1403, FR-1206, INV-076, п. 90, 92, 97).
//!
//! Проверяется то, что делает БД сама: публикуется результат принятого
//! решения со сформированным протоколом и только по публикуемой категории
//! (INV-087), срок публичного доступа считает БД (шесть месяцев), снятие
//! раньше срока невозможно и необратимо, публикация однократна, а материал
//! попадает в досье решения (FR-1206).
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use sqlx::Acquire as _;
use tou_domain::publication::PUBLIC_ACCESS_MONTHS;
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - публикации не проверялись");
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

/// Выкладка результата на портал. Общий текст остался в одном месте, но
/// теперь это макрос, а не константа: проверяемому запросу нужен литерал.
macro_rules! publish {
    ($request_id:expr, $file_key:expr, $published_by:expr) => {
        sqlx::query!(
            "INSERT INTO core.public_records
                (kind, special_request_id, title, file_key, published_by)
             VALUES ('decision', $1, 'Результат рассмотрения заявки особого порядка', $2, $3)",
            $request_id,
            $file_key,
            $published_by
        )
    };
}

/// Заявка особого порядка с заключением и решением Правления.
async fn decided(tx: &mut sqlx::PgConnection, with_pdf: bool) -> Result<Fixture, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let applicant_id = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т39 заявитель', now()) RETURNING id",
        format!("t39-applicant-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let staff_id = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т39 подразделение', now()) RETURNING id",
        format!("t39-staff-{tag}@tou.test")
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

    // `$3::text::core.special_decision`: решение приходит строкой доменного
    // типа, а приведение к перечислению делает БД
    sqlx::query!(
        "INSERT INTO core.special_reviews
           (special_request_id, reviewer_id, conclusion, recommendation)
         VALUES ($1, $2, 'Заявка соответствует требованиям категории',
                 $3::text::core.special_decision)",
        request_id,
        staff_id,
        SpecialDecision::Grant.as_str()
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        "INSERT INTO core.special_board_decisions
           (special_request_id, decision, rationale, decided_by, pdf_key)
         VALUES ($1, $2::text::core.special_decision, 'обоснование решения', $3, $4)",
        request_id,
        SpecialDecision::Grant.as_str(),
        staff_id,
        with_pdf.then(|| format!("special-requests/{request_id}/decision.pdf"))
    )
    .execute(&mut *tx)
    .await?;

    Ok(Fixture {
        request_id,
        staff_id,
    })
}

/// FR-1403 (п. 97): публикуется результат принятого решения со сформированным
/// протоколом, а срок публичного доступа считает БД - шесть месяцев (INV-076).
#[tokio::test]
async fn fr1403_publication_needs_a_protocol_and_lasts_six_months() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");

    let empty = decided(&mut tx, false).await.expect("решение без формы");
    let error = rejected!(
        tx,
        publish!(empty.request_id, None::<String>, empty.staff_id),
        "публикация без печатной формы обязана быть отклонена"
    );
    assert!(error.contains("FR-1403"), "{error}");

    let f = decided(&mut tx, true).await.expect("решение с формой");
    publish!(
        f.request_id,
        format!("special-requests/{}/decision.pdf", f.request_id),
        f.staff_id
    )
    .execute(&mut *tx)
    .await
    .expect("публикация результата");

    let record = sqlx::query!(
        "SELECT published_at, unpublish_at FROM core.public_records
         WHERE special_request_id = $1",
        f.request_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("публикация");

    let days = (record.unpublish_at - record.published_at).whole_days();
    assert!(
        (180..=185).contains(&days),
        "публичный доступ - {PUBLIC_ACCESS_MONTHS} месяцев (получилось {days} дней)"
    );
}

/// FR-1403 (п. 87, 97): по непубликуемой категории результат на портал
/// не попадает - публикуемость объявляет категория (INV-087).
#[tokio::test]
async fn fr1403_unpublishable_category_stays_off_the_portal() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = decided(&mut tx, true).await.expect("решение");

    // Публикуемость - данные справочника: их ведет админ, поэтому правка
    // идет под ролью владельца и живет до конца транзакции
    sqlx::query!("RESET ROLE")
        .execute(&mut *tx)
        .await
        .expect("роль владельца справочников");
    sqlx::query!(
        "UPDATE refdata.special_categories SET publishable = false WHERE code = $1",
        SpecialCategory::Category4.as_str()
    )
    .execute(&mut *tx)
    .await
    .expect("категория помечена непубликуемой");
    sqlx::query!("SET ROLE tou_rent_app")
        .execute(&mut *tx)
        .await
        .expect("возврат роли приложения");

    let error = rejected!(
        tx,
        publish!(
            f.request_id,
            format!("special-requests/{}/decision.pdf", f.request_id),
            f.staff_id
        ),
        "публикация по непубликуемой категории обязана быть отклонена"
    );
    assert!(error.contains("FR-1403"), "{error}");
}

/// INV-076 (п. 76): снять публикацию раньше истечения шести месяцев нельзя,
/// а снятая публикация публично не возвращается.
#[tokio::test]
async fn inv076_takedown_follows_the_term_and_is_final() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = decided(&mut tx, true).await.expect("решение");

    publish!(
        f.request_id,
        format!("special-requests/{}/decision.pdf", f.request_id),
        f.staff_id
    )
    .execute(&mut *tx)
    .await
    .expect("публикация результата");

    let early = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.public_records SET unpublished_at = now() WHERE special_request_id = $1",
            f.request_id
        ),
        "снятие раньше срока обязано быть отклонено"
    );
    assert!(early.contains("INV-076"), "{early}");

    // По истечении срока снятие проходит - и оно необратимо
    sqlx::query!(
        "UPDATE core.public_records
         SET unpublished_at = unpublish_at + interval '1 day'
         WHERE special_request_id = $1",
        f.request_id
    )
    .execute(&mut *tx)
    .await
    .expect("снятие после срока");

    let restored = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.public_records SET unpublished_at = NULL WHERE special_request_id = $1",
            f.request_id
        ),
        "возврат снятой публикации обязан быть отклонен"
    );
    assert!(restored.contains("INV-076"), "{restored}");
}

/// FR-1206: публикация ложится в досье решения - и остается в нем после
/// снятия с портала (п. 76, 97).
#[tokio::test]
async fn publication_is_recorded_in_the_decision_dossier() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = decided(&mut tx, true).await.expect("решение");

    publish!(
        f.request_id,
        format!("special-requests/{}/decision.pdf", f.request_id),
        f.staff_id
    )
    .execute(&mut *tx)
    .await
    .expect("публикация результата");

    let item = sqlx::query!(
        "SELECT title, file_key FROM core.dossier_items
         WHERE special_request_id = $1 AND kind = 'publication'",
        f.request_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("материал досье");

    assert!(
        item.title.unwrap_or_default().contains("опубликовано"),
        "факт публикации подписан в досье"
    );
    assert!(
        item.file_key.is_some(),
        "в досье лежит ссылка на печатную форму"
    );

    // Снятие не изымает материал - оно только меняет подпись (п. 76)
    sqlx::query!(
        "UPDATE core.public_records
         SET unpublished_at = unpublish_at + interval '1 day'
         WHERE special_request_id = $1",
        f.request_id
    )
    .execute(&mut *tx)
    .await
    .expect("снятие после срока");

    let after = sqlx::query!(
        r#"SELECT count(*) AS "count!", min(title) AS "title" FROM core.dossier_items
           WHERE special_request_id = $1 AND kind = 'publication'"#,
        f.request_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("материал досье после снятия");

    assert_eq!(after.count, 1, "снятие не плодит материалов досье");
    assert!(
        after.title.unwrap_or_default().contains("снята"),
        "в досье записано и снятие публикации (п. 76)"
    );
}

/// Публикация однократна (п. 97): повторно выложить тот же материал нельзя.
#[tokio::test]
async fn publication_happens_once() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = decided(&mut tx, true).await.expect("решение");

    let key = format!("special-requests/{}/decision.pdf", f.request_id);
    publish!(f.request_id, key.as_str(), f.staff_id)
        .execute(&mut *tx)
        .await
        .expect("публикация результата");

    let again = rejected!(
        tx,
        publish!(f.request_id, key.as_str(), f.staff_id),
        "повторная публикация обязана быть отклонена"
    );
    assert!(
        again.contains("public_records_decision_idx") || again.contains("duplicate key"),
        "{again}"
    );
}

/// Публикация - юридический факт: запись не удаляется (FR-1403, п. 97).
#[tokio::test]
async fn publications_cannot_be_removed() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = decided(&mut tx, true).await.expect("решение");

    publish!(
        f.request_id,
        format!("special-requests/{}/decision.pdf", f.request_id),
        f.staff_id
    )
    .execute(&mut *tx)
    .await
    .expect("публикация результата");

    let error = rejected!(
        tx,
        sqlx::query!(
            "DELETE FROM core.public_records WHERE special_request_id = $1",
            f.request_id
        ),
        "удаление публикации обязано быть отклонено"
    );
    assert!(
        error.contains("FR-1403") || error.contains("permission denied"),
        "{error}"
    );
}
