//! Голос члена комиссии подается один раз (G-111; FR-1103, INV-1103, п. 13–14).
//!
//! Запись голоса шла `ON CONFLICT ... DO UPDATE`: второй вызов молча
//! затирал первый голос и особое мнение, хотя окно подтверждения обещает
//! обратное - «голос не может быть изменен». Первый голос исчезал без
//! следа в подсчете, а расклад по заявке становился виден до окончания
//! голосования.
//!
//! Здесь проверяются оба рубежа против живой БД: обработчик отвечает
//! отказом по правилу `vote_already_cast` вместо перезаписи, а сама
//! таблица отбивает правку голоса мимо него (триггер
//! `core.check_vote_is_cast_once`).
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use tou_db::commission::{self, CommissionError};
use tou_domain::commission::Vote;
use tou_domain::rule::RuleViolation;
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - голосование не проверялось");
                return;
            }
        }
    };
}

struct Fixture {
    actor: Uuid,
    tender: Uuid,
    application: Uuid,
    meeting: Uuid,
    /// Первый голосующий - председатель заседания
    member: Uuid,
}

async fn user(db: &tou_db::Db, tag: &str) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'argon2-заглушка', $2, core.now()) RETURNING id",
        format!("g111-{tag}-{}@tou.test", Uuid::now_v7().simple()),
        format!("G111 {tag}")
    )
    .fetch_one(db)
    .await
}

/// Открытое заседание допуска с кворумом и одной поданной заявкой:
/// состав из семи голосующих и резервного (FR-1101, п. 9–11), явка
/// председателя и пяти членов - две трети (FR-1102, п. 12).
async fn fixture(db: &tou_db::Db) -> Result<Fixture, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let commission = sqlx::query_scalar!(
        "INSERT INTO core.commissions (name, valid_from, valid_until)
         VALUES ($1, current_date, current_date + interval '1 year') RETURNING id",
        format!("G111 комиссия {tag}")
    )
    .fetch_one(db)
    .await?;

    let roles = [
        "chairman", "deputy", "member", "member", "member", "member", "member", "reserve",
    ];
    let mut members = Vec::with_capacity(roles.len());
    for (index, role) in roles.iter().enumerate() {
        let user_id = user(db, &format!("m{index}")).await?;
        // `$3::text::core.commission_member_role`: роль приходит строкой,
        // перечисление собирает БД
        let member_id = sqlx::query_scalar!(
            "INSERT INTO core.commission_members (commission_id, user_id, member_role)
             VALUES ($1, $2, $3::text::core.commission_member_role) RETURNING id",
            commission,
            user_id,
            *role
        )
        .fetch_one(db)
        .await?;
        members.push(member_id);
    }
    sqlx::query!(
        "UPDATE core.commissions SET approved_at = core.now() WHERE id = $1",
        commission
    )
    .execute(db)
    .await?;

    let organizer = user(db, "organizer").await?;
    let participant = user(db, "participant").await?;
    let object = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', $1, 'адрес', 12.00) RETURNING id",
        format!("G111 объект {tag}")
    )
    .fetch_one(db)
    .await?;

    let tender = sqlx::query_scalar!(
        "INSERT INTO core.tenders
           (title, status, organizer_id, submission_deadline, opening_at)
         VALUES ($1, 'qualification', $2,
                 core.now() + interval '1 hour', core.now() + interval '2 hours')
         RETURNING id",
        format!("G111 тендер {tag}"),
        organizer
    )
    .fetch_one(db)
    .await?;

    let lot = sqlx::query_scalar!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                base_rate_monthly, guarantee_fee, rate_calculation)
         VALUES ($1, 1, $2, 'офис', 12, 50000.00, 50000.00, '{}'::jsonb) RETURNING id",
        tender,
        object
    )
    .fetch_one(db)
    .await?;

    let application = sqlx::query_scalar!(
        "INSERT INTO core.applications
           (tender_id, lot_id, participant_id, applicant_kind, applicant_details)
         VALUES ($1, $2, $3, 'legal_entity', $4::jsonb) RETURNING id",
        tender,
        lot,
        participant,
        serde_json::json!({ "name": "G111 участник" })
    )
    .fetch_one(db)
    .await?;

    // Прием закрывается уже после подачи: заявка, вставленная задним
    // числом, обходила бы INV-037 (сторож `check_application_deadline`)
    sqlx::query!(
        "UPDATE core.tenders
         SET submission_deadline = core.now() - interval '1 hour',
             opening_at = core.now() - interval '30 minutes'
         WHERE id = $1",
        tender
    )
    .execute(db)
    .await?;

    let meeting = sqlx::query_scalar!(
        "INSERT INTO core.sessions_meetings (tender_id, commission_id, kind, scheduled_at)
         VALUES ($1, $2, 'qualification', core.now() - interval '20 minutes') RETURNING id",
        tender,
        commission
    )
    .fetch_one(db)
    .await?;

    // Явка: председательствующий и пять членов - две трети от семи (п. 12)
    for (index, member) in members.iter().take(6).enumerate() {
        sqlx::query!(
            "INSERT INTO core.meeting_attendance (meeting_id, member_id, present, chairing)
             VALUES ($1, $2, true, $3)",
            meeting,
            member,
            index == 0
        )
        .execute(db)
        .await?;
    }
    sqlx::query!(
        "UPDATE core.sessions_meetings SET opened_at = core.now() WHERE id = $1",
        meeting
    )
    .execute(db)
    .await?;

    // Председатель заведен первым: без него состав не утвердился бы,
    // и отсутствие строки здесь - поломка фикстуры, а не отказ правила
    let Some(&member) = members.first() else {
        return Err(sqlx::Error::RowNotFound);
    };
    Ok(Fixture {
        actor: organizer,
        tender,
        application,
        meeting,
        member,
    })
}

/// Уборка той же процедурой, что и вкладка «Данные» кабинета админа (М15):
/// голос обычным DELETE не удаляется - право отобрано у роли приложения.
async fn cleanup(db: &tou_db::Db, f: &Fixture) {
    let _ = tou_db::purge::purge(
        db,
        f.actor,
        tou_db::purge::PurgeScope::Tenders,
        Some(&[f.tender]),
    )
    .await;
}

struct StoredVote {
    value: String,
    dissent: Option<String>,
    count: i64,
}

async fn stored_vote(db: &tou_db::Db, f: &Fixture) -> Result<StoredVote, sqlx::Error> {
    // `!` у value - это `::text`, который планировщик считает потенциально NULL
    let row = sqlx::query!(
        r#"SELECT value::text AS "value!", dissent,
                  count(*) OVER () AS "count!"
           FROM core.votes WHERE application_id = $1 AND member_id = $2"#,
        f.application,
        f.member
    )
    .fetch_one(db)
    .await?;
    Ok(StoredVote {
        value: row.value,
        dissent: row.dissent,
        count: row.count,
    })
}

/// FR-1103, INV-1103 (п. 13–14): второй голос того же члена по той же
/// заявке отклоняется причиной перечня, а первый остается нетронутым.
#[tokio::test]
async fn a_second_vote_is_refused_and_the_first_one_stands() {
    let db = require_db!();
    let f = fixture(&db).await.expect("фикстура");

    commission::cast_vote(
        &db,
        f.actor,
        f.meeting,
        f.application,
        f.member,
        Vote::For,
        Some("особое мнение по заявке"),
    )
    .await
    .expect("первый голос принимается");

    let err = commission::cast_vote(
        &db,
        f.actor,
        f.meeting,
        f.application,
        f.member,
        Vote::Against,
        None,
    )
    .await
    .expect_err("повторный голос обязан быть отклонен");

    assert!(
        matches!(
            err,
            CommissionError::Rejected(ref rejection)
                if rejection.rule() == RuleViolation::VoteAlreadyCast
        ),
        "ожидался отказ `vote_already_cast`, получено: {err}"
    );

    let stored = stored_vote(&db, &f).await.expect("голос по заявке");
    assert_eq!(stored.count, 1, "голос по заявке остается один");
    assert_eq!(stored.value, "for", "первый голос не переписан");
    assert_eq!(
        stored.dissent.as_deref(),
        Some("особое мнение по заявке"),
        "особое мнение - часть того же акта и тоже не переписывается (п. 13–14)"
    );

    cleanup(&db, &f).await;
}

/// INV-1103: нижний рубеж - сама таблица. Правка голоса мимо обработчика
/// отбивается триггером, а не проходит потому, что путь другой.
#[tokio::test]
async fn the_table_refuses_to_rewrite_a_cast_vote() {
    let db = require_db!();
    let f = fixture(&db).await.expect("фикстура");

    commission::cast_vote(
        &db,
        f.actor,
        f.meeting,
        f.application,
        f.member,
        Vote::For,
        None,
    )
    .await
    .expect("первый голос принимается");

    let value = sqlx::query!(
        "UPDATE core.votes SET value = 'against'
         WHERE application_id = $1 AND member_id = $2",
        f.application,
        f.member
    )
    .execute(&db)
    .await
    .expect_err("правка голоса обязана быть отклонена")
    .to_string();
    assert!(
        value.contains("INV-1103"),
        "ожидали отказ INV-1103, получили: {value}"
    );

    let dissent = sqlx::query!(
        "UPDATE core.votes SET dissent = 'подменено'
         WHERE application_id = $1 AND member_id = $2",
        f.application,
        f.member
    )
    .execute(&db)
    .await
    .expect_err("подмена особого мнения обязана быть отклонена")
    .to_string();
    assert!(
        dissent.contains("INV-1103"),
        "ожидали отказ INV-1103, получили: {dissent}"
    );

    let stored = stored_vote(&db, &f).await.expect("голос по заявке");
    assert_eq!(stored.value, "for");
    assert_eq!(stored.dissent, None);

    cleanup(&db, &f).await;
}
