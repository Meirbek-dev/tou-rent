//! Уклонение победителя и реестр уклонистов против живой БД
//! (T26, FR-903, FR-505).
//!
//! Проверяется то, что делает БД сама: условия признания уклонения
//! (п. 110–111, 116), его следствия - удержание взноса и прекращение
//! договора, право участника № 2 на договор (п. 117) и автоматическое
//! отклонение заявок уклонистов в будущих тендерах (п. 52.4, 120).
//!
//! Подключение - TESTKIT_DATABASE_URL (A-021).

use rust_decimal::Decimal;
use sqlx::Acquire as _;
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
                eprintln!("SKIP: TESTKIT_DATABASE_URL не задан - уклонение не проверялось");
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
    lot_id: Uuid,
    winner_id: Uuid,
    winner_application_id: Uuid,
    contract_id: Uuid,
}

/// Тендер с лотом, победителем и переданным ему договором: в этой точке
/// конвейера уклонение и возможно (п. 110–111).
async fn fixture(tx: &mut sqlx::PgConnection, handed: bool) -> Result<Fixture, sqlx::Error> {
    let tag = Uuid::now_v7().simple().to_string();

    let organizer_email = format!("t26-org-{tag}@tou.test");
    let organizer = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т26 организатор', now()) RETURNING id",
        organizer_email
    )
    .fetch_one(&mut *tx)
    .await?;

    let winner_email = format!("t26-win-{tag}@tou.test");
    let winner_id = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т26 победитель', now()) RETURNING id",
        winner_email
    )
    .fetch_one(&mut *tx)
    .await?;

    let object_id = sqlx::query_scalar!(
        "INSERT INTO core.objects (kind, name, address, area_m2)
         VALUES ('premises', 'Т26 объект', 'адрес', 10.00) RETURNING id"
    )
    .fetch_one(&mut *tx)
    .await?;

    let tender_id = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id, announced_at,
                                   submission_deadline, opening_at)
         VALUES ('Т26 тендер', 'summed_up', $1, now() - interval '30 days',
                 now() + interval '1 hour', now() + interval '2 hours')
         RETURNING id",
        organizer
    )
    .fetch_one(&mut *tx)
    .await?;

    let lot_id = sqlx::query_scalar!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                base_rate_monthly, rate_calculation, guarantee_fee)
         VALUES ($1, 1, $2, 'офис', 12, 50000.00, '{}'::jsonb, 50000.00)
         RETURNING id",
        tender_id,
        object_id
    )
    .fetch_one(&mut *tx)
    .await?;

    let winner_application_id = sqlx::query_scalar!(
        "INSERT INTO core.applications
           (tender_id, lot_id, participant_id, status, applicant_kind, applicant_details)
         VALUES ($1, $2, $3, 'admitted', 'legal_entity', '{\"name\": \"ТОО Т26\"}'::jsonb)
         RETURNING id",
        tender_id,
        lot_id,
        winner_id
    )
    .fetch_one(&mut *tx)
    .await?;

    // Гарантийный взнос победителя поступил (п. 23): при уклонении он и удерживается
    let account_id = sqlx::query_scalar!(
        "INSERT INTO core.ledger_accounts (kind, application_id, owner_user_id)
         VALUES ('participant_fee', $1, $2) RETURNING id",
        winner_application_id,
        winner_id
    )
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query!(
        "INSERT INTO core.ledger_entries (account_id, op, credit, rule_ref, paid_at)
         VALUES ($1, 'receipt_confirmed', 50000.00, 'п. 23, 25', current_date)",
        account_id
    )
    .execute(&mut *tx)
    .await?;

    // Передача экземпляра была сборкой SQL строкой; теперь это параметр:
    // NULL в `handed_to_tenant_at` - то же самое, что отсутствие столбца
    // в списке вставки, а запрос стал проверяемым
    let contract_id = sqlx::query_scalar!(
        "INSERT INTO core.contracts
           (tender_id, lot_id, object_id, tenant_id, winner_application_id, place,
            status, monthly_rate, lease_months, drafted_at, handed_to_tenant_at)
         VALUES ($1, $2, $3, $4, $5, 'winner', 'draft', 79750.00, 12, now(),
                 CASE WHEN $6 THEN now() END)
         RETURNING id",
        tender_id,
        lot_id,
        object_id,
        winner_id,
        winner_application_id,
        handed
    )
    .fetch_one(&mut *tx)
    .await?;

    // Прием закрывается после подачи, а не до нее: сторож INV-037
    // (`core.check_application_deadline`) не пускает заявку задним числом,
    // а сценарию нужен следующий этап - подведенные итоги и договор
    sqlx::query!(
        "UPDATE core.tenders
         SET submission_deadline = now() - interval '10 days',
             opening_at = now() - interval '9 days'
         WHERE id = $1",
        tender_id
    )
    .execute(&mut *tx)
    .await?;

    Ok(Fixture {
        tender_id,
        lot_id,
        winner_id,
        winner_application_id,
        contract_id,
    })
}

/// Уклонение по договору: запрос был константой-строкой, а стал макросом -
/// проверяемому запросу нужны аргументы в самом вызове.
macro_rules! evasion {
    ($contract_id:expr, $ground:expr) => {
        sqlx::query!(
            "INSERT INTO core.evasions (contract_id, tender_id, lot_id, application_id,
                                        user_id, place, ground)
             SELECT c.id, c.tender_id, c.lot_id, c.winner_application_id, c.tenant_id,
                    c.place, $2
             FROM core.contracts c WHERE c.id = $1",
            $contract_id,
            $ground
        )
    };
}

/// FR-903 (п. 110–111, 116): уклоняться можно только от переданного
/// и еще не подписанного договора.
#[tokio::test]
async fn fr903_evasion_needs_a_handed_and_unsigned_contract() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");

    let draft = fixture(&mut tx, false).await.expect("непереданный договор");
    let early = rejected!(
        tx,
        evasion!(draft.contract_id, "signing_deadline_missed"),
        "уклонение до передачи экземпляра обязано быть отклонено"
    );
    assert!(early.contains("FR-903"), "{early}");

    let signed = fixture(&mut tx, true).await.expect("переданный договор");
    sqlx::query!(
        "UPDATE core.contracts SET tenant_signed_at = now() WHERE id = $1",
        signed.contract_id
    )
    .execute(&mut *tx)
    .await
    .expect("подпись нанимателя");

    let too_late = rejected!(
        tx,
        evasion!(signed.contract_id, "refused"),
        "уклонение после подписания обязано быть отклонено"
    );
    assert!(too_late.contains("FR-903"), "{too_late}");
}

/// Следствия уклонения (п. 116): взнос удерживается, договор прекращается.
#[tokio::test]
async fn evasion_holds_the_fee_and_terminates_the_contract() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, true).await.expect("фикстура");

    evasion!(f.contract_id, "signing_deadline_missed")
        .execute(&mut *tx)
        .await
        .expect("уклонение");

    let status = sqlx::query_scalar!(
        r#"SELECT status::text AS "status!" FROM core.contracts WHERE id = $1"#,
        f.contract_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("договор");
    assert_eq!(
        status, "terminated",
        "уклонение прекращает договор (п. 116)"
    );

    let book = sqlx::query!(
        r#"SELECT coalesce(sum(e.debit) FILTER (WHERE e.op = 'hold'), 0)::numeric(14,2)
                    AS "held!",
                  coalesce(sum(e.credit - e.debit), 0)::numeric(14,2) AS "balance!"
           FROM core.ledger_entries e
           JOIN core.ledger_accounts acc ON acc.id = e.account_id
           WHERE acc.application_id = $1"#,
        f.winner_application_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("книга");

    assert_eq!(
        book.held,
        Decimal::from(50000),
        "взнос уклонившегося удерживается целиком (п. 116)"
    );
    assert_eq!(
        book.balance,
        Decimal::ZERO,
        "на счете взноса не остается остатка"
    );
}

/// Право на договор переходит к участнику № 2 только после уклонения
/// победителя (п. 117), и живой договор по лоту остается один.
#[tokio::test]
async fn runner_up_contract_follows_the_winner_evasion() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, true).await.expect("фикстура");

    let second_email = format!("t26-second-{}@tou.test", Uuid::now_v7().simple());
    let second_id = sqlx::query_scalar!(
        "INSERT INTO core.users (email, password_hash, full_name, email_confirmed_at)
         VALUES ($1, 'x', 'Т26 участник № 2', now()) RETURNING id",
        second_email
    )
    .fetch_one(&mut *tx)
    .await
    .expect("участник № 2");

    macro_rules! runner_up_contract {
        () => {
            sqlx::query!(
                "INSERT INTO core.contracts
                   (tender_id, lot_id, object_id, tenant_id, place, status, monthly_rate,
                    lease_months, drafted_at)
                 SELECT c.tender_id, c.lot_id, c.object_id, $2, 'runner_up', 'draft',
                        75000.00, 12, now()
                 FROM core.contracts c WHERE c.id = $1",
                f.contract_id,
                second_id
            )
        };
    }

    let early = rejected!(
        tx,
        runner_up_contract!(),
        "договор с участником № 2 до уклонения обязан быть отклонен"
    );
    assert!(early.contains("FR-903"), "{early}");

    evasion!(f.contract_id, "refused")
        .execute(&mut *tx)
        .await
        .expect("уклонение победителя");

    runner_up_contract!()
        .execute(&mut *tx)
        .await
        .expect("договор с участником № 2 после уклонения");

    let live = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM core.contracts
           WHERE lot_id = $1 AND status NOT IN ('terminated', 'cancelled')"#,
        f.lot_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("договоры лота");
    assert_eq!(live, 1, "по лоту действует один договор");
}

/// FR-505 (п. 52.4, 120): заявка уклониста в будущем тендере отклоняется
/// автоматически, решения комиссии для этого не требуется.
#[tokio::test]
async fn fr505_evader_applications_are_rejected_automatically() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, true).await.expect("фикстура");

    evasion!(f.contract_id, "signing_deadline_missed")
        .execute(&mut *tx)
        .await
        .expect("уклонение");

    let in_registry = sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM core.evader_registry WHERE user_id = $1) AS "found!""#,
        f.winner_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("реестр");
    assert!(in_registry, "уклонившийся попадает в реестр (п. 120)");

    // Новый тендер и лот: те же условия, другой процесс. Тендер именно
    // новый и с открытым приемом - в прежнем прием давно закрыт, и заявка
    // в него не прошла бы вовсе (INV-037), так что правило про уклониста
    // осталось бы непроверенным
    let next_tender = sqlx::query_scalar!(
        "INSERT INTO core.tenders (title, status, organizer_id, announced_at,
                                   submission_deadline, opening_at)
         SELECT 'Т26 тендер (следующий)', 'accepting', t.organizer_id, now(),
                now() + interval '10 days', now() + interval '11 days'
         FROM core.tenders t WHERE t.id = $1
         RETURNING id",
        f.tender_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("следующий тендер");

    let next_lot = sqlx::query_scalar!(
        "INSERT INTO core.lots (tender_id, seq, object_id, purpose, lease_months,
                                base_rate_monthly, rate_calculation, guarantee_fee)
         SELECT $2, 1, l.object_id, l.purpose, l.lease_months,
                l.base_rate_monthly, l.rate_calculation, l.guarantee_fee
         FROM core.lots l WHERE l.id = $1
         RETURNING id",
        f.lot_id,
        next_tender
    )
    .fetch_one(&mut *tx)
    .await
    .expect("лот следующего тендера");

    let application = sqlx::query!(
        r#"INSERT INTO core.applications
             (tender_id, lot_id, participant_id, applicant_kind, applicant_details)
           VALUES ($1, $2, $3, 'legal_entity', '{"name": "ТОО Т26"}'::jsonb)
           RETURNING status::text AS "status!", rejection_reason"#,
        next_tender,
        next_lot,
        f.winner_id
    )
    .fetch_one(&mut *tx)
    .await
    .expect("заявка уклониста");

    assert_eq!(
        application.status, "rejected",
        "заявка уклониста отклоняется (п. 52.4)"
    );
    assert_eq!(
        application.rejection_reason.as_deref(),
        Some("evader"),
        "основание - п. 52.4"
    );
}

/// Уклонение - юридический факт: переписать и удалить его нельзя.
#[tokio::test]
async fn evasions_cannot_be_rewritten() {
    let db = require_db!();
    let mut tx = db.begin().await.expect("begin");
    let f = fixture(&mut tx, true).await.expect("фикстура");

    evasion!(f.contract_id, "refused")
        .execute(&mut *tx)
        .await
        .expect("уклонение");

    let deleted = rejected!(
        tx,
        sqlx::query!(
            "DELETE FROM core.evasions WHERE contract_id = $1",
            f.contract_id
        ),
        "удаление уклонения обязано быть отклонено"
    );
    assert!(
        deleted.contains("FR-903") || deleted.contains("permission denied"),
        "уклонение удаляться не должно: {deleted}"
    );

    let updated = rejected!(
        tx,
        sqlx::query!(
            "UPDATE core.evasions SET ground = 'refused' WHERE contract_id = $1",
            f.contract_id
        ),
        "правка уклонения обязана быть отклонена"
    );
    assert!(
        updated.contains("FR-903") || updated.contains("permission denied"),
        "уклонение переписываться не должно: {updated}"
    );

    // Повторное уклонение по тому же договору - не второй факт
    let twice = rejected!(
        tx,
        evasion!(f.contract_id, "refused"),
        "повторное уклонение обязано быть отклонено"
    );
    assert!(twice.contains("evasions_contract_id_key"), "{twice}");
}
