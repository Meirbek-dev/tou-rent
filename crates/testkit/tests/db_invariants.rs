//! Интеграционные тесты инвариантов БД (критерий Т8): INV-037 (журнал после
//! дедлайна) и INV-040 (цены запечатаны до вскрытия).
//!
//! Подключение - TESTKIT_DATABASE_URL (стендовая или CI-сервисная БД);
//! без переменной тесты проходят вхолостую с предупреждением: dev-хост
//! не собирает Rust, а внутри контейнера cargo test недоступен docker
//! для testcontainers (A-021). Каждый тест живет в транзакции с откатом -
//! стендовая база не засоряется.

use uuid::Uuid;

/// `Ok(None)` - переменная не задана: тест обязан пропуститься с сообщением.
/// Паники и печать - только в телах тестов (clippy.toml: *-in-tests).
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
        match try_pool().await.expect("TESTKIT_DATABASE_URL: подключение не удалось") {
            Some(db) => db,
            None => {
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - инварианты БД не проверялись");
                return;
            }
        }
    };
}

struct Fixture {
    tender_id: Uuid,
    lot_id: Uuid,
    participant_id: Uuid,
    stranger_id: Uuid,
}

/// Пользователи, объект, тендер (`accepting`) и лот - прямыми вставками:
/// триггер переходов (INV-021) действует только на UPDATE статуса.
///
/// `deadline` - сдвиг дедлайна от `now()` интервалом (`'1 day'`, `'-1 hour'`):
/// запрос проверяется по схеме, поэтому кусок SQL в него не подставляется,
/// а приведение `$2::text::interval` делает БД.
async fn fixture(tx: &mut sqlx::PgConnection, deadline: &str) -> Result<Fixture, sqlx::Error> {
    let mut user = async |tag: &str| -> Result<Uuid, sqlx::Error> {
        sqlx::query_scalar!(
            "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
             VALUES ($1::citext, 'x', $2, now()) RETURNING id",
            format!("t8-{tag}-{}@tou.test", Uuid::now_v7().simple()),
            format!("Т8 фикстура {tag}")
        )
        .fetch_one(&mut *tx)
        .await
    };
    let participant_id = user("participant").await?;
    let stranger_id = user("stranger").await?;
    let organizer_id = user("organizer").await?;

    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Т8 фикстура', 'тестовый адрес', 10.00) RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await?;

    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, organizer_id, status, submission_deadline)
         VALUES ('Т8 инварианты', $1, 'accepting', now() + $2::text::interval) RETURNING id",
        organizer_id,
        deadline
    )
    .fetch_one(&mut *tx)
    .await?;

    let lot_id = sqlx::query_scalar!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
            base_rate_monthly, guarantee_fee, rate_calculation)
         VALUES ($1, 1, $2, 'тест', 12, 100.00, 100.00, '{}') RETURNING id",
        tender_id,
        object_id
    )
    .fetch_one(&mut *tx)
    .await?;

    Ok(Fixture {
        tender_id,
        lot_id,
        participant_id,
        stranger_id,
    })
}

async fn insert_journal(
    tx: &mut sqlx::PgConnection,
    tender_id: Uuid,
    actor: Uuid,
) -> Result<i32, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO core.journal_entries (tender_id, entry_kind, actor_id)
         VALUES ($1, 'application_submitted', $2) RETURNING seq",
        tender_id,
        actor
    )
    .fetch_one(&mut *tx)
    .await
}

/// INV-037 (FR-402, п. 36-39): вставка в журнал после дедлайна
/// отклоняется на уровне БД - прием закрывается сам, без участия людей.
#[tokio::test]
async fn inv037_journal_rejects_entry_after_deadline() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");

    let f = fixture(&mut tx, "-1 hour").await.expect("fixture");

    let err = insert_journal(&mut tx, f.tender_id, f.participant_id)
        .await
        .expect_err("вставка после дедлайна обязана быть отклонена");
    assert!(
        err.to_string().contains("INV-037"),
        "ожидали отказ INV-037, получили: {err}"
    );
    // tx откатывается drop'ом - база стенда не изменена
}

/// FR-402 (INV-037): позитивная ветка журнала - до дедлайна seq растет
/// монотонно с единицы в рамках тендера.
#[tokio::test]
async fn inv037_journal_seq_is_monotonic_before_deadline() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");

    let f = fixture(&mut tx, "1 day").await.expect("fixture");

    let first = insert_journal(&mut tx, f.tender_id, f.participant_id)
        .await
        .expect("первая запись");
    let second = insert_journal(&mut tx, f.tender_id, f.participant_id)
        .await
        .expect("вторая запись");
    assert_eq!((first, second), (1, 2), "seq монотонен в рамках тендера");
}

/// Вскрытие раньше назначенного времени заседания отклоняет CHECK
/// `opened_not_before_meeting` (FR-403/FR-501: «не ранее времени заседания»).
#[tokio::test]
async fn opening_before_meeting_time_is_rejected() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");

    let f = fixture(&mut tx, "1 day").await.expect("fixture");

    sqlx::query!(
        "UPDATE core.tenders SET submission_deadline = now() + interval '1 day',
         opening_at = now() + interval '2 days' WHERE id = $1",
        f.tender_id
    )
    .execute(&mut *tx)
    .await
    .expect("set opening_at");

    // Заседание открыто (FR-1102) - проверяем именно время вскрытия
    opened_meeting(&mut tx, f.tender_id)
        .await
        .expect("открытое заседание");

    let err = sqlx::query!(
        "UPDATE core.tenders SET status = 'qualification', opened_at = now() WHERE id = $1",
        f.tender_id
    )
    .execute(&mut *tx)
    .await
    .expect_err("вскрытие раньше заседания обязано быть отклонено");
    assert!(
        err.to_string().contains("opened_not_before_meeting"),
        "ожидали отказ CHECK opened_not_before_meeting, получили: {err}"
    );
}

/// Приглашение на торги - один и тот же запрос в двух тестах: общий текст
/// остается в одном месте, но проверяется по схеме (макрос разворачивается
/// в сам вызов). `$2::uuid::text` - тендер кладется в payload строкой.
macro_rules! invitation {
    ($user:expr, $tender:expr) => {
        sqlx::query_scalar!(
            "INSERT INTO core.notifications (user_id, kind, payload)
             VALUES ($1, 'auction_invitation', jsonb_build_object('tender_id', $2::uuid::text))
             RETURNING id",
            $user,
            $tender
        )
    };
}

/// FR-1302: процессуальное уведомление - доказательная база. Вставка в
/// `core.notifications` порождает запись `audit.log` (триггер INV-AUDIT)
/// с актором-отправителем из GUC `app.user_id` и временем события.
#[tokio::test]
async fn notification_insert_is_audited() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");

    let f = fixture(&mut tx, "1 day").await.expect("fixture");

    // stranger играет секретаря-отправителя; получатель - участник
    act_as(&mut tx, f.stranger_id).await.expect("guc");
    let id = invitation!(f.participant_id, f.tender_id)
        .fetch_one(&mut *tx)
        .await
        .expect("уведомление");

    let audited = sqlx::query!(
        "SELECT actor_id, action FROM audit.log
         WHERE table_name = 'core.notifications' AND row_id = $1
         ORDER BY id DESC LIMIT 1",
        id
    )
    .fetch_optional(&mut *tx)
    .await
    .expect("чтение audit.log");

    let audited = audited.expect("audit-событие уведомления обязано существовать");
    assert_eq!(audited.action, "INSERT");
    assert_eq!(
        audited.actor_id,
        Some(f.stranger_id),
        "актор события - отправитель"
    );
}

/// Открытое заседание комиссии по тендеру: с T19 вскрытие возможно только
/// на нем (FR-1102, п. 12, 50), поэтому оно нужно и тем тестам, что проверяют
/// другие правила вскрытия. Состав - минимальный допустимый (1 + 1 + 5).
async fn opened_meeting(tx: &mut sqlx::PgConnection, tender_id: Uuid) -> Result<Uuid, sqlx::Error> {
    let commission_id = sqlx::query_scalar!(
        "INSERT INTO core.commissions (name, valid_from, valid_until)
         VALUES ($1, current_date, current_date + interval '1 year') RETURNING id",
        format!("Фикстура комиссии {}", Uuid::now_v7().simple())
    )
    .fetch_one(&mut *tx)
    .await?;

    let roles = [
        "chairman", "deputy", "member", "member", "member", "member", "member",
    ];
    let mut members = Vec::new();
    for (index, role) in roles.iter().enumerate() {
        let user_id = sqlx::query_scalar!(
            "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
             VALUES ($1::citext, 'x', $2, now()) RETURNING id",
            format!("fixture-m{index}-{}@tou.test", Uuid::now_v7().simple()),
            format!("Фикстура член {index}")
        )
        .fetch_one(&mut *tx)
        .await?;
        // `$3::text::core.commission_member_role`: роль приходит строкой,
        // приведение к перечислению делает БД
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

    sqlx::query!(
        "UPDATE core.commissions SET approved_at = now() WHERE id = $1",
        commission_id
    )
    .execute(&mut *tx)
    .await?;

    let meeting_id = sqlx::query_scalar!(
        "INSERT INTO core.sessions_meetings (tender_id, commission_id, kind, scheduled_at)
         VALUES ($1, $2, 'qualification', now()) RETURNING id",
        tender_id,
        commission_id
    )
    .fetch_one(&mut *tx)
    .await?;

    for (index, member) in members.iter().enumerate() {
        sqlx::query!(
            "INSERT INTO core.meeting_attendance (meeting_id, member_id, present, chairing)
             VALUES ($1, $2, true, $3)",
            meeting_id,
            *member,
            index == 0
        )
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query!(
        "UPDATE core.sessions_meetings SET opened_at = now() WHERE id = $1",
        meeting_id
    )
    .execute(&mut *tx)
    .await?;

    Ok(meeting_id)
}

/// `fetch_one`, а не `execute`: `set_config` возвращает столбец.
async fn act_as(tx: &mut sqlx::PgConnection, user: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "SELECT set_config('app.user_id', $1, true)",
        user.to_string()
    )
    .fetch_one(tx)
    .await
    .map(|_| ())
}

async fn visible_prices(
    tx: &mut sqlx::PgConnection,
    application_id: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "visible!" FROM core.price_proposals WHERE application_id = $1"#,
        application_id
    )
    .fetch_one(tx)
    .await
}

/// INV-040: до вскрытия цену не видит никто, включая участника-владельца;
/// после `opened_at` ее видят допущенные политикой пользователи (FR-403).
/// Роль пула tou_rent_app не имеет BYPASSRLS (A-011).
#[tokio::test]
async fn inv040_price_sealed_until_opening() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");

    let f = fixture(&mut tx, "1 day").await.expect("fixture");

    // Заявка и цена - от имени участника (RLS INSERT: только свои)
    act_as(&mut tx, f.participant_id).await.expect("guc");
    let application_id = sqlx::query_scalar!(
        "INSERT INTO core.applications
           (tender_id, lot_id, participant_id, applicant_kind, applicant_details)
         VALUES ($1, $2, $3, 'individual', '{}') RETURNING id",
        f.tender_id,
        f.lot_id,
        f.participant_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("заявка");
    sqlx::query!(
        "INSERT INTO core.price_proposals (application_id, amount) VALUES ($1, 42.00)",
        application_id
    )
    .execute(&mut *tx)
    .await
    .expect("цена");

    // Чужой пользователь до вскрытия: 0 строк (INV-040)
    act_as(&mut tx, f.stranger_id).await.expect("guc");
    let sealed = visible_prices(&mut tx, application_id)
        .await
        .expect("count");
    assert_eq!(sealed, 0, "до вскрытия цена запечатана для не-владельца");

    // Критическое требование ТЗ закрывает цену и от ее владельца до вскрытия.
    act_as(&mut tx, f.participant_id).await.expect("guc");
    let own = visible_prices(&mut tx, application_id)
        .await
        .expect("count");
    assert_eq!(own, 0, "до вскрытия цена запечатана и для владельца");

    // Вскрытие состоялось - цена открыта (на открытом заседании, FR-1102)
    opened_meeting(&mut tx, f.tender_id)
        .await
        .expect("открытое заседание");
    sqlx::query!(
        "UPDATE core.tenders SET opened_at = now() WHERE id = $1",
        f.tender_id
    )
    .execute(&mut *tx)
    .await
    .expect("opened_at");
    act_as(&mut tx, f.stranger_id).await.expect("guc");
    let opened = visible_prices(&mut tx, application_id)
        .await
        .expect("count");
    assert_eq!(opened, 1, "после вскрытия цена видна (FR-403)");
}

#[tokio::test]
async fn fr206_guarantee_fee_equals_monthly_rate() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, "1 day").await.expect("fixture");
    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'FR-206', 'test', 10) RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await
    .expect("object");

    let error = sqlx::query!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
           base_rate_monthly, guarantee_fee, rate_calculation)
         VALUES ($1, 2, $2, 'test', 12, 100, 99, '{}')",
        f.tender_id,
        object_id
    )
    .execute(&mut *tx)
    .await
    .expect_err("разные ставка и гарантийный взнос должны отклоняться");

    assert!(
        error
            .to_string()
            .contains("lots_guarantee_fee_equals_monthly_rate")
    );
}

#[tokio::test]
async fn fr303_tender_without_lots_cannot_be_published() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let organizer_id = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1::citext, 'x', 'FR-303', now()) RETURNING id",
        format!("fr303-{}@tou.test", Uuid::now_v7().simple())
    )
    .fetch_one(&mut *tx)
    .await
    .expect("organizer");
    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders
           (title, organizer_id, submission_deadline, opening_at)
         VALUES ('FR-303', $1, now() + interval '10 days', now() + interval '11 days')
         RETURNING id",
        organizer_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("tender");

    let error = sqlx::query!(
        "UPDATE core.tenders SET status = 'announced' WHERE id = $1",
        tender_id
    )
    .execute(&mut *tx)
    .await
    .expect_err("публикация без лотов должна отклоняться");
    assert!(error.to_string().contains("FR-303"));
}

#[tokio::test]
async fn fr1302_auction_invitation_is_unique() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, "1 day").await.expect("fixture");

    invitation!(f.participant_id, f.tender_id)
        .fetch_one(&mut *tx)
        .await
        .expect("first invitation");
    let error = invitation!(f.participant_id, f.tender_id)
        .fetch_one(&mut *tx)
        .await
        .expect_err("повторное приглашение должно отклоняться UNIQUE-индексом");

    assert!(
        error
            .to_string()
            .contains("notifications_auction_invitation_once_idx")
    );
}

#[tokio::test]
async fn inv066_extension_is_exactly_fifteen_minutes() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, "1 day").await.expect("fixture");
    let auction_id = sqlx::query_scalar!(
        "INSERT INTO core.auctions (lot_id, starting_bid, bid_step)
         VALUES ($1, 100, 5) RETURNING id",
        f.lot_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("auction");
    sqlx::query!(
        "UPDATE core.auctions
         SET status = 'running', started_at = now(), ends_at = now() + interval '1 hour'
         WHERE id = $1",
        auction_id
    )
    .execute(&mut *tx)
    .await
    .expect("start auction");

    let error = sqlx::query!(
        "UPDATE core.auctions SET ends_at = ends_at + interval '14 minutes' WHERE id = $1",
        auction_id
    )
    .execute(&mut *tx)
    .await
    .expect_err("продление не на 15 минут должно отклоняться");
    assert!(error.to_string().contains("INV-066"));
}

/// Идущие торги с одной допущенной заявкой участника - фикстура Т11.
/// `window` задает время окончания относительно now() уже при старте:
/// сдвинуть `ends_at` позже нельзя иначе как продлением на 15 минут (INV-066).
///
/// Суммы и окно приходят строками и приводятся в БД (`$2::text::numeric`,
/// `$2::text::interval`): текст запроса остается литералом и проверяется
/// по схеме, а значения от этого не меняются.
async fn running_auction(
    tx: &mut sqlx::PgConnection,
    f: &Fixture,
    starting_bid: &str,
    step: &str,
    window: &str,
) -> Result<(Uuid, Uuid), sqlx::Error> {
    let application_id = sqlx::query_scalar!(
        "INSERT INTO core.applications
           (tender_id, lot_id, participant_id, applicant_kind, applicant_details, status)
         VALUES ($1, $2, $3, 'individual', '{}', 'admitted') RETURNING id",
        f.tender_id,
        f.lot_id,
        f.participant_id
    )
    .fetch_one(&mut *tx)
    .await?;

    let auction_id = sqlx::query_scalar!(
        "INSERT INTO core.auctions (lot_id, starting_bid, bid_step)
         VALUES ($1, $2::text::numeric, $3::text::numeric) RETURNING id",
        f.lot_id,
        starting_bid,
        step
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query!(
        "UPDATE core.auctions
         SET status = 'running', started_at = now() - interval '2 hours',
             ends_at = now() + $2::text::interval
         WHERE id = $1",
        auction_id,
        window
    )
    .execute(&mut *tx)
    .await?;

    Ok((auction_id, application_id))
}

async fn place_bid(
    tx: &mut sqlx::PgConnection,
    auction_id: Uuid,
    application_id: Uuid,
    amount: &str,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO core.bids (id, auction_id, application_id, amount)
         VALUES ($1, $2, $3, $4::text::numeric) RETURNING id",
        Uuid::now_v7(),
        auction_id,
        application_id,
        amount
    )
    .fetch_one(tx)
    .await
}

/// Q-019: даже обход HTTP не позволяет открыть комнату с процентом ниже 5.
#[tokio::test]
async fn inv063_bid_step_percent_below_five_is_rejected() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, "1 day").await.expect("fixture");

    let error = sqlx::query!(
        "INSERT INTO core.auctions
           (lot_id, starting_bid, bid_step_percent, bid_step)
         VALUES ($1, 55000, 4.99, 2750)",
        f.lot_id
    )
    .execute(&mut *tx)
    .await
    .expect_err("процент шага ниже 5 должен отклоняться CHECK-ограничением");

    assert!(
        error
            .to_string()
            .contains("auctions_bid_step_percent_minimum"),
        "ожидали отказ минимального процента шага, получили: {error}"
    );
}

/// INV-063 (п. 63): ставка принимается от «максимум + шаг»; до первой ставки
/// максимумом служит стартовая ставка. Отказ БД обрывает транзакцию, поэтому
/// принимаемая ставка проверяется первой.
#[tokio::test]
async fn inv063_bid_below_maximum_plus_step_is_rejected() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, "1 day").await.expect("fixture");
    let (auction_id, application_id) = running_auction(&mut tx, &f, "55000", "2750", "1 hour")
        .await
        .expect("running auction");

    place_bid(&mut tx, auction_id, application_id, "57750")
        .await
        .expect("ставка ровно на шаг выше старта принимается");

    let error = place_bid(&mut tx, auction_id, application_id, "60499.99")
        .await
        .expect_err("ставка ниже «максимум + шаг» обязана быть отклонена");
    assert!(
        error.to_string().contains("INV-063"),
        "ожидали отказ INV-063, получили: {error}"
    );
}

/// INV-066 (п. 66): после истечения времени торгов ставки не принимаются -
/// комнату закрывают часы сервера, а не клиент.
#[tokio::test]
async fn inv066_bid_after_deadline_is_rejected() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, "1 day").await.expect("fixture");
    let (auction_id, application_id) = running_auction(&mut tx, &f, "55000", "2750", "-1 minute")
        .await
        .expect("running auction");

    let error = place_bid(&mut tx, auction_id, application_id, "57750")
        .await
        .expect_err("ставка после окончания обязана быть отклонена");
    assert!(
        error.to_string().contains("INV-066"),
        "ожидали отказ INV-066, получили: {error}"
    );
}

/// FR-606: победитель фиксируется только вместе с реальной ставкой этих
/// торгов - «назначить» сумму мимо ленты нельзя.
#[tokio::test]
async fn fr606_winner_must_match_a_bid_of_this_auction() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, "1 day").await.expect("fixture");
    let (auction_id, application_id) = running_auction(&mut tx, &f, "55000", "2750", "1 hour")
        .await
        .expect("running auction");
    place_bid(&mut tx, auction_id, application_id, "57750")
        .await
        .expect("ставка");

    sqlx::query!(
        "UPDATE core.auctions
         SET status = 'finished', finished_at = now(),
             winner_application_id = $2, winner_amount = 57750
         WHERE id = $1",
        auction_id,
        application_id
    )
    .execute(&mut *tx)
    .await
    .expect("победитель со своей ставкой фиксируется");

    let error = sqlx::query!(
        "UPDATE core.auctions SET winner_amount = 99000 WHERE id = $1",
        auction_id
    )
    .execute(&mut *tx)
    .await
    .expect_err("сумма победителя без ставки должна отклоняться");
    assert!(
        error.to_string().contains("FR-606"),
        "ожидали отказ FR-606, получили: {error}"
    );
}

/// FR-404 (п. 43-45): отзыв заявки до дедлайна фиксируется в журнале
/// отдельной записью - журнал остается полной историей приема.
#[tokio::test]
async fn fr404_withdrawal_before_deadline_is_recorded() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");

    let f = fixture(&mut tx, "1 day").await.expect("fixture");
    let application_id = submitted_application(&mut tx, &f).await.expect("заявка");

    withdraw(&mut tx, &f, application_id)
        .await
        .expect("отзыв до дедлайна разрешен");

    let status = sqlx::query_scalar!(
        r#"SELECT status::text AS "status!" FROM core.applications WHERE id = $1"#,
        application_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("статус");
    assert_eq!(status, "withdrawn");

    let recorded = sqlx::query_scalar!(
        r#"SELECT count(*) AS "recorded!" FROM core.journal_entries
            WHERE application_id = $1 AND entry_kind = 'application_withdrawn'"#,
        application_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("журнал");
    assert_eq!(recorded, 1, "отзыв обязан попасть в журнал (Прил. 12)");
}

/// FR-404 (п. 45): после дедлайна отзыв невозможен. Рубеж - тот же
/// журнальный триггер INV-037: запись об отзыве не проходит, и вместе
/// с ней откатывается смена статуса - заявка остается поданной.
#[tokio::test]
async fn fr404_withdrawal_after_deadline_is_rejected() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");

    let f = fixture(&mut tx, "1 day").await.expect("fixture");
    let application_id = submitted_application(&mut tx, &f).await.expect("заявка");

    // Прием закрылся, пока заявка лежала поданной
    sqlx::query!(
        "UPDATE core.tenders SET submission_deadline = now() - interval '1 hour' WHERE id = $1",
        f.tender_id
    )
    .execute(&mut *tx)
    .await
    .expect("дедлайн в прошлом");

    let err = withdraw(&mut tx, &f, application_id)
        .await
        .expect_err("отзыв после дедлайна обязан быть отклонен");
    assert!(
        err.to_string().contains("INV-037"),
        "ожидали отказ INV-037, получили: {err}"
    );
}

/// Поданная заявка участника из фикстуры.
async fn submitted_application(
    tx: &mut sqlx::PgConnection,
    f: &Fixture,
) -> Result<Uuid, sqlx::Error> {
    act_as(&mut *tx, f.participant_id).await?;
    sqlx::query_scalar!(
        "INSERT INTO core.applications
           (tender_id, lot_id, participant_id, applicant_kind, applicant_details)
         VALUES ($1, $2, $3, 'individual', '{}') RETURNING id",
        f.tender_id,
        f.lot_id,
        f.participant_id
    )
    .fetch_one(tx)
    .await
}

/// Отзыв тем же составом действий, что и слой данных: смена статуса
/// и запись журнала одной транзакцией (`tou_db::applications::withdraw`).
async fn withdraw(
    tx: &mut sqlx::PgConnection,
    f: &Fixture,
    application_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE core.applications
            SET status = 'withdrawn', withdrawn_at = core.now()
          WHERE id = $1 AND participant_id = $2 AND status = 'submitted'",
        application_id,
        f.participant_id
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "INSERT INTO core.journal_entries (tender_id, entry_kind, application_id, actor_id)
         VALUES ($1, 'application_withdrawn', $2, $3)",
        f.tender_id,
        application_id,
        f.participant_id
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}
