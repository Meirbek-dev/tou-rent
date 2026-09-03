//! Публикация протоколов и досье тендера против живой БД
//! (T28, FR-702, FR-1402, FR-1602, INV-076).
//!
//! Проверяется то, что делает БД сама: публикуется только сформированная
//! печатная форма, срок публичного доступа считает БД (шесть месяцев),
//! снятие раньше срока невозможно, а досье собирается триггерами в момент
//! событий и материал из него не изымается.
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use sqlx::Acquire as _;
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - публикация не проверялась");
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
    tender_id: Uuid,
    protocol_id: Uuid,
}

/// Тендер с протоколом итогов: `with_pdf` - сформирована ли печатная форма.
async fn fixture(tx: &mut sqlx::PgConnection, with_pdf: bool) -> Result<Fixture, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let organizer = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т28 организатор', now()) RETURNING id",
        format!("t28-org-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await?;

    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id, announced_at,
                                   submission_deadline, opening_at, opened_at)
         VALUES ('Т28 тендер', 'summed_up', $1, now() - interval '30 days',
                 now() - interval '10 days', now() - interval '9 days',
                 now() - interval '9 days')
         RETURNING id",
        organizer
    )
    .fetch_one(&mut *tx)
    .await?;

    let protocol_id = sqlx::query_scalar!(
        "INSERT INTO core.protocols (tender_id, kind, number, content, pdf_key)
         VALUES ($1, 'results', 'И-Т28', '{}'::jsonb, $2)
         RETURNING id",
        tender_id,
        with_pdf.then(|| format!("protocols/{tender_id}/results.pdf"))
    )
    .fetch_one(&mut *tx)
    .await?;

    Ok(Fixture {
        tender_id,
        protocol_id,
    })
}

/// Протокол, опубликованный семь месяцев назад: срок публичного доступа
/// истек. Момент публикации задается вставкой - переписать его нельзя.
async fn published_long_ago(
    tx: &mut sqlx::PgConnection,
    taken_down: bool,
) -> Result<Fixture, sqlx::Error> {
    let f = fixture(&mut *tx, true).await?;

    let protocol_id = sqlx::query_scalar!(
        "INSERT INTO core.protocols (tender_id, kind, number, content, pdf_key,
                                     published_at, unpublish_at, unpublished_at)
         VALUES ($1, 'admission', 'Д-Т28', '{}'::jsonb, $2,
                 core.now() - interval '7 months',
                 core.now() - interval '1 month',
                 CASE WHEN $3 THEN core.now() END)
         RETURNING id",
        f.tender_id,
        format!("protocols/{}/admission.pdf", f.tender_id),
        // Отметки - от сервера (`core.now()`, ADR-0005)
        taken_down
    )
    .fetch_one(&mut *tx)
    .await?;

    Ok(Fixture {
        tender_id: f.tender_id,
        protocol_id,
    })
}

/// FR-702 (п. 75): публикуется сформированная печатная форма, INV-076 -
/// срок публичного доступа проставляет БД.
#[tokio::test]
async fn fr702_publication_needs_a_document_and_sets_six_months() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");

    let empty = fixture(&mut tx, false).await.expect("протокол без формы");
    let error = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.protocols SET published_at = now() WHERE id = $1",
            empty.protocol_id
        ),
        "публикация без печатной формы обязана быть отклонена"
    );
    assert!(error.contains("FR-702"), "{error}");

    let f = fixture(&mut tx, true).await.expect("протокол с формой");
    sqlx::query!(
        "UPDATE core.protocols SET published_at = now() WHERE id = $1",
        f.protocol_id
    )
    .execute(&mut *tx)
    .await
    .expect("публикация");

    let protocol = sqlx::query!(
        "SELECT published_at, unpublish_at FROM core.protocols WHERE id = $1",
        f.protocol_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("протокол");

    let published = protocol.published_at.expect("момент публикации");
    let unpublish_at = protocol.unpublish_at.expect("срок снятия");
    let days = (unpublish_at - published).whole_days();
    assert!(
        (180..=185).contains(&days),
        "публичный доступ - шесть месяцев (получилось {days} дней)"
    );
}

/// INV-076: снять публикацию раньше истечения шести месяцев нельзя.
#[tokio::test]
async fn inv076_takedown_before_the_term_is_rejected() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, true).await.expect("фикстура");

    sqlx::query!(
        "UPDATE core.protocols SET published_at = now() WHERE id = $1",
        f.protocol_id
    )
    .execute(&mut *tx)
    .await
    .expect("публикация");

    let early = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.protocols SET unpublished_at = now() WHERE id = $1",
            f.protocol_id
        ),
        "снятие раньше срока обязано быть отклонено"
    );
    assert!(early.contains("INV-076"), "{early}");

    // По истечении срока снятие проходит, и протокол остается в досье
    let expired = published_long_ago(&mut tx, false)
        .await
        .expect("протокол с истекшим сроком");

    sqlx::query!(
        "UPDATE core.protocols SET unpublished_at = now() WHERE id = $1",
        expired.protocol_id
    )
    .execute(&mut *tx)
    .await
    .expect("снятие после срока");

    let in_dossier = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM core.dossier_items
           WHERE tender_id = $1 AND kind = 'publication'"#,
        expired.tender_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("досье");
    assert!(in_dossier > 0, "снятая публикация остается в досье (п. 76)");
}

/// Снятый протокол публично не возвращается (INV-076, п. 76).
#[tokio::test]
async fn expired_publication_is_not_restored() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = published_long_ago(&mut tx, true)
        .await
        .expect("снятый протокол");

    // Снятие необратимо: вернуть протокол в публичный доступ нельзя
    let restored = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.protocols SET unpublished_at = NULL WHERE id = $1",
            f.protocol_id
        ),
        "возврат снятого протокола в публичный доступ обязан быть отклонен"
    );
    assert!(restored.contains("INV-076"), "{restored}");

    // И момент публикации не переписывается - это юридический факт
    let rewritten = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.protocols SET published_at = now() WHERE id = $1",
            f.protocol_id
        ),
        "правка момента публикации обязана быть отклонена"
    );
    assert!(rewritten.contains("FR-702"), "{rewritten}");
}

/// FR-1602: досье собирается само - протокол попадает в него при создании.
#[tokio::test]
async fn dossier_collects_materials_by_itself() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, true).await.expect("фикстура");

    let item = sqlx::query!(
        "SELECT kind, title, file_key FROM core.dossier_items
         WHERE tender_id = $1 AND source_id = $2",
        f.tender_id,
        f.protocol_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("материал досье");

    assert_eq!(
        item.kind, "protocol",
        "протокол попадает в досье при создании"
    );
    assert!(
        item.title.unwrap_or_default().contains("И-Т28"),
        "материал подписан номером протокола"
    );
    assert!(
        item.file_key.is_some(),
        "в досье лежит ссылка на печатную форму"
    );

    // Повторные события не плодят дублей
    sqlx::query!(
        "UPDATE core.protocols SET number = 'И-Т28' WHERE id = $1",
        f.protocol_id
    )
    .execute(&mut *tx)
    .await
    .expect("повторное событие");

    let count = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM core.dossier_items
           WHERE tender_id = $1 AND kind = 'protocol'"#,
        f.tender_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("материалы досье");
    assert_eq!(count, 1, "досье собирается идемпотентно");
}

/// Досье - доказательная база: материал из него не изымается (FR-1602).
#[tokio::test]
async fn dossier_items_cannot_be_removed() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, true).await.expect("фикстура");

    let error = rejected!(
        tx,
        sqlx::query!(
            "DELETE FROM core.dossier_items WHERE tender_id = $1",
            f.tender_id
        ),
        "удаление материала досье обязано быть отклонено"
    );
    assert!(
        error.contains("FR-1602") || error.contains("permission denied"),
        "{error}"
    );
}

/// G-103: досье получает объявление и тендерную документацию (FR-1602, п. 16).
///
/// До правки вид `announcement` попадал в досье только со строкой
/// `core.tender_amendments`, то есть с РЕДАКЦИЕЙ документации: у тендера
/// без редакций `GET /dossier` отвечал пустым перечнем, а `dossier.zip`
/// собирался из одного манифеста. Здесь проверяется то, что делает БД
/// сама: публикация объявления и загрузка документации ложатся в досье
/// в момент события и повторных дублей не порождают.
#[tokio::test]
async fn dossier_receives_the_announcement_and_the_documentation() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let tag = Uuid::now_v7().simple().to_string();

    let organizer = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т28 организатор досье', now()) RETURNING id",
        format!("t28-dossier-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await
    .expect("организатор");

    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Т28 объект досье', 'адрес', 12.00) RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await
    .expect("объект");

    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id,
                                   submission_deadline, opening_at)
         VALUES ('Т28 черновик досье', 'draft', $1,
                 core.now() + interval '11 days', core.now() + interval '12 days')
         RETURNING id",
        organizer
    )
    .fetch_one(&mut *tx)
    .await
    .expect("черновик");

    sqlx::query!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                base_rate_monthly, rate_calculation, guarantee_fee)
         VALUES ($1, 1, $2, 'офис', 12, 60000.00, '{}'::jsonb, 60000.00)",
        tender_id,
        object_id
    )
    .execute(&mut *tx)
    .await
    .expect("лот");

    // Документация прикладывается к черновику (FR-304), объявление выходит
    // публикацией: в досье обязаны попасть оба события
    sqlx::query!(
        "INSERT INTO core.tender_docs (tender_id, version, title, file_key)
         VALUES ($1, 1, 'Тендерная документация', $2)",
        tender_id,
        format!("tenders/{tender_id}/documentation.pdf")
    )
    .execute(&mut *tx)
    .await
    .expect("тендерная документация");

    let draft_announcements = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM core.dossier_items
           WHERE tender_id = $1 AND source_table = 'core.tenders'"#,
        tender_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("досье черновика");
    assert_eq!(
        draft_announcements, 0,
        "черновик не опубликован - объявления в досье нет (п. 5–6)"
    );

    sqlx::query!(
        "UPDATE core.tenders SET status = 'announced' WHERE id = $1",
        tender_id
    )
    .execute(&mut *tx)
    .await
    .expect("публикация объявления");

    let items = sqlx::query!(
        "SELECT kind, title, file_key, source_table FROM core.dossier_items
         WHERE tender_id = $1 ORDER BY source_table",
        tender_id
    )
    .fetch_all(&mut *tx)
    .await
    .expect("материалы досье");

    assert_eq!(items.len(), 2, "объявление и документация - два материала");
    assert!(
        items.iter().all(|item| item.kind == "announcement"),
        "оба материала - раздел «объявление и редакции документации»"
    );

    let documentation = items
        .iter()
        .find(|item| item.source_table.as_deref() == Some("core.tender_docs"))
        .expect("тендерная документация в досье");
    assert!(
        documentation.file_key.is_some(),
        "у документации в досье есть файл - иначе архив собирается пустым"
    );

    let announcement = items
        .iter()
        .find(|item| item.source_table.as_deref() == Some("core.tenders"))
        .expect("объявление в досье");
    assert!(
        announcement
            .title
            .as_deref()
            .unwrap_or_default()
            .contains("Т28 черновик досье"),
        "объявление подписано названием тендера"
    );

    // Ход процедуры дальше объявления не переиздает
    sqlx::query!(
        "UPDATE core.tenders SET status = 'accepting' WHERE id = $1",
        tender_id
    )
    .execute(&mut *tx)
    .await
    .expect("прием заявок открыт");

    let total = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM core.dossier_items
           WHERE tender_id = $1 AND kind = 'announcement'"#,
        tender_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("материалы досье");
    assert_eq!(total, 2, "досье собирается идемпотентно");
}

/// FR-1602, п. 16, 42: у объявления в досье появляется файл - иначе
/// `dossier.zip` начинается со строки без документа.
///
/// Круг 7 гаунтлета (G-156). Материал заводит триггер публикации, а ключ
/// файла записывает http-слой снимком печатной формы Прил. 1. Проверяется
/// последний рубеж: ключ ложится ровно на материал объявления, повторная
/// публикация записанный ключ не переписывает, и след остается в аудите.
#[tokio::test]
async fn the_announcement_in_the_dossier_gets_its_file() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let tag = Uuid::now_v7().simple().to_string();

    let organizer = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'G156 организатор', core.now()) RETURNING id",
        format!("g156-organizer-{tag}@tou.test")
    )
    .fetch_one(&mut *tx)
    .await
    .expect("организатор");

    // Актор транзакции: событие аудита по материалу досье пишется под ним
    tou_db::set_actor(&mut tx, organizer)
        .await
        .expect("актор транзакции");

    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'G156 объект', 'адрес', 14.00) RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await
    .expect("объект");

    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id,
                                   submission_deadline, opening_at)
         VALUES ('G156 тендер', 'draft', $1,
                 core.now() + interval '11 days', core.now() + interval '12 days')
         RETURNING id",
        organizer
    )
    .fetch_one(&mut *tx)
    .await
    .expect("черновик");

    sqlx::query!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                base_rate_monthly, rate_calculation, guarantee_fee)
         VALUES ($1, 1, $2, 'офис', 12, 60000.00, '{}'::jsonb, 60000.00)",
        tender_id,
        object_id
    )
    .execute(&mut *tx)
    .await
    .expect("лот");

    sqlx::query!(
        "UPDATE core.tenders SET status = 'announced' WHERE id = $1",
        tender_id
    )
    .execute(&mut *tx)
    .await
    .expect("публикация объявления");

    let before = sqlx::query_scalar!(
        "SELECT file_key FROM core.dossier_items
         WHERE tender_id = $1 AND source_table = 'core.tenders'",
        tender_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("материал объявления");
    assert!(
        before.is_none(),
        "триггер заводит материал без файла - файл кладет http-слой"
    );

    let key = format!("tenders/{tender_id}/announcement.pdf");
    let attached = tou_db::publications::attach_announcement_file_on(&mut tx, tender_id, &key)
        .await
        .expect("ключ объявления");
    assert!(attached, "ключ записан на материал объявления");

    let stored = sqlx::query_scalar!(
        "SELECT file_key FROM core.dossier_items
         WHERE tender_id = $1 AND source_table = 'core.tenders'",
        tender_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("материал объявления");
    assert_eq!(
        stored.as_deref(),
        Some(key.as_str()),
        "в досье лежит ключ опубликованного объявления (FR-1602)"
    );

    // Тендерная документация - отдельный материал того же раздела: ключ
    // объявления на нее не попадает
    let others = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM core.dossier_items
           WHERE tender_id = $1 AND source_table <> 'core.tenders' AND file_key = $2"#,
        tender_id,
        key
    )
    .fetch_one(&mut *tx)
    .await
    .expect("прочие материалы");
    assert_eq!(others, 0, "ключ лег ровно на объявление");

    // Повторная публикация опубликованное объявление не подменяет
    let again = tou_db::publications::attach_announcement_file_on(
        &mut tx,
        tender_id,
        "tenders/подмена/announcement.pdf",
    )
    .await
    .expect("повторная запись ключа");
    assert!(!again, "записанный ключ объявления не переписывается");

    let audited = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM audit.log
           WHERE table_name = 'core.dossier_items' AND action = 'UPDATE'
             AND actor_id = $1 AND payload -> 'new' ->> 'file_key' = $2"#,
        organizer,
        key
    )
    .fetch_one(&mut *tx)
    .await
    .expect("события аудита");
    assert_eq!(audited, 1, "запись ключа оставила след в аудите (А.5)");

    tx.rollback().await.expect("rollback");
}
