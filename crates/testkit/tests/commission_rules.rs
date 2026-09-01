//! Правила тендерной комиссии против живой БД (T19, FR-1101–1104).
//!
//! Проверяется то, что закреплено триггерами: состав (нечетный, ≥7, один
//! председатель и один заместитель), кворум ⅔ с председательствующим,
//! право голоса (присутствие, отвод, резервный) и закрытие материалов лота
//! отведенному (RLS). Подключение - TESTKIT_DATABASE_URL (A-021);
//! каждый тест живет в транзакции с откатом.

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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - правила комиссии не проверялись");
                return;
            }
        }
    };
}

struct Fixture {
    commission_id: Uuid,
    /// Члены в порядке: председатель, заместитель, пять членов, резервный
    members: Vec<Uuid>,
    tender_id: Uuid,
    lot_id: Uuid,
    application_id: Uuid,
    participant_id: Uuid,
}

async fn user(tx: &mut sqlx::PgConnection, tag: &str) -> Result<Uuid, sqlx::Error> {
    let email = format!("t19-{tag}-{}@tou.test", Uuid::now_v7().simple());
    let full_name = format!("Т19 {tag}");
    sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', $2, now()) RETURNING id",
        email,
        full_name
    )
    .fetch_one(tx)
    .await
}

/// Комиссия из семи голосующих и одного резервного, тендер на допуске
/// с одной поданной заявкой и ценой.
async fn fixture(tx: &mut sqlx::PgConnection) -> Result<Fixture, sqlx::Error> {
    let name = format!("Т19 комиссия {}", Uuid::now_v7().simple());
    let commission_id = sqlx::query_scalar!(
        "INSERT INTO core.commissions (name, valid_from, valid_until)
         VALUES ($1, current_date, current_date + interval '1 year') RETURNING id",
        name
    )
    .fetch_one(&mut *tx)
    .await?;

    let roles = [
        "chairman", "deputy", "member", "member", "member", "member", "member", "reserve",
    ];
    let mut members = Vec::new();
    for (index, role) in roles.iter().enumerate() {
        let user_id = user(&mut *tx, &format!("m{index}")).await?;
        // `$3::text::core.commission_member_role`: роль приходит строкой,
        // перечисление собирает БД
        let member_id = sqlx::query_scalar!(
            "INSERT INTO core.commission_members (commission_id, user_id, member_role)
             VALUES ($1, $2, $3::text::core.commission_member_role) RETURNING id",
            commission_id,
            user_id,
            *role
        )
        .fetch_one(&mut *tx)
        .await?;
        members.push(member_id);
    }

    let participant_id = user(&mut *tx, "participant").await?;
    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Т19 объект', 'адрес', 10.00) RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await?;
    let organizer_id = user(&mut *tx, "organizer").await?;
    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders
           (title, status, organizer_id, submission_deadline, opening_at)
         VALUES ('Т19 тендер', 'qualification', $1,
                 now() + interval '1 hour', now() + interval '2 hours')
         RETURNING id",
        organizer_id
    )
    .fetch_one(&mut *tx)
    .await?;
    let lot_id = sqlx::query_scalar!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                base_rate_monthly, guarantee_fee, rate_calculation)
         VALUES ($1, 1, $2, 'офис', 12, 50000.00, 50000.00, '{}'::jsonb) RETURNING id",
        tender_id,
        object_id
    )
    .fetch_one(&mut *tx)
    .await?;
    let application_id = sqlx::query_scalar!(
        "INSERT INTO core.applications
           (tender_id, lot_id, participant_id, applicant_kind, applicant_details)
         VALUES ($1, $2, $3, 'legal_entity', '{\"name\": \"Т19 участник\"}'::jsonb)
         RETURNING id",
        tender_id,
        lot_id,
        participant_id
    )
    .fetch_one(&mut *tx)
    .await?;

    // Прием закрывается уже после подачи: заявка, вставленная задним числом,
    // обходила бы INV-037 (сторож `core.check_application_deadline`), а
    // сценарию нужен как раз следующий этап - работа комиссии
    sqlx::query!(
        "UPDATE core.tenders
         SET submission_deadline = now() - interval '1 hour',
             opening_at = now() - interval '30 minutes'
         WHERE id = $1",
        tender_id
    )
    .execute(&mut *tx)
    .await?;

    let participant = participant_id.to_string();
    sqlx::query!("SELECT set_config('app.user_id', $1, true)", participant)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query!(
        "INSERT INTO core.price_proposals (application_id, amount) VALUES ($1, 55000.00)",
        application_id
    )
    .execute(&mut *tx)
    .await?;

    Ok(Fixture {
        commission_id,
        members,
        tender_id,
        lot_id,
        application_id,
        participant_id,
    })
}

async fn approve(tx: &mut sqlx::PgConnection, commission_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE core.commissions SET approved_at = now() WHERE id = $1",
        commission_id
    )
    .execute(tx)
    .await
    .map(|_| ())
}

async fn create_meeting(tx: &mut sqlx::PgConnection, f: &Fixture) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO core.sessions_meetings (tender_id, commission_id, kind, scheduled_at)
         VALUES ($1, $2, 'qualification', now()) RETURNING id",
        f.tender_id,
        f.commission_id
    )
    .fetch_one(tx)
    .await
}

async fn attend(
    tx: &mut sqlx::PgConnection,
    meeting_id: Uuid,
    member_id: Uuid,
    present: bool,
    chairing: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO core.meeting_attendance (meeting_id, member_id, present, chairing)
         VALUES ($1, $2, $3, $4)",
        meeting_id,
        member_id,
        present,
        chairing
    )
    .execute(tx)
    .await
    .map(|_| ())
}

async fn open_meeting(tx: &mut sqlx::PgConnection, meeting_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE core.sessions_meetings SET opened_at = now() WHERE id = $1",
        meeting_id
    )
    .execute(tx)
    .await
    .map(|_| ())
}

/// Сколько ценовых предложений заявки видит пользователь (RLS INV-040 + п. 15).
async fn visible_prices(
    tx: &mut sqlx::PgConnection,
    user: Uuid,
    application_id: Uuid,
) -> Result<i64, sqlx::Error> {
    let user = user.to_string();
    sqlx::query!("SELECT set_config('app.user_id', $1, true)", user)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM core.price_proposals WHERE application_id = $1"#,
        application_id
    )
    .fetch_one(tx)
    .await
}

/// Ожидаемый отказ триггера выполняется внутри SAVEPOINT: PostgreSQL рвет
/// транзакцию на любой ошибке, а тесту нужно продолжать в той же.
async fn rejected(
    tx: &mut sqlx::PgConnection,
    op: impl AsyncFnOnce(&mut sqlx::PgConnection) -> Result<(), sqlx::Error>,
) -> Result<Option<String>, sqlx::Error> {
    use sqlx::Connection as _;

    let mut savepoint = tx.begin().await?;
    let outcome = op(&mut savepoint).await;
    savepoint.rollback().await?;
    Ok(outcome.err().map(|error| error.to_string()))
}

async fn vote(
    tx: &mut sqlx::PgConnection,
    meeting_id: Uuid,
    application_id: Uuid,
    member_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO core.votes (meeting_id, application_id, member_id, value)
         VALUES ($1, $2, $3, 'for')",
        meeting_id,
        application_id,
        member_id
    )
    .execute(tx)
    .await
    .map(|_| ())
}

/// FR-1101 (п. 9–11): состав утверждается, только если он нечетный, не
/// меньше семи, с одним председателем и одним заместителем.
#[tokio::test]
async fn fr1101_composition_is_checked_on_approval() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("fixture");

    approve(&mut tx, f.commission_id)
        .await
        .expect("состав 1 + 1 + 5 голосующих утверждается");

    // Восьмой голосующий делает состав четным
    let extra = user(&mut tx, "extra").await.expect("пользователь");
    let message = rejected(&mut tx, async |c| {
        sqlx::query!(
            "INSERT INTO core.commission_members (commission_id, user_id, member_role)
             VALUES ($1, $2, 'member')",
            f.commission_id,
            extra
        )
        .execute(&mut *c)
        .await?;
        approve(&mut *c, f.commission_id).await
    })
    .await
    .expect("savepoint")
    .expect("четный состав утверждать нельзя");
    assert!(
        message.contains("нечетным"),
        "ожидали отказ по нечетности, получили: {message}"
    );

    // Второй председатель
    let message = rejected(&mut tx, async |c| {
        sqlx::query!(
            "UPDATE core.commission_members SET member_role = 'chairman' WHERE id = $1",
            f.members[2]
        )
        .execute(&mut *c)
        .await?;
        approve(&mut *c, f.commission_id).await
    })
    .await
    .expect("savepoint")
    .expect("двух председателей быть не может");
    assert!(
        message.contains("председатель"),
        "ожидали отказ по председателю, получили: {message}"
    );

    // Меньше семи голосующих
    let message = rejected(&mut tx, async |c| {
        sqlx::query!(
            "DELETE FROM core.commission_members WHERE id = $1",
            f.members[6]
        )
        .execute(&mut *c)
        .await?;
        approve(&mut *c, f.commission_id).await
    })
    .await
    .expect("savepoint")
    .expect("состав меньше семи утверждать нельзя");
    assert!(
        message.contains("не менее 7"),
        "ожидали отказ по численности, получили: {message}"
    );
}

/// FR-1102 (п. 12): заседание не открывается без кворума ⅔ и без
/// председателя или его заместителя.
#[tokio::test]
async fn fr1102_meeting_needs_quorum_and_chair() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("fixture");
    approve(&mut tx, f.commission_id).await.expect("состав");
    let meeting_id = create_meeting(&mut tx, &f).await.expect("заседание");

    // Четверо из семи - кворума нет (требуется 5)
    for member in f.members.iter().take(4) {
        attend(&mut tx, meeting_id, *member, true, false)
            .await
            .expect("явка");
    }
    let message = rejected(&mut tx, async |c| open_meeting(&mut *c, meeting_id).await)
        .await
        .expect("savepoint")
        .expect("без кворума заседание не открывается");
    assert!(
        message.contains("кворума нет"),
        "ожидали отказ по кворуму, получили: {message}"
    );

    // Все семеро явились: кворум есть, но председателя и заместителя нет
    for member in f.members.iter().skip(4).take(3) {
        attend(&mut tx, meeting_id, *member, true, false)
            .await
            .expect("явка");
    }
    let message = rejected(&mut tx, async |c| {
        sqlx::query!(
            "UPDATE core.meeting_attendance SET present = false
             WHERE meeting_id = $1 AND member_id = ANY($2)",
            meeting_id,
            &f.members[0..2]
        )
        .execute(&mut *c)
        .await?;
        open_meeting(&mut *c, meeting_id).await
    })
    .await
    .expect("savepoint")
    .expect("без председательствующего заседание не открывается");
    assert!(
        message.contains("председател"),
        "ожидали отказ по председателю, получили: {message}"
    );

    // Те же семеро, но с председательствующим - заседание открывается
    sqlx::query!(
        "UPDATE core.meeting_attendance SET chairing = true WHERE member_id = $1",
        f.members[0]
    )
    .execute(&mut *tx)
    .await
    .expect("председательствующий");
    open_meeting(&mut tx, meeting_id)
        .await
        .expect("кворум есть - заседание открывается");

    // `!` у обоих столбцов - то же требование, что и раньше: кворум пишет
    // триггер при открытии, и NULL здесь означал бы, что он не сработал
    let quorum = sqlx::query!(
        r#"SELECT quorum_present AS "present!", quorum_required AS "required!"
           FROM core.sessions_meetings WHERE id = $1"#,
        meeting_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("кворум записан");
    assert_eq!((quorum.present, quorum.required), (7, 5));
}

/// FR-1103 (п. 13): голосует только присутствующий член открытого заседания;
/// резервный - лишь заменив отведенного (п. 15).
#[tokio::test]
async fn fr1103_only_present_members_vote() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("fixture");
    approve(&mut tx, f.commission_id).await.expect("состав");
    let meeting_id = create_meeting(&mut tx, &f).await.expect("заседание");

    for (index, member) in f.members.iter().take(7).enumerate() {
        attend(&mut tx, meeting_id, *member, true, index == 0)
            .await
            .expect("явка");
    }
    attend(&mut tx, meeting_id, f.members[7], true, false)
        .await
        .expect("явка резервного");

    // До открытия заседания голосовать нельзя
    let message = rejected(&mut tx, async |c| {
        vote(&mut *c, meeting_id, f.application_id, f.members[1]).await
    })
    .await
    .expect("savepoint")
    .expect("на неоткрытом заседании голосов нет");
    assert!(message.contains("не открыто"), "получили: {message}");

    open_meeting(&mut tx, meeting_id).await.expect("открытие");
    vote(&mut tx, meeting_id, f.application_id, f.members[1])
        .await
        .expect("присутствующий член голосует");

    // Резервный без замены отведенного
    let message = rejected(&mut tx, async |c| {
        vote(&mut *c, meeting_id, f.application_id, f.members[7]).await
    })
    .await
    .expect("savepoint")
    .expect("резервный голосует только вместо отведенного");
    assert!(message.contains("резервный"), "получили: {message}");

    // Отсутствующий член
    let message = rejected(&mut tx, async |c| {
        sqlx::query!(
            "UPDATE core.meeting_attendance SET present = false WHERE member_id = $1",
            f.members[2]
        )
        .execute(&mut *c)
        .await?;
        vote(&mut *c, meeting_id, f.application_id, f.members[2]).await
    })
    .await
    .expect("savepoint")
    .expect("отсутствующий не голосует");
    assert!(message.contains("присутствующий"), "получили: {message}");
}

/// FR-1104 (п. 15): отведенный не голосует по лоту и не видит его материалы;
/// заменивший его резервный - голосует.
#[tokio::test]
async fn fr1104_recused_member_loses_vote_and_lot_materials() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx).await.expect("fixture");
    approve(&mut tx, f.commission_id).await.expect("состав");
    let meeting_id = create_meeting(&mut tx, &f).await.expect("заседание");
    for (index, member) in f.members.iter().take(7).enumerate() {
        attend(&mut tx, meeting_id, *member, true, index == 0)
            .await
            .expect("явка");
    }
    attend(&mut tx, meeting_id, f.members[7], true, false)
        .await
        .expect("явка резервного");
    open_meeting(&mut tx, meeting_id).await.expect("открытие");

    // Вскрытие состоялось - цены открыты всем, кроме отведенного
    sqlx::query!(
        "UPDATE core.tenders SET opened_at = now() WHERE id = $1",
        f.tender_id
    )
    .execute(&mut *tx)
    .await
    .expect("вскрытие");

    let recused_user = sqlx::query_scalar!(
        "SELECT user_id FROM core.commission_members WHERE id = $1",
        f.members[6]
    )
    .fetch_one(&mut *tx)
    .await
    .expect("пользователь члена комиссии");

    assert_eq!(
        visible_prices(&mut tx, recused_user, f.application_id)
            .await
            .expect("цены до отвода"),
        1,
        "до отвода член комиссии видит цену вскрытой заявки"
    );

    sqlx::query!(
        "INSERT INTO core.member_recusals
           (tender_id, member_id, lot_id, reason, replacement_member_id)
         VALUES ($1, $2, $3, 'аффилированность', $4)",
        f.tender_id,
        f.members[6],
        f.lot_id,
        f.members[7]
    )
    .execute(&mut *tx)
    .await
    .expect("отвод");

    assert_eq!(
        visible_prices(&mut tx, recused_user, f.application_id)
            .await
            .expect("цены после отвода"),
        0,
        "FR-1104: отведенному материалы лота закрыты (п. 15)"
    );
    assert_eq!(
        visible_prices(&mut tx, f.participant_id, f.application_id)
            .await
            .expect("цена участника"),
        1,
        "участника отвод чужого члена комиссии не касается"
    );

    let message = rejected(&mut tx, async |c| {
        vote(&mut *c, meeting_id, f.application_id, f.members[6]).await
    })
    .await
    .expect("savepoint")
    .expect("отведенный не голосует");
    assert!(message.contains("отведенный"), "получили: {message}");

    vote(&mut tx, meeting_id, f.application_id, f.members[7])
        .await
        .expect("резервный, заменивший отведенного, голосует");
}
